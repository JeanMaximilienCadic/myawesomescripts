//! Tunnel management: detect, start, stop SSM port-forwarding sessions.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::aws;
use crate::error::{AppError, Result};
use crate::models::{TunnelProcess, TunnelTarget};

// ── Port testing ──────────────────────────────────────────────────────────────

pub fn test_port(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(1),
    )
    .is_ok()
}

/// Probe a tunnel port by sending an HTTP HEAD request and waiting for any
/// response (including RST/EOF). This forces data through the SSM WebSocket
/// to the remote host, so the measured latency is real end-to-end latency,
/// not just the loopback TCP connect to the local SSM plugin socket.
///
/// Returns Some(ms) if the remote responded (even with an error).
/// Returns None if the remote did not respond within 5 s (unreachable).
fn probe_remote(port: u16) -> Option<u64> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let t0 = Instant::now();
    // HTTP HEAD forces any server to respond; non-HTTP services (SSH, postgres…)
    // will either send their banner or RST — both count as "reachable".
    let _ = stream.write_all(b"HEAD / HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let mut buf = [0u8; 16];
    // read() returns Ok(0)=EOF or Ok(n>0)=data or Err=timeout/reset — all mean remote responded.
    match stream.read(&mut buf) {
        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut
                   || e.kind() == std::io::ErrorKind::WouldBlock => None,
        _ => Some(t0.elapsed().as_millis() as u64),
    }
}

/// Wait for the port to open, probe the remote end, and kill the tunnel
/// process if the remote is unreachable.  Returns latency on success.
fn wait_and_probe(port: u16, pid: u32, timeout: Duration) -> Result<u64> {
    if let Err(e) = wait_for_port(port, timeout) {
        stop_tunnel(pid);
        return Err(e);
    }
    match probe_remote(port) {
        Some(ms) => Ok(ms),
        None => {
            stop_tunnel(pid);
            Err(AppError::Tunnel(format!(
                "Remote service unreachable — no response on port {} within 5 s", port
            )))
        }
    }
}

fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if test_port(port) { return Ok(()); }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(AppError::PortClosed(port))
}

// ── Detect running tunnels ────────────────────────────────────────────────────

pub fn detect_tunnels() -> Vec<TunnelProcess> {
    let out = match Command::new("ps").args(["-ww", "-eo", "pid,args"]).output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut tunnels = Vec::new();

    for line in stdout.lines() {
        if !line.contains("session-manager-plugin") { continue; }
        let pid_str = line.trim().split_whitespace().next().unwrap_or("");
        let pid: u32 = match pid_str.parse() { Ok(p) => p, Err(_) => continue };
        if let Some(tp) = parse_tunnel_line(line, pid) {
            tunnels.push(tp);
        }
    }
    tunnels
}

fn parse_tunnel_line(line: &str, pid: u32) -> Option<TunnelProcess> {
    let after = line.splitn(2, "session-manager-plugin").nth(1)?;

    let mut local_port: u16 = 0;
    let mut remote_port: u16 = 0;
    let mut remote_host: Option<String> = None;
    let mut instance_id = String::new();

    for json_str in extract_json_objects(after) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(t) = val.get("Target").and_then(|v| v.as_str()) {
                instance_id = t.to_string();
            }
            // Parameters may be at the top level OR nested under "Parameters"
            let params = val.get("Parameters").unwrap_or(&val);
            if let Some(arr) = params.get("localPortNumber").and_then(|v| v.as_array()) {
                if let Some(p) = arr.first().and_then(|v| v.as_str()) {
                    local_port = p.parse().unwrap_or(0);
                }
            }
            if let Some(arr) = params.get("portNumber").and_then(|v| v.as_array()) {
                if let Some(p) = arr.first().and_then(|v| v.as_str()) {
                    remote_port = p.parse().unwrap_or(0);
                }
            }
            if let Some(arr) = params.get("host").and_then(|v| v.as_array()) {
                if let Some(h) = arr.first().and_then(|v| v.as_str()) {
                    remote_host = Some(h.to_string());
                }
            }
        }
    }

    if local_port == 0 { return None; }
    // For display: prefer the remote host as name, fall back to instance ID
    let instance_name = remote_host.clone().unwrap_or_else(|| instance_id.clone());
    let port_open = test_port(local_port);
    let latency_ms = if port_open { probe_remote(local_port) } else { None };
    Some(TunnelProcess { pid, local_port, remote_port, remote_host, instance_id, instance_name, port_open, latency_ms })
}

fn extract_json_objects(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            let start = i;
            let mut depth = 0usize;
            while i < chars.len() {
                match chars[i] {
                    '{' => depth += 1,
                    '}' => { depth -= 1; if depth == 0 { result.push(chars[start..=i].iter().collect()); i += 1; break; } }
                    _ => {}
                }
                i += 1;
            }
        } else { i += 1; }
    }
    result
}

// ── Build SSM start-session command ──────────────────────────────────────────

