//! macOS LaunchAgent integration: install/uninstall a `com.awsx2.tunnels`
//! agent that runs `awsx2 tunnel-up` at login and keeps it alive, so persistent
//! tunnels survive logout/reboot and supervisor crashes.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, Result};

pub const LABEL: &str = "com.awsx2.tunnels";

/// Default PATH for the agent so launchd (which has a minimal environment) can
/// find `aws`, `session-manager-plugin`, and `socat`.
const DEFAULT_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

fn logs_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Logs"))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Render the LaunchAgent plist. Pure except for resolving the log directory
/// from `$HOME`.
pub fn plist_xml(exe: &str, path_env: &str, extra_env: &[(String, String)]) -> String {
    let logs = logs_dir();
    let out = logs.join("awsx2-tunnels.out.log");
    let err = logs.join("awsx2-tunnels.err.log");

    let mut env_entries = format!("    <key>PATH</key>\n    <string>{}</string>\n", path_env);
    for (k, v) in extra_env {
        env_entries.push_str(&format!("    <key>{}</key>\n    <string>{}</string>\n", k, v));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>tunnel-up</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>EnvironmentVariables</key>
  <dict>
{env_entries}  </dict>
  <key>StandardOutPath</key>
  <string>{out}</string>
  <key>StandardErrorPath</key>
  <string>{err}</string>
</dict>
</plist>
"#,
        label = LABEL,
        exe = exe,
        env_entries = env_entries,
        out = out.display(),
        err = err.display(),
    )
}

fn plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Launchd("cannot determine home directory".into()))?;
    let dir = home.join("Library/LaunchAgents");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Launchd(format!("create {}: {}", dir.display(), e)))?;
    Ok(dir.join(format!("{}.plist", LABEL)))
}

pub fn plist_location() -> Result<PathBuf> {
    plist_path()
}

fn collect_aws_env() -> Vec<(String, String)> {
    let mut extra = Vec::new();
    for k in ["AWS_PROFILE", "AWS_REGION", "AWS_DEFAULT_REGION"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                extra.push((k.to_string(), v));
            }
        }
    }
    extra
}

pub fn install() -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(AppError::Launchd(
            "macOS only — on Linux run `awsx2 tunnel-up` under nohup or a systemd user unit".into(),
        ));
    }

    let exe = std::env::current_exe()
        .map_err(|e| AppError::Launchd(format!("cannot resolve current exe: {}", e)))?
        .to_string_lossy()
        .to_string();

    let xml = plist_xml(&exe, DEFAULT_PATH, &collect_aws_env());
    let plist = plist_path()?;
    std::fs::write(&plist, xml).map_err(|e| AppError::Launchd(format!("write plist: {}", e)))?;

    let uid = unsafe { libc::getuid() };
    let target = format!("gui/{}", uid);
    let plist_str = plist.to_string_lossy().to_string();

    // Replace any previous instance, then bootstrap the fresh one.
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{}/{}", target, LABEL)])
        .output();

    let boot = Command::new("launchctl")
        .args(["bootstrap", &target, &plist_str])
        .output()
        .map_err(|e| AppError::Launchd(format!("launchctl bootstrap: {}", e)))?;

    if !boot.status.success() {
        // Fall back to the legacy interface on older macOS.
        let load = Command::new("launchctl")
            .args(["load", "-w", &plist_str])
            .output()
            .map_err(|e| AppError::Launchd(format!("launchctl load: {}", e)))?;
        if !load.status.success() {
            return Err(AppError::Launchd(format!(
                "launchctl bootstrap/load failed: {}",
                String::from_utf8_lossy(&boot.stderr).trim()
            )));
        }
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(AppError::Launchd("macOS only".into()));
    }
    let uid = unsafe { libc::getuid() };
    let plist = plist_path()?;
    let plist_str = plist.to_string_lossy().to_string();

    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{}/{}", uid, LABEL)])
        .output();
    let _ = Command::new("launchctl")
        .args(["unload", "-w", &plist_str])
        .output();

    if plist.exists() {
        std::fs::remove_file(&plist)
            .map_err(|e| AppError::Launchd(format!("remove plist: {}", e)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_has_required_keys() {
        let xml = plist_xml(
            "/usr/local/bin/awsx2",
            "/opt/homebrew/bin:/usr/bin",
            &[("AWS_PROFILE".into(), "yoii".into())],
        );
        assert!(xml.contains("com.awsx2.tunnels"));
        assert!(xml.contains("<string>/usr/local/bin/awsx2</string>"));
        assert!(xml.contains("<string>tunnel-up</string>"));
        assert!(xml.contains("RunAtLoad"));
        assert!(xml.contains("KeepAlive"));
        assert!(xml.contains("AWS_PROFILE"));
        assert!(xml.contains("/opt/homebrew/bin:/usr/bin"));
    }
}
