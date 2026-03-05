//! AWS CLI wrapper — all calls shell out to the `aws` binary.
//! Auth (SSO/profiles) is handled transparently by the CLI.

use std::collections::{HashMap, HashSet};
use std::process::Command;

use crate::error::{AppError, Result};
use crate::models::*;

// ── Internal helpers ──────────────────────────────────────────────────────────

fn aws_cmd(profile: Option<&str>) -> Command {
    let mut cmd = Command::new("aws");
    let p = profile
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AWS_PROFILE").ok().filter(|s| !s.is_empty()));
    if let Some(p) = p {
        cmd.args(["--profile", &p]);
    }
    cmd
}

fn run_aws(args: &[&str], profile: Option<&str>) -> Result<String> {
    let output = aws_cmd(profile)
        .args(args)
        .args(["--output", "json"])
        .output()?;
    if !output.status.success() {
        return Err(AppError::AwsCli(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_aws_silent(args: &[&str], profile: Option<&str>) -> Result<()> {
    let output = aws_cmd(profile).args(args).output()?;
    if !output.status.success() {
        return Err(AppError::AwsCli(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn list_instances(profile: Option<&str>) -> Result<Vec<Instance>> {
    let json = run_aws(
        &["ec2", "describe-instances", "--query", "Reservations[*].Instances[*]"],
        profile,
    )?;
    let raw: Vec<Vec<RawInstance>> = serde_json::from_str(&json)?;
    let flat: Vec<RawInstance> = raw.into_iter().flatten().collect();
    let ssm_map = get_ssm_status(profile).unwrap_or_default();
    Ok(flat.into_iter().map(|r| raw_to_instance(r, &ssm_map)).collect())
}

fn raw_to_instance(raw: RawInstance, ssm_map: &HashMap<String, String>) -> Instance {
    let name = raw
        .tags
        .as_ref()
        .and_then(|tags| tags.iter().find(|t| t.key == "Name"))
        .map(|t| t.value.clone())
        .unwrap_or_default();

    let ssm_status = match ssm_map.get(&raw.instance_id).map(|s| s.as_str()) {
        Some("Online")  => SsmStatus::Online,
        Some("Offline") => SsmStatus::Offline,
        _               => SsmStatus::Unknown,
    };

    let sgs = raw.security_groups.unwrap_or_default();
    let security_group_ids = sgs.iter().map(|sg| sg.group_id.clone()).collect();
    let security_groups = sgs.into_iter().map(|sg| sg.group_name).collect();

    Instance {
        id: raw.instance_id,
        name,
        instance_type: raw.instance_type,
        state: InstanceState::from_str(&raw.state.name),
        private_ip: raw.private_ip,
        public_ip: raw.public_ip,
        ssm_status,
        tunnel: None,
        security_groups,
        security_group_ids,
    }
}

pub fn get_ssm_status(profile: Option<&str>) -> Result<HashMap<String, String>> {
    let json = run_aws(&["ssm", "describe-instance-information"], profile)?;
    let resp: SsmDescribeResponse = serde_json::from_str(&json)?;
    Ok(resp
        .instance_information_list
        .into_iter()
        .map(|i| (i.instance_id, i.ping_status))
        .collect())
}

pub fn start_instance(id: &str, profile: Option<&str>) -> Result<()> {
    run_aws_silent(&["ec2", "start-instances", "--instance-ids", id], profile)
}

pub fn stop_instance(id: &str, force: bool, profile: Option<&str>) -> Result<()> {
    let mut args = vec!["ec2", "stop-instances", "--instance-ids", id];
    if force { args.push("--force"); }
    run_aws_silent(&args, profile)
}

pub fn modify_instance_type(id: &str, new_type: &str, profile: Option<&str>) -> Result<()> {
    run_aws_silent(
        &["ec2", "modify-instance-attribute", "--instance-id", id, "--instance-type", new_type],
        profile,
    )
}

pub fn find_instance_by_name(pattern: &str, profile: Option<&str>) -> Result<Instance> {
    let instances = list_instances(profile)?;
    let pat_lower = pattern.to_lowercase();
    let matches: Vec<Instance> = instances
        .into_iter()
        .filter(|i| i.name.to_lowercase().contains(&pat_lower))
        .collect();
    match matches.len() {
        0 => Err(AppError::NoInstance(pattern.to_string())),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(AppError::MultipleInstances(format!("{} ({} matches)", pattern, n))),
    }
}

pub fn find_bastions(profile: Option<&str>) -> Result<Vec<BastionInfo>> {
    let ssm_map = get_ssm_status(profile).unwrap_or_default();
    let instances = list_instances(profile)?;
    Ok(instances
        .into_iter()
        .filter(|i| i.name.to_lowercase().contains("bastion") && i.state == InstanceState::Running)
        .map(|i| {
            let ssm_online = ssm_map.get(&i.id).map(|s| s == "Online").unwrap_or(false);
            BastionInfo { id: i.id, name: i.name, ssm_online }
        })
        .collect())
}

pub fn resolve_dns_to_target(input: &str, profile: Option<&str>) -> Result<TunnelTarget> {
    let host = strip_url_to_host(input);
    let addrs = dns_lookup(&host);
    let instances = list_instances(profile)?;

    // Direct local IP → EC2 match
    for addr in &addrs {
        let addr_str = addr.to_string();
        if let Some(inst) = instances.iter().find(|i| i.private_ip.as_deref() == Some(&addr_str)) {
            return Ok(TunnelTarget::Ec2 { instance_id: inst.id.clone(), name: inst.name.clone() });
        }
    }

    // For internal hostnames (no local DNS) or unmatched IPs, fall back to bastion
    // forwarding using the hostname as-is — the bastion's DNS will resolve it.
    let bastions = find_bastions(profile)?;
    let bastion = bastions.into_iter().find(|b| b.ssm_online).ok_or(AppError::NoBastions)?;
    let target_host = addrs.first().map(|a| a.to_string()).unwrap_or_else(|| host.to_string());
    let target_port = if input.starts_with("https://") { 443u16 } else { 80u16 };

    Ok(TunnelTarget::RemoteViaBastion {
        bastion_id: bastion.id,
        bastion_name: bastion.name,
        target_host,
        target_port,
    })
}

pub fn strip_url_to_host(input: &str) -> String {
    input
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(input)
        .split(':')
        .next()
        .unwrap_or(input)
        .to_string()
}

fn dns_lookup(host: &str) -> Vec<std::net::IpAddr> {
    use std::net::ToSocketAddrs;
    match (host, 80u16).to_socket_addrs() {
        Ok(addrs) => addrs.map(|a| a.ip()).collect(),
        Err(_) => vec![],
    }
}

/// Resolve a hostname using an external DNS server (dig @8.8.8.8) to bypass
/// /etc/hosts overrides (e.g. from --proxy).
fn dns_lookup_external(host: &str) -> Vec<std::net::IpAddr> {
    let output = match std::process::Command::new("dig")
        .args(["+short", "@8.8.8.8", host])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<std::net::IpAddr>().ok())
        .collect()
}

/// Resolve a hostname from inside a bastion using SSM send-command + dig.
/// Returns the resolved IPs (one per line) or an error.
/// Note: run_aws always appends --output json, so we parse JSON throughout.
pub fn resolve_via_bastion(bastion_id: &str, host: &str, profile: Option<&str>) -> Result<String> {
    // Send command — run_aws appends --output json, CommandId lives at .Command.CommandId
    let send_json = run_aws(
        &[
            "ssm", "send-command",
            "--instance-ids", bastion_id,
            "--document-name", "AWS-RunShellScript",
            "--parameters", &format!(
                "commands=[\"dig +short {h} 2>/dev/null || host {h} 2>/dev/null | awk '/has address/{{print $4}}' || echo FAIL\"]",
                h = host
            ),
        ],
        profile,
    )?;
    let send_val: serde_json::Value = serde_json::from_str(&send_json)?;
    let command_id = send_val["Command"]["CommandId"]
        .as_str()
        .ok_or_else(|| AppError::AwsCli("send-command: no CommandId".to_string()))?
        .to_string();

    // Give SSM a moment to dispatch before polling
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Poll up to 10 × 2s = 20s
    for _ in 0..10 {
        let inv_json = run_aws(
            &[
                "ssm", "get-command-invocation",
                "--command-id", &command_id,
                "--instance-id", bastion_id,
            ],
            profile,
        );
        if let Ok(j) = inv_json {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&j) {
                let status = val["Status"].as_str().unwrap_or("");
                if status == "Success" || status == "Failed" {
                    let out = val["StandardOutputContent"].as_str().unwrap_or("").trim().to_string();
                    return Ok(out);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    Err(AppError::AwsCli("SSM command timed out after 23 s".to_string()))
}

pub fn sso_login(profile: Option<&str>) -> Result<()> {
    let status = aws_cmd(profile).args(["sso", "login"]).status()?;
    if !status.success() {
        return Err(AppError::AwsCli("aws sso login failed".to_string()));
    }
    Ok(())
}

pub fn get_caller_identity(profile: Option<&str>) -> Result<String> {
    let json = run_aws(&["sts", "get-caller-identity"], profile)?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    Ok(format!(
        "Account: {}\nARN:     {}",
        val["Account"].as_str().unwrap_or("?"),
        val["Arn"].as_str().unwrap_or("?"),
    ))
}

pub fn get_region(profile: Option<&str>) -> String {
    if let Ok(r) = std::env::var("AWS_DEFAULT_REGION") { if !r.is_empty() { return r; } }
    if let Ok(r) = std::env::var("AWS_REGION") { if !r.is_empty() { return r; } }
    if let Ok(o) = aws_cmd(profile).args(["configure", "get", "region"]).output() {
        let r = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !r.is_empty() { return r; }
    }
    "us-east-1".to_string()
}

pub fn get_profile() -> String {
    std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string())
}

/// List all configured AWS profiles by parsing ~/.aws/config and ~/.aws/credentials.
pub fn list_profiles() -> Vec<String> {
    let mut profiles = std::collections::BTreeSet::new();
    profiles.insert("default".to_string());

    let home = std::env::var("HOME").unwrap_or_default();
    for filename in &["config", "credentials"] {
        let path = std::path::Path::new(&home).join(".aws").join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                // [profile foo] in config, [foo] in credentials
                if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    let name = inner.strip_prefix("profile ").unwrap_or(inner).trim();
                    if !name.is_empty() {
                        profiles.insert(name.to_string());
                    }
                }
            }
        }
    }
    profiles.into_iter().collect()
}

// ── ALB-aware tunnel resolution ──────────────────────────────────────────────

/// Find an ALB whose DNS resolves to the same IPs as the given hostname.
/// Returns the ALB ARN if found.
pub fn find_alb_for_hostname(host: &str, profile: Option<&str>) -> Result<Option<String>> {
    let mut resolved = dns_lookup(host);
    // If /etc/hosts overrides to loopback (e.g. from --proxy), use external DNS
    if resolved.iter().all(|ip| ip.is_loopback()) {
        let external = dns_lookup_external(host);
        if !external.is_empty() {
            resolved = external;
        }
    }
    let target_ips: HashSet<String> = resolved
        .into_iter()
        .filter(|a| !a.is_loopback())
        .map(|a| a.to_string())
        .collect();
    if target_ips.is_empty() {
        return Ok(None);
    }

    let json = run_aws(&["elbv2", "describe-load-balancers"], profile)?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    let albs = val["LoadBalancers"].as_array().unwrap_or(&empty);

    for alb in albs {
        let dns_name = alb["DNSName"].as_str().unwrap_or("");
        if dns_name.is_empty() { continue; }
        let alb_ips: HashSet<String> = dns_lookup(dns_name)
            .into_iter()
            .map(|a| a.to_string())
            .collect();
        if !target_ips.is_disjoint(&alb_ips) {
            if let Some(arn) = alb["LoadBalancerArn"].as_str() {
                return Ok(Some(arn.to_string()));
            }
        }
    }
    Ok(None)
}

/// Get healthy targets from an ALB's target groups.
/// If `remote_port` is specified, only return targets whose port matches.
/// Returns Vec<(target_id, port)> where target_id is an IP or instance ID.
pub fn get_alb_healthy_targets(
    alb_arn: &str,
    remote_port: Option<u16>,
    profile: Option<&str>,
) -> Result<Vec<(String, u16)>> {
    let json = run_aws(
        &["elbv2", "describe-target-groups", "--load-balancer-arn", alb_arn],
        profile,
    )?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    let tgs = val["TargetGroups"].as_array().unwrap_or(&empty);

    let mut targets = Vec::new();
    for tg in tgs {
        let tg_arn = match tg["TargetGroupArn"].as_str() {
            Some(a) => a,
            None => continue,
        };
        let tg_port = tg["Port"].as_u64().unwrap_or(0) as u16;

        let health_json = run_aws(
            &["elbv2", "describe-target-health", "--target-group-arn", tg_arn],
            profile,
        )?;
        let health_val: serde_json::Value = serde_json::from_str(&health_json)?;
        let empty2 = Vec::new();
        let descs = health_val["TargetHealthDescriptions"].as_array().unwrap_or(&empty2);

        for desc in descs {
            let state = desc["TargetHealth"]["State"].as_str().unwrap_or("");
            if state != "healthy" { continue; }
            let id = match desc["Target"]["Id"].as_str() {
                Some(id) => id.to_string(),
                None => continue,
            };
            let port = desc["Target"]["Port"].as_u64().unwrap_or(tg_port as u64) as u16;
            if let Some(rp) = remote_port {
                if port != rp { continue; }
            }
            targets.push((id, port));
        }
    }
    Ok(targets)
}

/// Get security group IDs for a target (private IP or instance ID) via ENI lookup.
pub fn get_target_sg_ids(target_id: &str, profile: Option<&str>) -> Result<Vec<String>> {
    let filter = if target_id.starts_with("i-") {
        format!("Name=attachment.instance-id,Values={}", target_id)
    } else {
        format!("Name=addresses.private-ip-address,Values={}", target_id)
    };
    let json = run_aws(
        &["ec2", "describe-network-interfaces", "--filters", &filter],
        profile,
    )?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    let enis = val["NetworkInterfaces"].as_array().unwrap_or(&empty);

    let mut sg_ids = Vec::new();
    for eni in enis {
        let empty2 = Vec::new();
        let groups = eni["Groups"].as_array().unwrap_or(&empty2);
        for g in groups {
            if let Some(id) = g["GroupId"].as_str() {
                if !sg_ids.contains(&id.to_string()) {
                    sg_ids.push(id.to_string());
                }
            }
        }
    }
    Ok(sg_ids)
}

/// Get source security group IDs that are allowed inbound to any of the given
/// SGs on the given port.
pub fn get_allowed_source_sgs(sg_ids: &[String], port: u16, profile: Option<&str>) -> Result<Vec<String>> {
    if sg_ids.is_empty() { return Ok(vec![]); }
    let sg_refs: Vec<&str> = sg_ids.iter().map(|s| s.as_str()).collect();
    let mut args: Vec<&str> = vec!["ec2", "describe-security-groups", "--group-ids"];
    args.extend_from_slice(&sg_refs);

    let json = run_aws(&args, profile)?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    let sgs = val["SecurityGroups"].as_array().unwrap_or(&empty);

    let mut allowed = Vec::new();
    for sg in sgs {
        let empty2 = Vec::new();
        let perms = sg["IpPermissions"].as_array().unwrap_or(&empty2);
        for perm in perms {
            let protocol = perm["IpProtocol"].as_str().unwrap_or("");
            let matches_port = if protocol == "-1" {
                true // all traffic
            } else {
                let from = perm["FromPort"].as_i64().unwrap_or(0) as u16;
                let to = perm["ToPort"].as_i64().unwrap_or(0) as u16;
                from <= port && port <= to
            };
            if !matches_port { continue; }
            let empty3 = Vec::new();
            let pairs = perm["UserIdGroupPairs"].as_array().unwrap_or(&empty3);
            for pair in pairs {
                if let Some(gid) = pair["GroupId"].as_str() {
                    if !allowed.contains(&gid.to_string()) {
                        allowed.push(gid.to_string());
                    }
                }
            }
        }
    }
    Ok(allowed)
}

/// Find an SSM-online, running EC2 instance that belongs to one of the given
/// security groups.
pub fn find_ssm_hop_by_sgs(allowed_sg_ids: &[String], profile: Option<&str>) -> Result<Option<Instance>> {
    let allowed_set: HashSet<&str> = allowed_sg_ids.iter().map(|s| s.as_str()).collect();
    let instances = list_instances(profile)?;
    Ok(instances
        .into_iter()
        .filter(|i| i.ssm_status == SsmStatus::Online && i.state == InstanceState::Running)
        .find(|i| i.security_group_ids.iter().any(|sg| allowed_set.contains(sg.as_str()))))
}

/// Common service ports to probe when auto-detecting.
pub const COMMON_PORTS: &[u16] = &[80, 443, 3000, 5000, 8000, 8080, 8443, 8501, 8888, 9000, 9090];

/// Probe which ports are open on a remote host from a bastion via SSM
/// send-command. Uses perl `IO::Socket::INET` (available on macOS, Amazon
/// Linux, and Ubuntu) for portable TCP connect with timeout.
pub fn probe_ports_via_bastion(
    bastion_id: &str,
    host: &str,
    ports: &[u16],
    profile: Option<&str>,
) -> Result<Vec<u16>> {
    let checks: Vec<String> = ports
        .iter()
        .map(|p| format!(
            "perl -MIO::Socket::INET -e 'exit(IO::Socket::INET->new(PeerAddr=>$ARGV[0],PeerPort=>$ARGV[1],Timeout=>1)?0:1)' {} {} 2>/dev/null && echo {}",
            host, p, p
        ))
        .collect();
    let script = checks.join("; ");

    let send_json = run_aws(
        &[
            "ssm", "send-command",
            "--instance-ids", bastion_id,
            "--document-name", "AWS-RunShellScript",
            "--parameters", &format!("commands=[\"{}\"]", script),
        ],
        profile,
    )?;
    let send_val: serde_json::Value = serde_json::from_str(&send_json)?;
    let command_id = send_val["Command"]["CommandId"]
        .as_str()
        .ok_or_else(|| AppError::AwsCli("send-command: no CommandId".to_string()))?
        .to_string();

    std::thread::sleep(std::time::Duration::from_secs(3));

    for _ in 0..10 {
        let inv_json = run_aws(
            &[
                "ssm", "get-command-invocation",
                "--command-id", &command_id,
                "--instance-id", bastion_id,
            ],
            profile,
        );
        if let Ok(j) = inv_json {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&j) {
                let status = val["Status"].as_str().unwrap_or("");
                if status == "Success" || status == "Failed" {
                    let out = val["StandardOutputContent"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    return Ok(out
                        .lines()
                        .filter_map(|line| line.trim().parse::<u16>().ok())
                        .collect());
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    Err(AppError::AwsCli("Port probe timed out after 23 s".to_string()))
}

// ── ECR ───────────────────────────────────────────────────────────────────────

/// A single ECR image entry (mirrors a `docker images` row).
pub struct EcrImage {
    pub repository: String,
    /// First tag, or `<none>` for untagged images.
    pub tag: String,
    /// Short image digest (first 12 hex chars after `sha256:`).
    pub image_id: String,
    /// Unix timestamp (seconds) when the image was pushed.
    pub pushed_at: f64,
    pub size_bytes: u64,
}

impl EcrImage {
    /// Human-readable size: bytes → "1.23 GB" / "456 MB" / "789 KB".
    pub fn human_size(&self) -> String {
        let b = self.size_bytes as f64;
        if b >= 1_073_741_824.0 {
            format!("{:.2} GB", b / 1_073_741_824.0)
        } else if b >= 1_048_576.0 {
            format!("{:.0} MB", b / 1_048_576.0)
        } else {
            format!("{:.0} KB", b / 1_024.0)
        }
    }

    /// Relative time string like "3 weeks ago".
    pub fn relative_pushed_at(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(self.pushed_at);
        let secs = (now - self.pushed_at).max(0.0) as u64;
        if secs < 60 {
            "just now".to_string()
        } else if secs < 3_600 {
            let m = secs / 60;
            format!("{} minute{} ago", m, if m == 1 { "" } else { "s" })
        } else if secs < 86_400 {
            let h = secs / 3_600;
            format!("{} hour{} ago", h, if h == 1 { "" } else { "s" })
        } else if secs < 604_800 {
            let d = secs / 86_400;
            format!("{} day{} ago", d, if d == 1 { "" } else { "s" })
        } else if secs < 2_592_000 {
            let w = secs / 604_800;
            format!("{} week{} ago", w, if w == 1 { "" } else { "s" })
        } else if secs < 31_536_000 {
            let mo = secs / 2_592_000;
            format!("{} month{} ago", mo, if mo == 1 { "" } else { "s" })
        } else {
            let y = secs / 31_536_000;
            format!("{} year{} ago", y, if y == 1 { "" } else { "s" })
        }
    }
}

/// Parse an ISO 8601 datetime string (e.g. `"2024-01-15T10:30:00+00:00"`) to a
/// Unix timestamp in seconds.  No external crates required.
fn parse_iso8601_to_unix(s: &str) -> Option<f64> {
    if s.len() < 19 { return None; }
    let year:  i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day:   i64 = s[8..10].parse().ok()?;
    let hour:  i64 = s[11..13].parse().ok()?;
    let min:   i64 = s[14..16].parse().ok()?;
    let sec:   i64 = s[17..19].parse().ok()?;

    // Skip optional fractional seconds, then read timezone
    let rest = &s[19..];
    let tz_rest = if rest.starts_with('.') {
        let end = rest.find(|c: char| c == '+' || c == '-' || c == 'Z').unwrap_or(rest.len());
        &rest[end..]
    } else {
        rest
    };
    let tz_offset_secs: i64 = if tz_rest.is_empty() || tz_rest.starts_with('Z') {
        0
    } else {
        let sign: i64 = if tz_rest.starts_with('-') { -1 } else { 1 };
        let parts: Vec<&str> = tz_rest[1..].split(':').collect();
        let tz_h: i64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let tz_m: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        sign * (tz_h * 3_600 + tz_m * 60)
    };

    fn is_leap(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }
    fn days_in_month(y: i64, m: i64) -> i64 {
        match m { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 => if is_leap(y) { 29 } else { 28 }, _ => 0 }
    }
    fn days_before_year(y: i64) -> i64 { let y = y - 1; y*365 + y/4 - y/100 + y/400 }

    let epoch = days_before_year(1970);
    let mut days = days_before_year(year);
    for m in 1..month { days += days_in_month(year, m); }
    days += day - 1;

    Some(((days - epoch) * 86_400 + hour * 3_600 + min * 60 + sec - tz_offset_secs) as f64)
}

/// Given multiple tags for the same image, return their longest common prefix
/// with trailing separators (`-`, `_`, `.`) stripped.
/// E.g. `["v5.4.0-production-3cac2d2", "v5.4.0-production-latest"]` → `"v5.4.0-production"`.
fn common_tag_prefix(tags: &[&str]) -> String {
    match tags {
        [] => "<none>".to_string(),
        [single] => single.to_string(),
        [first, rest @ ..] => {
            let mut prefix_len = first.len();
            for tag in rest {
                let common = first.chars().zip(tag.chars()).take_while(|(a, b)| a == b).count();
                prefix_len = prefix_len.min(common);
            }
            first[..prefix_len]
                .trim_end_matches(|c| c == '-' || c == '_' || c == '.')
                .to_string()
        }
    }
}

/// Strip the trailing `-<commit>` or `-latest` suffix from a tag to get its
/// base prefix.  E.g. `"forecast-3.0-01d4e46"` → `"forecast-3.0"`.
/// A commit suffix is 7 lowercase hex characters.
fn tag_base_prefix(tag: &str) -> &str {
    if let Some(pos) = tag.rfind('-') {
        let suffix = &tag[pos + 1..];
        let is_commit = suffix.len() == 7 && suffix.bytes().all(|b| b.is_ascii_hexdigit());
        if is_commit || suffix == "latest" {
            return &tag[..pos];
        }
    }
    tag
}

/// Keep only the newest image per tag base-prefix.
/// Input must already be sorted newest-first (as returned by `list_ecr_images`).
pub fn filter_latest_images(images: Vec<EcrImage>) -> Vec<EcrImage> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    images
        .into_iter()
        .filter(|img| seen.insert(format!("{}:{}", img.repository, tag_base_prefix(&img.tag))))
        .collect()
}

/// List all ECR repository names in the account/region.
pub fn list_ecr_repositories(region: Option<&str>, profile: Option<&str>) -> Result<Vec<String>> {
    let mut args = vec!["ecr", "describe-repositories"];
    if let Some(r) = region {
        args.extend_from_slice(&["--region", r]);
    }
    let json = run_aws(&args, profile)?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    Ok(val["repositories"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|r| r["repositoryName"].as_str().map(|s| s.to_string()))
        .collect())
}

/// List all images in an ECR repository, sorted newest-first.
pub fn list_ecr_images(
    repository: &str,
    region: Option<&str>,
    profile: Option<&str>,
) -> Result<Vec<EcrImage>> {
    let mut args = vec!["ecr", "describe-images", "--repository-name", repository];
    if let Some(r) = region {
        args.extend_from_slice(&["--region", r]);
    }
    let json = run_aws(&args, profile)?;
    let val: serde_json::Value = serde_json::from_str(&json)?;
    let empty = Vec::new();
    let details = val["imageDetails"].as_array().unwrap_or(&empty);

    // Build full ECR URI: {accountId}.dkr.ecr.{region}.amazonaws.com/{repo}
    let effective_region = region
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_region(profile));
    let registry_id = details
        .first()
        .and_then(|d| d["registryId"].as_str())
        .unwrap_or("");
    let full_repository = if registry_id.is_empty() {
        repository.to_string()
    } else {
        format!("{}.dkr.ecr.{}.amazonaws.com/{}", registry_id, effective_region, repository)
    };

    let mut images: Vec<EcrImage> = details
        .iter()
        .map(|d| {
            let raw_tags: Vec<&str> = d["imageTags"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let tag = common_tag_prefix(&raw_tags);
            let digest = d["imageDigest"].as_str().unwrap_or("");
            let image_id = digest
                .strip_prefix("sha256:")
                .unwrap_or(digest)
                .chars()
                .take(12)
                .collect();
            // imagePushedAt may be a float (some CLI versions) or an ISO 8601 string
            let pushed_at = d["imagePushedAt"]
                .as_f64()
                .or_else(|| d["imagePushedAt"].as_str().and_then(parse_iso8601_to_unix))
                .unwrap_or(0.0);
            let size_bytes = d["imageSizeInBytes"].as_u64().unwrap_or(0);
            EcrImage { repository: full_repository.clone(), tag, image_id, pushed_at, size_bytes }
        })
        .collect();

    images.sort_by(|a, b| b.pushed_at.partial_cmp(&a.pushed_at).unwrap_or(std::cmp::Ordering::Equal));
    Ok(images)
}

/// Human-readable DNS → EC2 resolution report.
pub fn resolve_dns_report(input: &str, profile: Option<&str>) -> Result<String> {
    use std::fmt::Write as _;
    let host = strip_url_to_host(input);
    let mut out = String::new();
    writeln!(out, "Resolving: {}", host).ok();

    // ── Step 1: local DNS ────────────────────────────────────────────────────
    let addrs = dns_lookup(&host);
    let locally_resolved = !addrs.is_empty();

    if locally_resolved {
        writeln!(out, "  DNS (local): {}",
            addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")).ok();
    } else {
        writeln!(out, "  DNS (local): not resolvable — likely an internal hostname").ok();
    }

    // ── Step 2: fetch EC2 + bastions ─────────────────────────────────────────
    let instances = list_instances(profile)?;
    let bastions = find_bastions(profile).unwrap_or_default();
    let online_bastions: Vec<&BastionInfo> = bastions.iter().filter(|b| b.ssm_online).collect();

    // ── Step 3: direct IP → EC2 match ────────────────────────────────────────
    let mut direct_match = false;
    for addr in &addrs {
        let addr_str = addr.to_string();
        for inst in instances.iter().filter(|i| {
            i.private_ip.as_deref() == Some(&addr_str)
                || i.public_ip.as_deref() == Some(&addr_str)
        }) {
            direct_match = true;
            writeln!(out, "\n  EC2 match: {} ({})", inst.name, inst.id).ok();
            writeln!(out, "    type={} state={} ssm={}",
                inst.instance_type, inst.state.as_str(), inst.ssm_status.as_str()).ok();
        }
    }

    // ── Step 4: if no local match, try resolving via bastion ─────────────────
    if !direct_match {
        if online_bastions.is_empty() {
            writeln!(out, "\n  No SSM-online bastions found to try remote resolution.").ok();
        } else {
            writeln!(out, "\n  No direct EC2 IP match.").ok();

            // Try to resolve hostname from the first available bastion
            let bastion = online_bastions[0];
            writeln!(out, "\n  Trying resolution via bastion: {} ({})", bastion.name, bastion.id).ok();

            match resolve_via_bastion(&bastion.id, &host, profile) {
                Ok(result) if !result.is_empty() && result != "FAIL" => {
                    writeln!(out, "  DNS (from bastion): {}", result.lines().next().unwrap_or(&result)).ok();
                    // Check if bastion-resolved IP matches an EC2 instance
                    for line in result.lines() {
                        let trimmed = line.trim();
                        if trimmed.parse::<std::net::IpAddr>().is_ok() {
                            if let Some(inst) = instances.iter().find(|i|
                                i.private_ip.as_deref() == Some(trimmed)
                            ) {
                                writeln!(out, "  EC2 match: {} ({}) — reachable via bastion", inst.name, inst.id).ok();
                            }
                        }
                    }
                }
                Ok(_) => {
                    writeln!(out, "  Bastion could not resolve {} either (not in VPC DNS?)", host).ok();
                }
                Err(e) => {
                    writeln!(out, "  Bastion resolution failed: {}", e).ok();
                }
            }

            // List all available bastions
            writeln!(out, "\n  Available SSM-online bastions:").ok();
            for b in &online_bastions {
                writeln!(out, "    ● {} ({})", b.name, b.id).ok();
            }
            writeln!(out, "\n  Tunnel suggestion: awsx2 tunnel-url {} <local_port>", input).ok();
        }
    }

    Ok(out)
}