fn make_ssm_cmd(instance_id: &str, doc_name: &str, params: &str, profile: Option<&str>) -> Command {
    let mut cmd = Command::new("aws");
    let p = profile
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AWS_PROFILE").ok().filter(|s| !s.is_empty()));
    if let Some(p) = p { cmd.args(["--profile", &p]); }
    cmd.args(["ssm", "start-session",
        "--target", instance_id,
        "--document-name", doc_name,
        "--parameters", params]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd
}

// ── Start tunnels ─────────────────────────────────────────────────────────────

pub fn start_direct_tunnel(
    instance_id: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<Child> {
    let params = format!(
        r#"{{"portNumber":["{}"],"localPortNumber":["{}"]}}"#,
        remote_port, local_port
    );
    Ok(make_ssm_cmd(instance_id, "AWS-StartPortForwardingSession", &params, profile).spawn()?)
}

pub fn start_remote_tunnel(
    bastion_id: &str,
    host: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<Child> {
    let params = format!(
        r#"{{"host":["{}"],"portNumber":["{}"],"localPortNumber":["{}"]}}"#,
        host, remote_port, local_port
    );
    Ok(make_ssm_cmd(bastion_id, "AWS-StartPortForwardingSessionToRemoteHost", &params, profile).spawn()?)
}

// ── High-level tunnel creation ────────────────────────────────────────────────

pub fn start_tunnel_by_pattern(
    pattern: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<TunnelProcess> {
    let inst = aws::find_instance_by_name(pattern, profile)?;
    let child = start_direct_tunnel(&inst.id, local_port, remote_port, profile)?;
    let pid = child.id();
    std::mem::forget(child);
    let latency_ms = wait_and_probe(local_port, pid, Duration::from_secs(20))?;
    Ok(TunnelProcess {
        pid, local_port, remote_port, remote_host: None,
        instance_id: inst.id, instance_name: inst.name,
        port_open: true, latency_ms: Some(latency_ms),
    })
}

pub fn start_url_tunnel_via_any_bastion(
    url: &str,
    local_port: u16,
    profile: Option<&str>,
) -> Result<TunnelProcess> {
    let host = aws::strip_url_to_host(url);
    let remote_port: u16 = if url.starts_with("https://") { 443 } else { 80 };
    let bastions = aws::find_bastions(profile)?;
    let online_bastions: Vec<_> = bastions.into_iter().filter(|b| b.ssm_online).collect();
    if online_bastions.is_empty() { return Err(AppError::NoBastions); }

    for bastion in &online_bastions {
        let child = start_remote_tunnel(&bastion.id, &host, local_port, remote_port, profile)?;
        let pid = child.id();
        std::mem::forget(child);
        match wait_and_probe(local_port, pid, Duration::from_secs(10)) {
            Ok(latency_ms) => {
                return Ok(TunnelProcess {
                    pid, local_port, remote_port,
                    remote_host: Some(host),
                    instance_id: bastion.id.clone(), instance_name: bastion.name.clone(),
                    port_open: true, latency_ms: Some(latency_ms),
                });
            }
            Err(_) => {
                stop_tunnel(pid);
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Err(AppError::Tunnel(format!(
        "All {} bastion(s) failed to tunnel to {}:{}", online_bastions.len(), host, remote_port
    )))
}

pub fn start_dns_tunnel(
    url: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<TunnelProcess> {
    let target = aws::resolve_dns_to_target(url, profile)?;
    match target {
        TunnelTarget::Ec2 { instance_id, name } => {
            let child = start_direct_tunnel(&instance_id, local_port, remote_port, profile)?;
            let pid = child.id();
            std::mem::forget(child);
            let latency_ms = wait_and_probe(local_port, pid, Duration::from_secs(20))?;
            Ok(TunnelProcess {
                pid, local_port, remote_port, remote_host: None,
                instance_id, instance_name: name,
                port_open: true, latency_ms: Some(latency_ms),
            })
        }
        TunnelTarget::RemoteViaBastion { bastion_id, bastion_name, target_host, .. } => {
            let child = start_remote_tunnel(&bastion_id, &target_host, local_port, remote_port, profile)?;
            let pid = child.id();
            std::mem::forget(child);
            let latency_ms = wait_and_probe(local_port, pid, Duration::from_secs(20))?;
            Ok(TunnelProcess {
                pid, local_port, remote_port,
                remote_host: Some(target_host),
                instance_id: bastion_id, instance_name: bastion_name,
                port_open: true, latency_ms: Some(latency_ms),
            })
        }
    }
}

pub fn start_remote_tunnel_via_pattern(
    bastion_pattern: &str,
    host: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<TunnelProcess> {
    let bastion = aws::find_instance_by_name(bastion_pattern, profile)?;
    let child = start_remote_tunnel(&bastion.id, host, local_port, remote_port, profile)?;
    let pid = child.id();
    std::mem::forget(child);
    let latency_ms = wait_and_probe(local_port, pid, Duration::from_secs(20))?;
    Ok(TunnelProcess {
        pid, local_port, remote_port,
        remote_host: Some(host.to_string()),
        instance_id: bastion.id, instance_name: bastion.name,
        port_open: true, latency_ms: Some(latency_ms),
    })
}

pub fn start_remote_tunnel_via_instance(
    instance_id: &str,
    instance_name: &str,
    host: &str,
    local_port: u16,
    remote_port: u16,
    profile: Option<&str>,
) -> Result<TunnelProcess> {
    let child = start_remote_tunnel(instance_id, host, local_port, remote_port, profile)?;
    let pid = child.id();
    std::mem::forget(child);
    let latency_ms = wait_and_probe(local_port, pid, Duration::from_secs(20))?;
    Ok(TunnelProcess {
        pid, local_port, remote_port,
        remote_host: Some(host.to_string()),
        instance_id: instance_id.to_string(),
        instance_name: instance_name.to_string(),
        port_open: true, latency_ms: Some(latency_ms),
    })
}

// ── Stop tunnels ──────────────────────────────────────────────────────────────

pub fn stop_tunnel(pid: u32) {
    #[cfg(unix)]
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
    #[cfg(not(unix))]
    { let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status(); }
}

pub fn stop_all_tunnels() {
    for t in detect_tunnels() { stop_tunnel(t.pid); }
    let _ = Command::new("pkill").args(["-f", "session-manager-plugin"]).status();
}
