//! Self-healing tunnel supervisor. Establishes each tunnel unit, monitors its
//! health, and re-establishes it with exponential backoff when it drops.
//! `supervise_all` runs one watcher thread per spec and is the entry point for
//! both `tunnel-up` and a single `tunnel --keep-alive`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::registry::{self, TunnelSpec};
use crate::tunnel;

/// Set by the OS signal handler; observed by every watcher thread and the main
/// loop so a single Ctrl-C / SIGTERM tears the whole supervisor down cleanly.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

const HEALTH_INTERVAL: Duration = Duration::from_millis(5000);
const TICK: Duration = Duration::from_millis(500);

// ── Pure helpers (unit-tested) ────────────────────────────────────────────────

/// Exponential backoff in seconds: 1, 2, 4, 8, 16, then capped at 30.
pub fn backoff_secs(consecutive_failures: u32) -> u64 {
    (1u64 << consecutive_failures.min(5)).min(30)
}

/// Current UTC time-of-day as `HH:MM:SS`, computed without external crates.
pub fn now_hms() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

fn log(msg: &str) {
    eprintln!("[{}] {}", now_hms(), msg);
}

// ── Signal handling ───────────────────────────────────────────────────────────

extern "C" fn handle_signal(_sig: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn install_signal_handler() {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGINT, handle_signal as usize);
        libc::signal(libc::SIGTERM, handle_signal as usize);
    }
}

fn should_stop(local: &Arc<AtomicBool>) -> bool {
    local.load(Ordering::SeqCst) || SHUTDOWN.load(Ordering::SeqCst)
}

/// Sleep up to `secs`, waking early if shutdown is requested.
fn sleep_interruptible(secs: u64, shutdown: &Arc<AtomicBool>) {
    let ticks = secs * 2; // TICK = 500ms
    for _ in 0..ticks {
        if should_stop(shutdown) {
            return;
        }
        thread::sleep(TICK);
    }
}

// ── Pid file ──────────────────────────────────────────────────────────────────

pub fn pid_file_path() -> Result<PathBuf> {
    Ok(registry::config_dir()?.join("supervisor.pid"))
}

pub fn write_pid_file() -> Result<()> {
    let p = pid_file_path()?;
    std::fs::write(&p, std::process::id().to_string())
        .map_err(|e| AppError::Registry(format!("write pid file: {}", e)))?;
    Ok(())
}

/// Return the supervisor pid if one is recorded AND still alive. A stale pid
/// file (process gone) is removed and `None` is returned, avoiding signalling a
/// reused pid.
pub fn read_pid_file() -> Option<u32> {
    let p = pid_file_path().ok()?;
    let pid: u32 = std::fs::read_to_string(&p).ok()?.trim().parse().ok()?;
    if tunnel::pid_alive(pid) {
        Some(pid)
    } else {
        let _ = std::fs::remove_file(&p);
        None
    }
}

pub fn clear_pid_file() {
    if let Ok(p) = pid_file_path() {
        let _ = std::fs::remove_file(p);
    }
}

// ── Supervision ───────────────────────────────────────────────────────────────

/// Keep a single tunnel unit alive until shutdown is requested.
pub fn supervise_one(spec: TunnelSpec, shutdown: Arc<AtomicBool>) {
    let name = spec.display_name();
    let mut failures: u32 = 0;

    while !should_stop(&shutdown) {
        match tunnel::establish_unit(&spec) {
            Ok(unit) => {
                failures = 0;
                log(&format!(
                    "[{}] up: localhost:{} -> {}:{}",
                    name, spec.local_port, unit.instance_name, spec.remote_port
                ));

                // Monitor until the unit becomes unhealthy or we are asked to stop.
                loop {
                    let waited = wait_health_interval(&shutdown);
                    if should_stop(&shutdown) {
                        break;
                    }
                    if !waited {
                        continue;
                    }
                    if !tunnel::is_unit_healthy(&unit) {
                        log(&format!(
                            "[{}] tunnel dropped on :{}, reconnecting...",
                            name, spec.local_port
                        ));
                        break;
                    }
                }
                tunnel::stop_unit(&unit);
            }
            Err(e) => {
                let wait = backoff_secs(failures);
                log(&format!(
                    "[{}] establish failed: {} (retry in {}s; if SSO expired run: awsx2 login)",
                    name, e, wait
                ));
                sleep_interruptible(wait, &shutdown);
                failures = failures.saturating_add(1);
            }
        }
    }
    log(&format!("[{}] supervisor for :{} exiting", name, spec.local_port));
}

/// Sleep one health interval in small ticks. Returns true if the full interval
/// elapsed, false if interrupted by a shutdown request.
fn wait_health_interval(shutdown: &Arc<AtomicBool>) -> bool {
    let ticks = HEALTH_INTERVAL.as_millis() / TICK.as_millis(); // 10
    for _ in 0..ticks {
        if should_stop(shutdown) {
            return false;
        }
        thread::sleep(TICK);
    }
    true
}

/// Establish and supervise every spec in one process. Blocks until a shutdown
/// signal, then tears everything down. Entry point for `tunnel-up` and for a
/// single `tunnel --keep-alive` (called with a one-element vec).
pub fn supervise_all(specs: Vec<TunnelSpec>) -> Result<()> {
    if specs.is_empty() {
        return Err(AppError::Registry("no tunnels to supervise".into()));
    }
    install_signal_handler();
    write_pid_file()?;
    log(&format!("supervisor pid {} managing {} tunnel(s)", std::process::id(), specs.len()));

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    for spec in specs {
        let sd = shutdown.clone();
        handles.push(thread::spawn(move || supervise_one(spec, sd)));
    }

    // Wait for a signal (set by the handler into the global flag).
    while !SHUTDOWN.load(Ordering::SeqCst) {
        thread::sleep(TICK);
    }

    log("shutdown requested; stopping all tunnels...");
    shutdown.store(true, Ordering::SeqCst);
    for h in handles {
        let _ = h.join();
    }
    tunnel::stop_all_tunnels();
    clear_pid_file();
    log("all tunnels stopped.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_secs(0), 1);
        assert_eq!(backoff_secs(1), 2);
        assert_eq!(backoff_secs(2), 4);
        assert_eq!(backoff_secs(3), 8);
        assert_eq!(backoff_secs(4), 16);
        assert_eq!(backoff_secs(5), 30);
        assert_eq!(backoff_secs(10), 30);
    }

    #[test]
    fn hms_is_well_formed() {
        let s = now_hms();
        assert_eq!(s.len(), 8);
        assert_eq!(s.as_bytes()[2], b':');
        assert_eq!(s.as_bytes()[5], b':');
    }
}
