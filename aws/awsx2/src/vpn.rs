//! VPN connection via AWS Client VPN with SAML authentication.
//!
//! Flow: openvpn → SAML URL → headless browser (SSO login) → SAML callback → VPN connect → DNS config.
//! Supports both Linux and macOS.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::error::{AppError, Result};
use crate::models::VpnConfig;

const SAML_LISTEN_PORT: u16 = 35001;

fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

// ── Config persistence ───────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    let base = dirs::config_dir()
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into()))
                .join(".config")
        });
    base.join("awsx2").join("vpn.json")
}

pub fn load_config() -> Result<VpnConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(VpnConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    serde_json::from_str(&content).map_err(|e| AppError::Vpn(format!("Bad vpn.json: {}", e)))
}

pub fn save_config(config: &VpnConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| AppError::Vpn(format!("Serialize error: {}", e)))?;
    std::fs::write(&path, &json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ── OpenVPN binary detection (platform-aware) ────────────────────────────────

/// Paths to the AWS-patched OpenVPN binary bundled with AWS VPN Client.
/// On Linux it uses a musl-linked binary with its own loader.
/// On macOS it ships as a standard Mach-O binary inside the .app bundle.
fn find_aws_openvpn() -> Option<Vec<String>> {
    if is_macos() {
        // macOS: AWS VPN Client ships openvpn inside the .app bundle
        let candidates = [
            "/Applications/AWS VPN Client.app/Contents/Resources/openvpn/acvc-openvpn",
            "/Applications/AWS VPN Client/AWS VPN Client.app/Contents/Resources/openvpn/acvc-openvpn",
        ];
        for path in candidates {
            if std::path::Path::new(path).exists() {
                return Some(vec![path.to_string()]);
            }
        }
    } else {
        // Linux: musl-linked binary needs the bundled loader
        let dir = "/opt/awsvpnclient/Service/Resources/openvpn";
        let musl = format!("{}/ld-musl-x86_64.so.1", dir);
        let acvc = format!("{}/acvc-openvpn", dir);
        if std::path::Path::new(&musl).exists() && std::path::Path::new(&acvc).exists() {
            return Some(vec![musl, "--library-path".into(), dir.into(), acvc]);
        }
    }
    None
}

fn openvpn_cmd(config_path: &str, creds_path: &str) -> Command {
    if let Some(args) = find_aws_openvpn() {
        let mut cmd = Command::new(&args[0]);
        for arg in &args[1..] {
            cmd.arg(arg);
        }
        cmd.args(["--config", config_path, "--auth-user-pass", creds_path, "--verb", "3"]);
        cmd
    } else {
        let mut cmd = Command::new("openvpn");
        cmd.args(["--config", config_path, "--auth-user-pass", creds_path, "--verb", "3"]);
        cmd
    }
}

// ── .ovpn config preparation ─────────────────────────────────────────────────

fn prepare_ovpn_config(ovpn_path: &str) -> Result<tempfile::NamedTempFile> {
    let content = std::fs::read_to_string(ovpn_path)
        .map_err(|e| AppError::Vpn(format!("Cannot read {}: {}", ovpn_path, e)))?;
    let filtered: String = content
        .lines()
        .filter(|line| {
            let l = line.trim();
            !l.starts_with("auth-federate")
                && !l.starts_with("auth-retry")
                && !l.starts_with("auth-nocache")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(filtered.as_bytes())?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;
    Ok(tmp)
}

fn write_creds(user: &str, pass: &str) -> Result<tempfile::NamedTempFile> {
    let mut tmp = tempfile::NamedTempFile::new()?;
    write!(tmp, "{}\n{}\n", user, pass)?;
    tmp.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(tmp)
}

// ── Phase 1: Get SAML challenge from VPN server ──────────────────────────────

pub struct SamlChallenge {
    pub saml_url: String,
    pub sid: String,
    /// The actual server IP:port from phase 1 (needed because remote-random-hostname
    /// causes each connection to resolve to a different server, but the SAML session
    /// is bound to the server that issued the challenge).
    pub server_ip: Option<String>,
}

pub fn fetch_saml_challenge(ovpn_config_path: &str) -> Result<SamlChallenge> {
    let creds = write_creds("N/A", &format!("ACS::{}", SAML_LISTEN_PORT))?;

    let mut child = openvpn_cmd(ovpn_config_path, creds.path().to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if start.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }

    let mut stdout_buf = String::new();
    let mut stderr_buf = String::new();
    if let Some(ref mut out) = child.stdout {
        let _ = out.read_to_string(&mut stdout_buf);
    }
    if let Some(ref mut err) = child.stderr {
        let _ = err.read_to_string(&mut stderr_buf);
    }
    let combined = format!("{}\n{}", stdout_buf, stderr_buf);

    let url_re = Regex::new(r"https://portal\.sso\.[^\s,]+").unwrap();
    let sid_re = Regex::new(r"CRV1:R:([^:]+)").unwrap();

    let saml_url = url_re
        .find(&combined)
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| {
            AppError::SamlAuth(format!(
                "Could not extract SAML URL from VPN server output.\nLast lines:\n{}",
                combined.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
            ))
        })?;

    let sid = sid_re
        .captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| AppError::SamlAuth("Could not extract session ID (CRV1:R:...)".into()))?;

    // Extract the actual server IP so phase 4 connects to the same server.
    // remote-random-hostname causes DNS to resolve differently each time.
    let ip_re = Regex::new(r"link remote: \[AF_INET\](\d+\.\d+\.\d+\.\d+:\d+)").unwrap();
    let server_ip = ip_re
        .captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    Ok(SamlChallenge { saml_url, sid, server_ip })
}

// ── Phase 2: SAML helpers ─────────────────────────────────────────────────────

fn extract_saml_from_form(body: &str) -> Option<String> {
    url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == "SAMLResponse")
        .map(|(_, val)| val.replace(['\n', '\r', ' '], ""))
}

