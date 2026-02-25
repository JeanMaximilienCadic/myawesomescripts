//! VPN connection via AWS Client VPN with SAML authentication.
//!
//! Flow: openvpn → SAML URL → headless browser (SSO login) → SAML callback → VPN connect → DNS config.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use regex::Regex;

use crate::error::{AppError, Result};
use crate::models::VpnConfig;

const SAML_LISTEN_PORT: u16 = 35001;
const AWS_OVPN_DIR: &str = "/opt/awsvpnclient/Service/Resources/openvpn";

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
    // chmod 600 — contains password
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ── OpenVPN binary detection ─────────────────────────────────────────────────

fn aws_openvpn_available() -> bool {
    let musl = format!("{}/ld-musl-x86_64.so.1", AWS_OVPN_DIR);
    let acvc = format!("{}/acvc-openvpn", AWS_OVPN_DIR);
    std::path::Path::new(&musl).exists() && std::path::Path::new(&acvc).exists()
}

fn openvpn_cmd(config_path: &str, creds_path: &str) -> Command {
    if aws_openvpn_available() {
        let musl = format!("{}/ld-musl-x86_64.so.1", AWS_OVPN_DIR);
        let acvc = format!("{}/acvc-openvpn", AWS_OVPN_DIR);
        let mut cmd = Command::new(musl);
        cmd.args(["--library-path", AWS_OVPN_DIR, &acvc]);
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
}

pub fn fetch_saml_challenge(ovpn_config_path: &str) -> Result<SamlChallenge> {
    let creds = write_creds("N/A", &format!("ACS::{}", SAML_LISTEN_PORT))?;

    let mut child = openvpn_cmd(ovpn_config_path, creds.path().to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait up to 20 seconds for openvpn to exit (it will fail auth and exit)
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

    Ok(SamlChallenge { saml_url, sid })
}

// ── Phase 2: SAML callback HTTP listener ─────────────────────────────────────

fn wait_for_saml_callback(timeout: Duration) -> Result<String> {
    let server = tiny_http::Server::http(format!("127.0.0.1:{}", SAML_LISTEN_PORT))
        .map_err(|e| AppError::Vpn(format!("Cannot bind SAML listener on port {}: {}", SAML_LISTEN_PORT, e)))?;

    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() > deadline {
            return Err(AppError::SamlAuth("SAML callback timeout (no response received)".into()));
        }

        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(mut request)) => {
                // Try POST body first, then query string
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
                    return Ok(saml);
                }
            }
            Ok(None) => continue,
            Err(e) => return Err(AppError::Vpn(format!("SAML listener error: {}", e))),
        }
    }
}

fn extract_saml_from_form(body: &str) -> Option<String> {
    url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == "SAMLResponse")
        .map(|(_, val)| val.to_string())
}

fn extract_saml_from_query(url_str: &str) -> Option<String> {
    let full = format!("http://localhost{}", url_str);
    url::Url::parse(&full)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "SAMLResponse")
        .map(|(_, val)| val.to_string())
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

    // Navigate to SAML URL
    tab.navigate_to(saml_url)
        .map_err(|e| AppError::Browser(format!("Navigation failed: {}", e)))?;
    tab.wait_until_navigated()
        .map_err(|e| AppError::Browser(format!("Wait failed: {}", e)))?;

    std::thread::sleep(Duration::from_secs(3));

    // Step A: Username
    let username_selectors = &[
        "input[type='email']",
        "input[name='username']",
        "input[name='email']",
        "#awsui-input-0",
        "input[data-testid='username-input']",
    ];
    fill_field_and_submit(&tab, username_selectors, sso_user)?;
    std::thread::sleep(Duration::from_secs(3));

    // Step B: Password
    let password_selectors = &[
        "input[type='password']",
        "input[name='password']",
        "#awsui-input-1",
        "input[data-testid='password-input']",
    ];
    fill_field_and_submit(&tab, password_selectors, sso_pass)?;
    std::thread::sleep(Duration::from_secs(4));

    // Step C: MFA
    let mfa_selectors = &[
        "input[placeholder='Enter code']",
        "input[placeholder*='code']",
        "input[name='mfaCode']",
        "input[name='totp']",
        "input[type='tel']",
        "input[data-testid='mfa-code-input']",
        "input[inputmode='numeric']",
    ];
    fill_field_and_submit(&tab, mfa_selectors, mfa_code)?;
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

            // Try submit buttons
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

            // Wait for navigation
            let _ = tab.wait_until_navigated();
            return Ok(());
        }
    }
    // Field not found — might not be present (e.g., no MFA step), not an error
    Ok(())
}

// ── Phase 4: Connect VPN with SAML token ─────────────────────────────────────