fn extract_saml_from_query(url_str: &str) -> Option<String> {
    let full = format!("http://localhost{}", url_str);
    url::Url::parse(&full)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "SAMLResponse")
        .map(|(_, val)| val.replace(['\n', '\r', ' '], ""))
}

// ── Phase 3: Browser automation (headless Chrome) ────────────────────────────

fn complete_saml_auth(
    saml_url: &str,
    sso_user: &str,
    sso_pass: &str,
    mfa_code: &str,
) -> Result<()> {
    use headless_chrome::{Browser, LaunchOptions};

    let options = LaunchOptions {
        headless: true,
        sandbox: false,
        args: vec![
            std::ffi::OsStr::new("--disable-gpu"),
            std::ffi::OsStr::new("--no-sandbox"),
        ],
        ..Default::default()
    };

    let browser = Browser::new(options)
        .map_err(|e| AppError::Browser(format!("Failed to launch browser: {}", e)))?;

    let tab = browser
        .new_tab()
        .map_err(|e| AppError::Browser(format!("Failed to create tab: {}", e)))?;

    tab.navigate_to(saml_url)
        .map_err(|e| AppError::Browser(format!("Navigation failed: {}", e)))?;
    tab.wait_until_navigated()
        .map_err(|e| AppError::Browser(format!("Wait failed: {}", e)))?;

    std::thread::sleep(Duration::from_secs(3));

    // Step A: Username
    fill_field_and_submit(&tab, &[
        "input[type='email']",
        "input[name='username']",
        "input[name='email']",
        "#awsui-input-0",
        "input[data-testid='username-input']",
    ], sso_user)?;
    std::thread::sleep(Duration::from_secs(3));

    // Step B: Password
    fill_field_and_submit(&tab, &[
        "input[type='password']",
        "input[name='password']",
        "#awsui-input-1",
        "input[data-testid='password-input']",
    ], sso_pass)?;
    std::thread::sleep(Duration::from_secs(4));

    // Step C: MFA
    fill_field_and_submit(&tab, &[
        "input[placeholder='Enter code']",
        "input[placeholder*='code']",
        "input[name='mfaCode']",
        "input[name='totp']",
        "input[type='tel']",
        "input[data-testid='mfa-code-input']",
        "input[inputmode='numeric']",
    ], mfa_code)?;
    std::thread::sleep(Duration::from_secs(4));

    // Check if page has SAMLResponse form and submit it
    if let Ok(content) = tab.get_content() {
        if content.contains("SAMLResponse") {
            let _ = tab.evaluate("document.forms[0].submit()", false);
            std::thread::sleep(Duration::from_secs(3));
        }
    }

    Ok(())
}

fn fill_field_and_submit(
    tab: &headless_chrome::Tab,
    selectors: &[&str],
    value: &str,
) -> Result<()> {
    for selector in selectors {
        if let Ok(el) = tab.find_element(selector) {
            el.click()
                .map_err(|e| AppError::Browser(format!("Click failed: {}", e)))?;
            el.type_into(value)
                .map_err(|e| AppError::Browser(format!("Type failed: {}", e)))?;
            std::thread::sleep(Duration::from_millis(500));

            let submit_selectors = [
                "button[type='submit']",
                "input[type='submit']",
            ];
            let mut submitted = false;
            for s in submit_selectors {
                if let Ok(btn) = tab.find_element(s) {
                    if btn.click().is_ok() {
                        submitted = true;
                        break;
                    }
                }
            }
            if !submitted {
                let _ = tab.press_key("Enter");
            }

            let _ = tab.wait_until_navigated();
            return Ok(());
        }
    }
    Ok(())
}

fn open_url_in_browser(url: &str) {
    let cmd = if is_macos() { "open" } else { "xdg-open" };
    let _ = Command::new(cmd).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

// ── Phase 4: Connect VPN with SAML token ─────────────────────────────────────

/// Pin the config to a specific server IP for phase 4 reconnection.
/// remote-random-hostname causes each connection to resolve to a different server,
/// but the SAML session is bound to the server that issued the challenge.
fn pin_config_to_server(ovpn_config_path: &str, ip: &str, port: &str) -> Result<tempfile::NamedTempFile> {
    let content = std::fs::read_to_string(ovpn_config_path)
        .map_err(|e| AppError::Vpn(format!("Cannot read {}: {}", ovpn_config_path, e)))?;
    let filtered: String = content
        .lines()
        .filter(|line| {
            let l = line.trim();
            !l.starts_with("remote ") && !l.starts_with("remote-random-hostname")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut tmp = tempfile::NamedTempFile::new()?;
    write!(tmp, "{}\nremote {} {}\n", filtered, ip, port)?;
    tmp.flush()?;
    Ok(tmp)
}

fn start_vpn_process(
    ovpn_config_path: &str,
    sid: &str,
    saml_response: &str,
    server_ip: Option<&str>,
) -> Result<(u32, tempfile::NamedTempFile, tempfile::NamedTempFile, Option<tempfile::NamedTempFile>)> {
    let cred_password = format!("CRV1::{}::{}", sid, saml_response);
    let creds = write_creds("N/A", &cred_password)?;
    let creds_path = creds.path().to_str().unwrap().to_string();

    // If we know the server IP from phase 1, pin the config to that IP
    let pinned_config = if let Some(ip_port) = server_ip {
        if let Some((ip, port)) = ip_port.split_once(':') {
            Some(pin_config_to_server(ovpn_config_path, ip, port)?)
        } else {
            None
        }
    } else {
        None
    };
    let effective_config = pinned_config.as_ref()
        .map(|f| f.path().to_str().unwrap())
        .unwrap_or(ovpn_config_path);

    // Build the openvpn command. Use sudo only if not already root.
    let is_root = unsafe { libc::geteuid() } == 0;
    let mut cmd = if let Some(args) = find_aws_openvpn() {
        let (bin, rest) = if is_root {
            (args[0].clone(), &args[1..])
        } else {
            ("sudo".to_string(), &args[..])
        };
        let mut c = Command::new(&bin);
        for arg in rest {
            c.arg(arg);
        }
        c.args(["--config", effective_config, "--auth-user-pass", &creds_path, "--verb", "3"]);
        c
    } else {
        if is_root {
            let mut c = Command::new("openvpn");
            c.args(["--config", effective_config, "--auth-user-pass", &creds_path, "--verb", "3"]);
            c
        } else {
            let mut c = Command::new("sudo");
            c.args(["openvpn", "--config", effective_config, "--auth-user-pass", &creds_path, "--verb", "3"]);
            c
        }
    };

    let stderr_log = tempfile::NamedTempFile::new()?;
    let stderr_file = stderr_log.reopen()?;
    let stdout_log = tempfile::NamedTempFile::new()?;
    let stdout_file = stdout_log.reopen()?;

    use std::os::unix::process::CommandExt;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(stdout_file)
        .stderr(stderr_file)
        .process_group(0) // detach into own process group
        .spawn()?;

    // Give the process a moment to start, then check if it crashed immediately
    std::thread::sleep(Duration::from_secs(2));
    if let Ok(Some(status)) = child.try_wait() {
        let mut log = String::new();
        if let Ok(mut f) = std::fs::File::open(stderr_log.path()) {
            let _ = f.read_to_string(&mut log);
        }
        if let Ok(mut f) = std::fs::File::open(stdout_log.path()) {
            let _ = f.read_to_string(&mut log);
        }
        let last_lines: String = log.lines().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(AppError::Vpn(format!(
            "openvpn exited immediately ({})\n{}",
            status, last_lines
        )));
    }

    let pid = child.id();
    std::mem::forget(child);

    Ok((pid, creds, stderr_log, pinned_config))
}

// ── Phase 5: TUN interface detection (platform-aware) ────────────────────────

/// Find the active TUN/UTUN interface name.
/// Linux: tun0, tun1, etc.
/// macOS: utun0, utun1, utun2, etc. (utun0 is often used by the system)
fn find_tun_interface() -> Option<String> {
    if is_macos() {
        let output = Command::new("ifconfig")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let iface_re = Regex::new(r"(?m)^(\S+):").ok()?;
        let inet_re = Regex::new(r"inet (\d+\.\d+\.\d+\.\d+)").ok()?;

        // Split ifconfig output by interface block and find the last utun with IPv4
        let starts: Vec<_> = iface_re.find_iter(&stdout).collect();
        let mut last_vpn = None;
        for (i, m) in starts.iter().enumerate() {
            let name = m.as_str().trim_end_matches(':');
            if !name.starts_with("utun") {
                continue;
            }
            let block_end = starts.get(i + 1).map_or(stdout.len(), |n| n.start());
            let block = &stdout[m.start()..block_end];
            if inet_re.is_match(block) {
                last_vpn = Some(name.to_string());
            }
        }
        last_vpn
    } else {
        // Linux: check tun0, tun1, etc.
        for i in 0..8 {
            let iface = format!("tun{}", i);
            let status = Command::new("ip")
                .args(["link", "show", &iface])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if status.map_or(false, |s| s.success()) {
                return Some(iface);
            }
        }
        None
    }
}

// ── Phase 6: DNS configuration (platform-aware) ─────────────────────────────

pub fn configure_dns(dns_server: &str, dns_domain: &str) -> Result<()> {
    if dns_server.is_empty() || dns_domain.is_empty() {
        return Ok(());
    }

    // Wait for TUN interface to come up
    let start = Instant::now();
    let mut tun_iface = None;
    while start.elapsed() < Duration::from_secs(20) {
        if let Some(iface) = find_tun_interface() {
            tun_iface = Some(iface);
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let tun_iface = tun_iface
        .ok_or_else(|| AppError::Vpn("TUN interface did not come up within 20 seconds".into()))?;

    std::thread::sleep(Duration::from_secs(1));

    if is_macos() {
        configure_dns_macos(dns_server, dns_domain, &tun_iface)
    } else {
        configure_dns_linux(dns_server, dns_domain, &tun_iface)
    }
}

fn configure_dns_linux(dns_server: &str, dns_domain: &str, iface: &str) -> Result<()> {
    let _ = Command::new("resolvectl")
        .args(["dns", iface, dns_server])
        .status()
        .map_err(|e| AppError::Vpn(format!("resolvectl dns failed: {}", e)))?;

    let _ = Command::new("resolvectl")
        .args(["domain", iface, dns_domain])
        .status()
        .map_err(|e| AppError::Vpn(format!("resolvectl domain failed: {}", e)))?;

    let _ = Command::new("resolvectl")
        .args(["default-route", iface, "false"])
        .status();

    Ok(())
}

fn configure_dns_macos(dns_server: &str, dns_domain: &str, _iface: &str) -> Result<()> {
    // Strip the ~ prefix from the domain for the resolver config
    let domain = dns_domain.trim_start_matches('~');

    // macOS: create a resolver configuration file in /etc/resolver/
    // This tells macOS to route DNS queries for the specified domain to our DNS server.
    let resolver_dir = "/etc/resolver";
    let _ = Command::new("sudo")
        .args(["mkdir", "-p", resolver_dir])
        .status();

    let resolver_content = format!("nameserver {}\n", dns_server);
    let resolver_path = format!("{}/{}", resolver_dir, domain);

    let mut child = Command::new("sudo")
        .args(["tee", &resolver_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Vpn(format!("Failed to write resolver config: {}", e)))?;
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(resolver_content.as_bytes());
    }
    let _ = child.wait();

    // Flush DNS cache
    let _ = Command::new("sudo")
        .args(["dscacheutil", "-flushcache"])
        .status();
    let _ = Command::new("sudo")
        .args(["killall", "-HUP", "mDNSResponder"])
        .status();

    Ok(())
}

// ── Status detection (platform-aware) ────────────────────────────────────────

pub fn is_connected() -> bool {
    find_tun_interface().is_some()
}

pub fn get_vpn_ip() -> Option<String> {
    let iface = find_tun_interface()?;
    let re = Regex::new(r"inet (\d+\.\d+\.\d+\.\d+)").ok()?;

    if is_macos() {
        let output = Command::new("ifconfig")
            .arg(&iface)
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        re.captures(&stdout)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    } else {
        let output = Command::new("ip")
            .args(["-4", "addr", "show", &iface])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        re.captures(&stdout)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }
}

pub fn find_vpn_pid() -> Option<u32> {
    let output = Command::new("pgrep")
        .args(["-f", "acvc-openvpn|openvpn.*--config"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
}

pub fn disconnect() {
    if let Some(pid) = find_vpn_pid() {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    // Fallback: kill by process name pattern
    let _ = Command::new("pkill")
        .args(["-f", "acvc-openvpn|openvpn.*--config"])
        .status();

    // macOS: clean up resolver files created by configure_dns_macos and flush DNS
    if is_macos() {
        if let Ok(config) = load_config() {
            if !config.dns_domain.is_empty() {
                let domain = config.dns_domain.trim_start_matches('~');
                let resolver_path = format!("/etc/resolver/{}", domain);
                let _ = Command::new("sudo")
                    .args(["rm", "-f", &resolver_path])
                    .status();
            }
        }
        let _ = Command::new("sudo")
            .args(["dscacheutil", "-flushcache"])
            .status();
        let _ = Command::new("sudo")
            .args(["killall", "-HUP", "mDNSResponder"])
            .status();
    }
}

// ── High-level orchestration ─────────────────────────────────────────────────

/// Full VPN connection flow. Returns the openvpn PID on success.
pub fn connect<F>(config: &VpnConfig, mfa_code: &str, mut progress: F) -> Result<u32>
where
    F: FnMut(&str),
{
    if config.ovpn_path.is_empty() {
        return Err(AppError::Vpn("No .ovpn file path configured. Run 'awsx2 vpn setup' first.".into()));
    }
    if config.sso_username.is_empty() || config.sso_password.is_empty() {
        return Err(AppError::Vpn("SSO credentials not configured. Run 'awsx2 vpn setup' first.".into()));
    }

    progress("[1/5] Preparing VPN config...");
    let modified_config = prepare_ovpn_config(&config.ovpn_path)?;
    let config_path = modified_config.path().to_str().unwrap().to_string();

    progress("[2/5] Fetching SAML URL from VPN server...");
    let challenge = fetch_saml_challenge(&config_path)?;
    progress(&format!("  SAML URL received ({} chars), SID: {}...",
        challenge.saml_url.len(),
        &challenge.sid[..challenge.sid.len().min(30)]));

    progress("[3/5] Completing SAML authentication (headless browser)...");

    let saml_url = challenge.saml_url.clone();
    let user = config.sso_username.clone();
    let pass = config.sso_password.clone();
    let mfa = mfa_code.to_string();

    let browser_failed = Arc::new(AtomicBool::new(false));
    let bf = browser_failed.clone();
    let browser_handle = std::thread::spawn(move || {
        let result = complete_saml_auth(&saml_url, &user, &pass, &mfa);
        if result.is_err() {
            bf.store(true, Ordering::SeqCst);
        }
        result
    });

    // Wait for SAML callback with system browser fallback.
    // If headless Chrome fails or takes too long, open the real browser.
    let server = tiny_http::Server::http(format!("127.0.0.1:{}", SAML_LISTEN_PORT))
        .map_err(|e| AppError::Vpn(format!("Cannot bind SAML listener on port {}: {}", SAML_LISTEN_PORT, e)))?;

    let deadline = Instant::now() + Duration::from_secs(120);
    let fallback_at = Instant::now() + Duration::from_secs(25);
    let mut fallback_opened = false;

    let saml_response = loop {
        if Instant::now() > deadline {
            return Err(AppError::SamlAuth("SAML callback timeout (no response received)".into()));
        }

        // Fall back to system browser if headless Chrome failed or is taking too long
        if !fallback_opened
            && (browser_failed.load(Ordering::SeqCst) || Instant::now() > fallback_at)
        {
            progress("  Headless browser did not complete. Opening system browser...");
            open_url_in_browser(&challenge.saml_url);
            fallback_opened = true;
        }

        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(mut request)) => {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);

                let saml = extract_saml_from_form(&body)
                    .or_else(|| extract_saml_from_query(request.url()));

                let response = tiny_http::Response::from_string(
                    "<html><body><h2>VPN auth complete. You can close this tab.</h2></body></html>",
                )
                .with_header(
                    "Content-Type: text/html"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                );
                let _ = request.respond(response);

                if let Some(saml) = saml {
                    break saml;
                }
            }
            Ok(None) => continue,
            Err(e) => return Err(AppError::Vpn(format!("SAML listener error: {}", e))),
        }
    };

    progress(&format!("  SAML response captured ({} chars)", saml_response.len()));

    let _ = browser_handle.join().map_err(|_| AppError::Browser("Browser thread panicked".into()))?;

    progress("[4/5] Connecting VPN with SAML token (sudo required)...");
    let openvpn_type = if find_aws_openvpn().is_some() { "acvc-openvpn" } else { "stock openvpn" };
    progress(&format!("  Using {}, pinned to server: {}",
        openvpn_type,
        challenge.server_ip.as_deref().unwrap_or("(DNS, not pinned)")));

    // Prime sudo credentials so the openvpn spawn doesn't silently wait for a password
    let sudo_status = Command::new("sudo")
        .args(["-v"])
        .status()
        .map_err(|e| AppError::Vpn(format!("sudo failed: {}", e)))?;
    if !sudo_status.success() {
        return Err(AppError::Vpn("sudo authentication failed".into()));
    }

    let (pid, _creds, _stderr_log, _pinned_config) = start_vpn_process(
        &config_path,
        &challenge.sid,
        &saml_response,
        challenge.server_ip.as_deref(),
    )?;

    // Keep temp files alive so openvpn can read them
    std::mem::forget(modified_config);
    std::mem::forget(_creds);
    std::mem::forget(_stderr_log);
    if let Some(pc) = _pinned_config { std::mem::forget(pc); }

    progress("[5/5] Waiting for TUN interface and configuring DNS...");

    // Wait for TUN interface to come up, checking that openvpn is still alive.
    let start = Instant::now();
    let mut tun_found = false;
    while start.elapsed() < Duration::from_secs(20) {
        if find_tun_interface().is_some() {
            tun_found = true;
            break;
        }
        // Check if the openvpn process died
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_or(false, |s| s.success());
        if !alive {
            return Err(AppError::Vpn(format!(
                "openvpn process (PID {}) exited before TUN interface came up. \
                 Try running with: sudo awsx2 vpn connect",
                pid
            )));
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    if !tun_found {
        return Err(AppError::Vpn("TUN interface did not come up within 20 seconds".into()));
    }

    configure_dns(&config.dns_server, &config.dns_domain)?;

    let ip = get_vpn_ip().unwrap_or_else(|| "unknown".into());
    progress(&format!("VPN connected! IP: {}, PID: {}", ip, pid));

    Ok(pid)
}