fn start_vpn_process(
    ovpn_config_path: &str,
    sid: &str,
    saml_response: &str,
) -> Result<(u32, tempfile::NamedTempFile, tempfile::NamedTempFile)> {
    // We need to keep the temp config alive because we passed ovpn_config_path from
    // a NamedTempFile in the caller. But here we just create the creds file.
    let creds = write_creds("N/A", &format!("CRV1::{}::{}", sid, saml_response))?;

    let child = openvpn_cmd(ovpn_config_path, creds.path().to_str().unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    std::mem::forget(child); // Detach — same pattern as tunnel.rs

    // Return creds so caller can keep them alive
    // Config tempfile must also stay alive — caller manages that
    // We create a dummy for the config slot
    let dummy = tempfile::NamedTempFile::new()?;
    Ok((pid, creds, dummy))
}

// ── Phase 5: DNS configuration ───────────────────────────────────────────────

pub fn configure_dns(dns_server: &str, dns_domain: &str) -> Result<()> {
    if dns_server.is_empty() || dns_domain.is_empty() {
        // No DNS config specified — skip silently
        return Ok(());
    }
    // Wait for tun0 to come up (up to 20 seconds)
    let start = Instant::now();
    let mut tun_up = false;
    while start.elapsed() < Duration::from_secs(20) {
        let status = Command::new("ip")
            .args(["link", "show", "tun0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.map_or(false, |s| s.success()) {
            tun_up = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !tun_up {
        return Err(AppError::Vpn("tun0 did not come up within 20 seconds".into()));
    }

    // Give it a moment to stabilize
    std::thread::sleep(Duration::from_secs(1));

    let _ = Command::new("resolvectl")
        .args(["dns", "tun0", dns_server])
        .status()
        .map_err(|e| AppError::Vpn(format!("resolvectl dns failed: {}", e)))?;

    let _ = Command::new("resolvectl")
        .args(["domain", "tun0", dns_domain])
        .status()
        .map_err(|e| AppError::Vpn(format!("resolvectl domain failed: {}", e)))?;

    let _ = Command::new("resolvectl")
        .args(["default-route", "tun0", "false"])
        .status();

    Ok(())
}

// ── Status detection ─────────────────────────────────────────────────────────

pub fn is_connected() -> bool {
    Command::new("ip")
        .args(["link", "show", "tun0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_or(false, |s| s.success())
}

pub fn get_vpn_ip() -> Option<String> {
    let output = Command::new("ip")
        .args(["-4", "addr", "show", "tun0"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"inet (\d+\.\d+\.\d+\.\d+)").ok()?;
    re.captures(&stdout)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
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
    // Also try pkill as fallback
    let _ = Command::new("pkill").args(["-f", "acvc-openvpn"]).status();
}

// ── High-level orchestration ─────────────────────────────────────────────────

/// Full VPN connection flow. Returns the openvpn PID on success.
/// `progress` callback is called with status messages for UI feedback.
pub fn connect<F>(config: &VpnConfig, mfa_code: &str, mut progress: F) -> Result<u32>
where
    F: FnMut(&str),
{
    // Validate config
    if config.ovpn_path.is_empty() {
        return Err(AppError::Vpn("No .ovpn file path configured. Run 'awsx2 vpn setup' first.".into()));
    }
    if config.sso_username.is_empty() || config.sso_password.is_empty() {
        return Err(AppError::Vpn("SSO credentials not configured. Run 'awsx2 vpn setup' first.".into()));
    }

    // 1. Prepare modified .ovpn config
    progress("[1/5] Preparing VPN config...");
    let modified_config = prepare_ovpn_config(&config.ovpn_path)?;
    let config_path = modified_config.path().to_str().unwrap().to_string();

    // 2. Get SAML challenge from VPN server
    progress("[2/5] Fetching SAML URL from VPN server...");
    let challenge = fetch_saml_challenge(&config_path)?;
    progress(&format!("  SAML URL received ({} chars), SID: {}...",
        challenge.saml_url.len(),
        &challenge.sid[..challenge.sid.len().min(30)]));

    // 3. Start SAML callback listener + browser in parallel
    progress("[3/5] Completing SAML authentication (headless browser)...");

    let saml_url = challenge.saml_url.clone();
    let user = config.sso_username.clone();
    let pass = config.sso_password.clone();
    let mfa = mfa_code.to_string();

    // Browser thread
    let browser_handle = std::thread::spawn(move || {
        complete_saml_auth(&saml_url, &user, &pass, &mfa)
    });

    // SAML callback listener (blocks until response or timeout)
    let saml_response = wait_for_saml_callback(Duration::from_secs(120))?;
    progress(&format!("  SAML response captured ({} chars)", saml_response.len()));

    // Wait for browser to finish
    let _ = browser_handle.join().map_err(|_| AppError::Browser("Browser thread panicked".into()))?;

    // 4. Connect VPN with SAML token
    progress("[4/5] Connecting VPN with SAML token...");
    let (pid, _creds, _dummy) = start_vpn_process(&config_path, &challenge.sid, &saml_response)?;

    // Keep temp files alive by leaking them (they must outlive the openvpn process)
    std::mem::forget(modified_config);
    std::mem::forget(_creds);

    // 5. Configure DNS
    progress("[5/5] Waiting for tun0 and configuring DNS...");
    configure_dns(&config.dns_server, &config.dns_domain)?;

    let ip = get_vpn_ip().unwrap_or_else(|| "unknown".into());
    progress(&format!("VPN connected! IP: {}, PID: {}", ip, pid));

    Ok(pid)
}
