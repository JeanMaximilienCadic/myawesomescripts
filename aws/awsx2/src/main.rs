//! awsx2 — AWS management CLI/TUI
//!
//! No args  → TUI mode (ratatui full-screen)
//! With args → non-interactive CLI (same functionality as the bash awsx)

mod aws;
mod error;
mod models;
mod proxy;
mod tunnel;
mod tui;
mod vpn;

use std::io;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, Args};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::tui::app::{App, ConfirmTag, InputTag, Popup, Tab};
use crate::tui::pages;

// ── ANSI helpers ─────────────────────────────────────────────────────────────

fn gray(s: impl std::fmt::Display) -> String {
    format!("\x1b[90m{}\x1b[0m", s)
}

fn find_pid_on_port(port: u16) -> Option<u32> {
    let out = std::process::Command::new("lsof")
        .args(["-t", "-i", &format!("TCP:{}", port), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()
}

fn confirm_and_kill_port(port: u16) -> bool {
    match find_pid_on_port(port) {
        Some(pid) => {
            eprint!("Port {} in use by PID {}. Kill it and proceed? [y/N] ", port, pid);
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let mut s = String::new();
            if std::io::stdin().read_line(&mut s).is_err() { return false; }
            if s.trim().eq_ignore_ascii_case("y") {
                #[cfg(unix)]
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
                std::thread::sleep(std::time::Duration::from_millis(500));
                true
            } else {
                false
            }
        }
        None => {
            eprintln!("Port {} already in use (could not identify owning process).", port);
            false
        }
    }
}

// ── CLI spinner ───────────────────────────────────────────────────────────────

struct Spinner {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    msg:  std::sync::Arc<std::sync::Mutex<String>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    fn new(initial: &str) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let msg  = std::sync::Arc::new(std::sync::Mutex::new(initial.to_string()));
        let stop_c = stop.clone();
        let msg_c  = msg.clone();
        let handle = std::thread::spawn(move || {
            let frames = ['⠋','⠙','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];
            let mut i = 0usize;
            while !stop_c.load(std::sync::atomic::Ordering::Relaxed) {
                let m = msg_c.lock().unwrap().clone();
                print!("\r{} {}", frames[i % frames.len()], m);
                let _ = std::io::Write::flush(&mut std::io::stdout());
                std::thread::sleep(std::time::Duration::from_millis(80));
                i += 1;
            }
            let m = msg_c.lock().unwrap().clone();
            print!("\r{}\r", " ".repeat(m.len() + 4));
            let _ = std::io::Write::flush(&mut std::io::stdout());
        });
        Self { stop, msg, handle: Some(handle) }
    }

    fn set(&self, new_msg: &str) {
        *self.msg.lock().unwrap() = new_msg.to_string();
    }

    fn stop(mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() { let _ = h.join(); }
    }
}

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "awsx2",
    about = "AWS management CLI/TUI",
    version = concat!(env!("CARGO_PKG_VERSION"), "-", env!("BUILD_DATE"))
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// List all EC2 instances with state and SSM status
    List,
    /// Start an EC2 instance (uses INSTANCE_NAME env or --name)
    Start {
        #[arg(long, env = "INSTANCE_NAME")]
        name: String,
    },
    /// Stop an EC2 instance gracefully
    Stop {
        #[arg(long, env = "INSTANCE_NAME")]
        name: String,
    },
    /// Force-stop an EC2 instance (like pulling the power cord)
    ForceStop {
        #[arg(long, env = "INSTANCE_NAME")]
        name: String,
    },
    /// Switch instance type to gpu (g4dn.4xlarge) or cpu (m6i.2xlarge)
    Switch {
        /// Target type: "gpu" or "cpu"
        target: String,
        #[arg(long, env = "INSTANCE_NAME")]
        name: String,
    },
    /// Show instance status
    Status {
        #[arg(long, env = "INSTANCE_NAME")]
        name: String,
    },
    /// Run aws sso login
    Login {
        /// AWS profile (defaults to $AWS_PROFILE)
        profile: Option<String>,
    },
    /// Resolve a URL/hostname to its EC2/ALB/Fargate resource
    Resolve {
        url: String,
    },
    /// Open an SSM port-forwarding tunnel to an EC2 instance by name pattern
    Tunnel {
        /// Substring to match against EC2 Name tags
        pattern: String,
        /// Local port to listen on
        local_port: u16,
        /// Remote port on the instance (default: 8000)
        #[arg(default_value = "8000")]
        remote_port: u16,
        /// Bind address (default: 0.0.0.0 for Docker/external access)
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// Tunnel to any internal URL (smart ALB resolution + bastion fallback)
    TunnelUrl {
        url: String,
        local_port: u16,
        /// Remote port (auto-detected from ALB target group if omitted)
        remote_port: Option<u16>,
        /// Set up nginx reverse proxy so the URL works directly in the browser
        #[arg(long)]
        proxy: bool,
        /// Bind address (default: 0.0.0.0 for Docker/external access)
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// Tunnel to EC2 or Fargate by resolving a URL's DNS
    TunnelDns {
        url: String,
        local_port: u16,
        #[arg(default_value = "8501")]
        remote_port: u16,
    },
    /// Tunnel to a remote host via a specific bastion
    TunnelRemote {
        /// Bastion name pattern
        bastion: String,
        /// Private IP or hostname of the target
        host: String,
        local_port: u16,
        #[arg(default_value = "8501")]
        remote_port: u16,
    },
    /// Kill all running SSM tunnel processes
    TunnelStop,
    /// Test if a local tunnel port is open
    TunnelTest {
        local_port: u16,
    },
    /// List ECR images (like `docker images`). Omit repository to scan all repos.
    EcrImages {
        /// ECR repository name (omit to list all repositories)
        repository: Option<String>,
        /// AWS region (overrides profile/env default)
        #[arg(long, short = 'r')]
        region: Option<String>,
        /// Show only the newest image per tag prefix (filter out older builds)
        #[arg(long)]
        latest: bool,
    },
    /// Act as SSH ProxyCommand: resolve EC2 Name tag to instance ID and exec SSM session
    SsmProxy {
        /// EC2 Name tag to resolve
        #[arg(long)]
        name: String,
        /// SSH port (passed by SSH as %p)
        #[arg(long, default_value = "22")]
        port: String,
        /// AWS region (defaults to configured region)
        #[arg(long)]
        region: Option<String>,
    },
    /// Generate ~/.ssh/config entries for all running EC2 instances (SSM-online)
    SshConfig {
        /// Only print, don't write to ~/.ssh/config
        #[arg(long)]
        dry_run: bool,
        /// SSH user (default: ec2-user)
        #[arg(long, default_value = "ec2-user")]
        user: String,
    },
    /// Print the raw AWS CLI commands used behind the scenes
    Cheatcodes,
    /// AWS Client VPN management (SAML authentication)
    Vpn {
        #[command(subcommand)]
        action: VpnAction,
    },
}

#[derive(Subcommand)]
enum VpnAction {
    /// Connect to VPN (prompts for MFA code)
    Connect {
        /// MFA/TOTP code from your authenticator app
        mfa: Option<String>,
    },
    /// Disconnect active VPN
    Disconnect,
    /// Show VPN connection status
    Status,
    /// Configure VPN credentials and .ovpn file path
    Setup(VpnSetupArgs),
}

#[derive(Args)]
struct VpnSetupArgs {
    /// SSO username/email
    #[arg(long)]
    username: Option<String>,
    /// SSO password
    #[arg(long)]
    password: Option<String>,
    /// Path to .ovpn config file
    #[arg(long)]
    ovpn: Option<String>,
    /// DNS server IP for VPN (e.g. 10.0.0.2)
    #[arg(long)]
    dns_server: Option<String>,
    /// DNS routing domain for VPN (e.g. ~internal.example.com)
    #[arg(long)]
    dns_domain: Option<String>,
}

const GPU_TYPE: &str = "g4dn.4xlarge";
const CPU_TYPE: &str = "m6i.2xlarge";

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => {
            if let Err(e) = run_tui() {
                eprintln!("TUI error: {}", e);
                std::process::exit(1);
            }
        }
        Some(cmd) => {
            if let Err(e) = run_cli(cmd) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

// ── Non-interactive CLI ───────────────────────────────────────────────────────

fn run_cli(cmd: Cmd) -> error::Result<()> {
    match cmd {
        Cmd::List => {
            let instances = aws::list_instances(None)?;
            println!(
                "{:<22} {:<30} {:<14} {:<12} {:<10} {:<18}",
                "INSTANCE ID", "NAME", "TYPE", "STATE", "SSM", "PRIVATE IP"
            );
            println!("{}", "-".repeat(110));
            for i in &instances {
                println!(
                    "{:<22} {:<30} {:<14} {:<12} {:<10} {:<18}",
                    i.id, i.name, i.instance_type,
                    i.state.as_str(), i.ssm_status.as_str(),
                    i.private_ip.as_deref().unwrap_or("-"),
                );
            }
        }

        Cmd::Start { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("{}", gray(format!("Starting {} ({})...", inst.name, inst.id)));
            aws::start_instance(&inst.id, None)?;
            println!("Start command sent.");
        }

        Cmd::Stop { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("{}", gray(format!("Stopping {} ({})...", inst.name, inst.id)));
            aws::stop_instance(&inst.id, false, None)?;
            println!("Stop command sent.");
        }

        Cmd::ForceStop { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("{}", gray(format!("Force-stopping {} ({})...", inst.name, inst.id)));
            aws::stop_instance(&inst.id, true, None)?;
            println!("Force-stop command sent.");
        }

        Cmd::Switch { target, name } => {
            let new_type = match target.to_lowercase().as_str() {
                "gpu" => GPU_TYPE,
                "cpu" => CPU_TYPE,
                other => {
                    eprintln!("Unknown target '{}'. Use 'gpu' or 'cpu'.", other);
                    std::process::exit(1);
                }
            };
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("{}", gray(format!("Switching {} ({}) to {}...", inst.name, inst.id, new_type)));
            if inst.state == models::InstanceState::Running {
                println!("{}", gray("Stopping instance first..."));
                aws::stop_instance(&inst.id, false, None)?;
            }
            aws::modify_instance_type(&inst.id, new_type, None)?;
            println!("Instance type changed to {}.", new_type);
        }

        Cmd::Status { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("-------------------------------------");
            println!("  Name:       {}", inst.name);
            println!("  ID:         {}", inst.id);
            println!("  Type:       {}", inst.instance_type);
            println!("  State:      {}", inst.state.as_str());
            println!("  SSM:        {}", inst.ssm_status.as_str());
            println!("  Private IP: {}", inst.private_ip.as_deref().unwrap_or("N/A"));
            println!("  Public IP:  {}", inst.public_ip.as_deref().unwrap_or("N/A"));
            println!("-------------------------------------");
        }

        Cmd::Login { profile } => {
            let profile_str = profile
                .or_else(|| std::env::var("AWS_PROFILE").ok())
                .unwrap_or_default();
            let profile_opt = if profile_str.is_empty() { None } else { Some(profile_str.as_str()) };
            println!("{}", gray(format!("Running: aws sso login{}",
                profile_opt.map(|p| format!(" --profile {}", p)).unwrap_or_default())));
            aws::sso_login(profile_opt)?;
            println!("{}", gray("\nVerifying identity..."));
            println!("{}", aws::get_caller_identity(profile_opt)?);
        }

        Cmd::Resolve { url } => {
            println!("{}", aws::resolve_dns_report(&url, None)?);
        }

        Cmd::Tunnel { pattern, local_port, remote_port, bind } => {
            if tunnel::test_port(local_port) {
                if !confirm_and_kill_port(local_port) {
                    return Ok(());
                }
            }

            let needs_forwarder = bind != "127.0.0.1";
            let ssm_port = if needs_forwarder {
                tunnel::find_available_port(local_port + 10000)
            } else {
                local_port
            };

            println!("{}", gray(format!("Starting tunnel: *{}*:{} -> {}:{}", pattern, remote_port, bind, local_port)));
            let tp = tunnel::start_tunnel_by_pattern(&pattern, ssm_port, remote_port, None)?;

            if needs_forwarder {
                let fwd_pid = tunnel::start_bind_forwarder(&bind, local_port, ssm_port)?;
                println!("Tunnel active: {}:{} -> {}:{} (forwarder pid {})",
                    bind, local_port, tp.instance_name, tp.remote_port, fwd_pid);
            } else {
                println!("Tunnel active: localhost:{} -> {}:{}", tp.local_port, tp.instance_name, tp.remote_port);
            }
        }

        Cmd::TunnelUrl { url, local_port, remote_port, proxy, bind } => {
            if tunnel::test_port(local_port) {
                if !confirm_and_kill_port(local_port) {
                    return Ok(());
                }
            }
            let host = aws::strip_url_to_host(&url);
            println!("{}", gray(format!("Resolving {}...", host)));

            let needs_forwarder = bind != "127.0.0.1";
            let ssm_port = if needs_forwarder {
                tunnel::find_available_port(local_port + 10000)
            } else {
                local_port
            };

            // Smart path: URL → ALB → target group → healthy backend → SG → hop instance
            let tunneled = match try_alb_tunnel(&host, ssm_port, remote_port) {
                Ok(Some(tp)) => {
                    if needs_forwarder {
                        let fwd_pid = tunnel::start_bind_forwarder(&bind, local_port, ssm_port)?;
                        println!(
                            "Tunnel active: {}:{} -> {}:{} via {} (forwarder pid {})",
                            bind, local_port,
                            tp.remote_host.as_deref().unwrap_or("?"),
                            tp.remote_port,
                            tp.instance_name,
                            fwd_pid,
                        );
                    } else {
                        println!(
                            "Tunnel active: localhost:{} -> {}:{} via {}",
                            tp.local_port,
                            tp.remote_host.as_deref().unwrap_or("?"),
                            tp.remote_port,
                            tp.instance_name,
                        );
                    }
                    true
                }
                _ => {
                    // Fallback: try all SSM-online bastions directly
                    println!("{}", gray("  Trying bastions..."));
                    let tp = tunnel::start_url_tunnel_via_any_bastion(&url, ssm_port, remote_port, None)?;
                    if needs_forwarder {
                        let fwd_pid = tunnel::start_bind_forwarder(&bind, local_port, ssm_port)?;
                        println!(
                            "Tunnel active: {}:{} -> {} via {} (forwarder pid {})",
                            bind, local_port, tp.remote_host.as_deref().unwrap_or("?"), tp.instance_name, fwd_pid
                        );
                    } else {
                        println!(
                            "Tunnel active: localhost:{} -> {} via {}",
                            tp.local_port, tp.remote_host.as_deref().unwrap_or("?"), tp.instance_name
                        );
                    }
                    true
                }
            };

            if tunneled && proxy {
                println!("{}", gray("Setting up reverse proxy..."));
                proxy::setup_proxy(&host, local_port)?;
                println!("Access: http://{}", host);
            }
        }

        Cmd::TunnelDns { url, local_port, remote_port } => {
            if tunnel::test_port(local_port) {
                if !confirm_and_kill_port(local_port) {
                    return Ok(());
                }
            }
            println!("{}", gray(format!("Resolving {} for tunnel...", url)));
            let tp = tunnel::start_dns_tunnel(&url, local_port, remote_port, None)?;
            println!("Tunnel active: localhost:{} -> {}:{}", tp.local_port, tp.instance_name, tp.remote_port);
        }

        Cmd::TunnelRemote { bastion, host, local_port, remote_port } => {
            if tunnel::test_port(local_port) {
                if !confirm_and_kill_port(local_port) {
                    return Ok(());
                }
            }
            println!("{}", gray(format!("Starting remote tunnel via *{}* -> {}:{}", bastion, host, remote_port)));
            let tp = tunnel::start_remote_tunnel_via_pattern(&bastion, &host, local_port, remote_port, None)?;
            println!("Tunnel active: localhost:{} -> {}:{} via {}", tp.local_port, host, remote_port, tp.instance_name);
        }

        Cmd::TunnelStop => {
            tunnel::stop_all_tunnels();
            if proxy::has_active_proxies() {
                println!("{}", gray("Cleaning up reverse proxies..."));
                proxy::teardown_all_proxies();
            }
            println!("All SSM tunnels stopped.");
        }

        Cmd::TunnelTest { local_port } => {
            if tunnel::test_port(local_port) {
                println!("Port {} is OPEN (tunnel active).", local_port);
            } else {
                eprintln!("Port {} is CLOSED.", local_port);
                std::process::exit(1);
            }
        }

        Cmd::EcrImages { repository, region, latest } => {
            // If a full ECR URI was given, extract repo name + region from it
            let (repository, region) = match repository {
                Some(r) => {
                    let (repo, uri_region) = aws::parse_ecr_uri(&r);
                    let effective_region = uri_region.or(region);
                    (Some(repo), effective_region)
                }
                None => (None, region),
            };
            let spinner = Spinner::new("Fetching repositories...");
            let repos = match repository {
                Some(r) => vec![r],
                None => aws::list_ecr_repositories(region.as_deref(), None)?,
            };
            let total = repos.len();
            let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let handles: Vec<_> = repos.iter().map(|r| {
                let r = r.clone();
                let region = region.clone();
                let done = done.clone();
                std::thread::spawn(move || {
                    let result = aws::list_ecr_images(&r, region.as_deref(), None).unwrap_or_default();
                    done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    result
                })
            }).collect();
            while done.load(std::sync::atomic::Ordering::Relaxed) < total {
                spinner.set(&format!(
                    "Fetching images ({}/{})...",
                    done.load(std::sync::atomic::Ordering::Relaxed), total
                ));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let mut images: Vec<aws::EcrImage> = handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect();
            spinner.stop();
            // Sort: repository alphabetically, then newest-first, then tag alphabetically
            images.sort_by(|a, b| {
                a.repository.cmp(&b.repository)
                    .then(b.pushed_at.partial_cmp(&a.pushed_at).unwrap_or(std::cmp::Ordering::Equal))
                    .then(a.tag.cmp(&b.tag))
            });
            let images = if latest { aws::filter_latest_images(images) } else { images };
            if images.is_empty() {
                println!("No images found.");
            } else {
                let repo_w = images.iter().map(|i| i.repository.len()).max().unwrap_or(10).max(10) + 2;
                let tag_w  = images.iter().map(|i| i.tag.len()).max().unwrap_or(3).max(3) + 2;
                println!("{:<repo_w$} {:<tag_w$} {:<14} {:<20} {}", "REPOSITORY", "TAG", "IMAGE ID", "CREATED", "SIZE");
                println!("{}", "-".repeat(repo_w + tag_w + 14 + 20 + 10));
                for img in &images {
                    println!(
                        "{:<repo_w$} {:<tag_w$} {:<14} {:<20} {}",
                        img.repository, img.tag, img.image_id,
                        img.relative_pushed_at(), img.human_size()
                    );
                }
            }
        }

        Cmd::SsmProxy { name, port, region } => {
            run_ssm_proxy(&name, &port, region.as_deref())?;
        }

        Cmd::SshConfig { dry_run, user } => {
            run_ssh_config(dry_run, &user)?;
        }

        Cmd::Cheatcodes => {
            print_cheatcodes();
        }

        Cmd::Vpn { action } => {
            match action {
                VpnAction::Setup(args) => {
                    let mut config = vpn::load_config()?;
                    if let Some(u) = args.username { config.sso_username = u; }
                    if let Some(p) = args.password { config.sso_password = p; }
                    if let Some(o) = args.ovpn { config.ovpn_path = o; }
                    if let Some(d) = args.dns_server { config.dns_server = d; }
                    if let Some(d) = args.dns_domain { config.dns_domain = d; }
                    // Interactive prompts for missing fields
                    if config.sso_username.is_empty() {
                        eprint!("SSO Username/Email: ");
                        let mut s = String::new();
                        std::io::stdin().read_line(&mut s)?;
                        config.sso_username = s.trim().to_string();
                    }
                    if config.sso_password.is_empty() {
                        eprint!("SSO Password: ");
                        let mut s = String::new();
                        std::io::stdin().read_line(&mut s)?;
                        config.sso_password = s.trim().to_string();
                    }
                    if config.ovpn_path.is_empty() {
                        eprint!("Path to .ovpn file: ");
                        let mut s = String::new();
                        std::io::stdin().read_line(&mut s)?;
                        config.ovpn_path = s.trim().to_string();
                    }
                    vpn::save_config(&config)?;
                    let path = dirs::config_dir()
                        .unwrap_or_default()
                        .join("awsx2")
                        .join("vpn.json");
                    println!("VPN config saved to {}", path.display());
                    println!("  Username: {}", config.sso_username);
                    println!("  OVPN:     {}", config.ovpn_path);
                    println!("  DNS:      {} ({})", config.dns_server, config.dns_domain);
                }
                VpnAction::Connect { mfa } => {
                    let config = vpn::load_config()?;
                    let mfa_code = match mfa {
                        Some(code) => code,
                        None => {
                            eprint!("MFA Code: ");
                            let mut s = String::new();
                            std::io::stdin().read_line(&mut s)?;
                            s.trim().to_string()
                        }
                    };
                    if mfa_code.is_empty() {
                        eprintln!("MFA code is required.");
                        std::process::exit(1);
                    }
                    let pid = vpn::connect(&config, &mfa_code, |msg| println!("{}", msg))?;
                    let ip = vpn::get_vpn_ip().unwrap_or_else(|| "?".into());
                    println!("\nVPN connected and running in background.");
                    println!("  IP:  {}", ip);
                    println!("  PID: {}", pid);
                    println!("\nUse 'awsx2 vpn disconnect' to stop.");
                }
                VpnAction::Disconnect => {
                    vpn::disconnect();
                    println!("VPN disconnected.");
                }
                VpnAction::Status => {
                    if vpn::is_connected() {
                        let ip = vpn::get_vpn_ip().unwrap_or_else(|| "unknown".into());
                        let pid = vpn::find_vpn_pid().map(|p| p.to_string()).unwrap_or_else(|| "?".into());
                        println!("VPN: CONNECTED");
                        println!("  IP:  {}", ip);
                        println!("  PID: {}", pid);
                    } else {
                        println!("VPN: DISCONNECTED");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Try ALB-aware tunnel resolution.
/// Returns Ok(None) if no ALB path is found (caller should fall back to bastions).
/// Returns Ok(Some(tp)) on success.
/// Returns Err if the path was found but the tunnel itself failed.
fn try_alb_tunnel(
    host: &str,
    local_port: u16,
    remote_port: Option<u16>,
) -> error::Result<Option<models::TunnelProcess>> {
    let alb_arn = match aws::find_alb_for_hostname(host, None).unwrap_or(None) {
        Some(arn) => arn,
        None => return Ok(None),
    };
    let targets = aws::get_alb_healthy_targets(&alb_arn, remote_port, None).unwrap_or_default();
    if targets.is_empty() { return Ok(None); }

    // Try each healthy target — pick the first one for which we can find a valid hop.
    for (target_ip, target_port) in &targets {
        let target_sgs = match aws::get_target_sg_ids(target_ip, None) {
            Ok(sgs) if !sgs.is_empty() => sgs,
            _ => continue,
        };
        let allowed_sgs = match aws::get_allowed_source_sgs(&target_sgs, *target_port, None) {
            Ok(sgs) if !sgs.is_empty() => sgs,
            _ => continue,
        };
        let hop = match aws::find_ssm_hop_by_sgs(&allowed_sgs, None).unwrap_or(None) {
            Some(inst) => inst,
            None => continue,
        };
        println!("{}", gray(format!("  ALB target: {}:{}", target_ip, target_port)));
        println!("{}", gray(format!("  Via: {}", hop.name)));

        let tp = tunnel::start_remote_tunnel_via_instance(
            &hop.id, &hop.name, target_ip, local_port, *target_port, None,
        )?;
        return Ok(Some(tp));
    }
    Ok(None)
}

// ── SSM Proxy (SSH ProxyCommand) ──────────────────────────────────────────

fn run_ssm_proxy(name: &str, port: &str, region: Option<&str>) -> error::Result<()> {
    let region = region
        .map(|s| s.to_string())
        .unwrap_or_else(|| aws::get_region(None));

    // Resolve Name tag → instance ID
    let inst = aws::find_instance_by_name(name, None)?;

    // Ensure SSH public key is on the instance (cached, only runs once per instance)
    ensure_ssh_key_pushed(&inst.id, &region);

    // exec aws ssm start-session (replaces current process)
    let err = exec::execvp(
        "aws",
        &[
            "aws", "ssm", "start-session",
            "--target", &inst.id,
            "--document-name", "AWS-StartSSHSession",
            "--parameters", &format!("portNumber={}", port),
            "--region", &region,
        ],
    );
    Err(error::AppError::AwsCli(format!("exec failed: {}", err)))
}

/// Push the user's SSH public key to the instance via SSM send-command,
/// but only once per instance (cached in ~/.cache/awsx2/ssh-keys/).
fn ensure_ssh_key_pushed(instance_id: &str, region: &str) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("awsx2")
        .join("ssh-keys");
    let marker = cache_dir.join(instance_id);

    // If marker exists and is less than 7 days old, skip
    if let Ok(meta) = std::fs::metadata(&marker) {
        if let Some(age) = meta.modified().ok().and_then(|m| m.elapsed().ok()) {
            if age < std::time::Duration::from_secs(7 * 86_400) {
                return;
            }
        }
    }

    // Find the user's public key
    let home = std::env::var("HOME").unwrap_or_default();
    let pub_key = ["id_ed25519.pub", "id_rsa.pub", "id_ecdsa.pub"]
        .iter()
        .map(|f| std::path::PathBuf::from(&home).join(".ssh").join(f))
        .find(|p| p.exists());
    let pub_key_path = match pub_key {
        Some(p) => p,
        None => return, // no public key found, nothing to push
    };
    let pub_key_content = match std::fs::read_to_string(&pub_key_path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };

    // Push key idempotently: grep to avoid duplicates
    let script = format!(
        "grep -qF '{}' /home/ec2-user/.ssh/authorized_keys 2>/dev/null || echo '{}' >> /home/ec2-user/.ssh/authorized_keys",
        pub_key_content, pub_key_content
    );

    let output = std::process::Command::new("aws")
        .args([
            "ssm", "send-command",
            "--instance-ids", instance_id,
            "--document-name", "AWS-RunShellScript",
            "--parameters", &format!("commands=[\"{}\"]", script.replace('"', "\\\"")),
            "--region", region,
            "--output", "json",
        ])
        .output();

    let command_id = match output {
        Ok(o) if o.status.success() => {
            let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_default();
            json["Command"]["CommandId"].as_str().unwrap_or("").to_string()
        }
        _ => return,
    };

    if command_id.is_empty() { return; }

    // Wait for completion (up to 10s)
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let poll = std::process::Command::new("aws")
            .args([
                "ssm", "get-command-invocation",
                "--command-id", &command_id,
                "--instance-id", instance_id,
                "--region", region,
                "--output", "json",
            ])
            .output();
        if let Ok(o) = poll {
            if o.status.success() {
                let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_default();
                let status = json["Status"].as_str().unwrap_or("");
                if status == "Success" {
                    let _ = std::fs::create_dir_all(&cache_dir);
                    let _ = std::fs::write(&marker, "");
                    return;
                }
                if status == "Failed" || status == "Cancelled" {
                    return;
                }
            }
        }
    }
}

// ── SSH Config generation ────────────────────────────────────────────────

const SSH_CONFIG_BEGIN: &str = "# BEGIN awsx2-managed";
const SSH_CONFIG_END: &str = "# END awsx2-managed";

fn run_ssh_config(dry_run: bool, user: &str) -> error::Result<()> {
    let instances = aws::list_instances(None)?;
    let running_ssm: Vec<_> = instances
        .into_iter()
        .filter(|i| {
            i.state == models::InstanceState::Running
                && i.ssm_status == models::SsmStatus::Online
                && !i.name.is_empty()
        })
        .collect();

    if running_ssm.is_empty() {
        println!("No running SSM-online instances found.");
        return Ok(());
    }

    // Find awsx2 binary path for ProxyCommand
    let awsx2_bin = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "awsx2".to_string());

    let region = aws::get_region(None);

    // Collect instance names we'll manage
    let managed_names: std::collections::HashSet<String> = running_ssm
        .iter()
        .map(|i| i.name.clone())
        .collect();

    let mut block = String::new();
    block.push_str(&format!("{}\n", SSH_CONFIG_BEGIN));
    for inst in &running_ssm {
        block.push_str(&format!(
            "\nHost {name}\n    User {user}\n    ProxyCommand {bin} ssm-proxy --name {name} --port %p --region {region}\n",
            name = inst.name,
            user = user,
            bin = awsx2_bin,
            region = region,
        ));
    }
    block.push_str(&format!("{}\n", SSH_CONFIG_END));

    if dry_run {
        println!("{}", block);
        println!("# {} instances (dry-run, not written)", running_ssm.len());
        return Ok(());
    }

    // Read existing config
    let home = std::env::var("HOME").unwrap_or_default();
    let ssh_dir = std::path::PathBuf::from(&home).join(".ssh");
    let config_path = ssh_dir.join("config");
    let _ = std::fs::create_dir_all(&ssh_dir);
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();

    // Step 1: Remove the old awsx2-managed block if present
    let cleaned = if let (Some(begin), Some(end)) = (
        existing.find(SSH_CONFIG_BEGIN),
        existing.find(SSH_CONFIG_END),
    ) {
        let end_line = existing[end..].find('\n').map(|i| end + i + 1).unwrap_or(existing.len());
        format!("{}{}", &existing[..begin], &existing[end_line..])
    } else {
        existing.clone()
    };

    // Step 2: Remove stale Host blocks outside the managed section that
    // duplicate instances we're about to add (matching by Host name).
    let cleaned = remove_stale_host_blocks(&cleaned, &managed_names);

    // Step 3: Append the new managed block
    let new_config = if cleaned.is_empty() || cleaned.ends_with('\n') {
        format!("{}{}", cleaned, block)
    } else {
        format!("{}\n\n{}", cleaned, block)
    };

    std::fs::write(&config_path, &new_config)?;
    println!("Updated {} with {} instances.", config_path.display(), running_ssm.len());
    for inst in &running_ssm {
        println!("  ssh {}", inst.name);
    }
    Ok(())
}

/// Remove SSH config Host blocks whose Host name matches any of the given names.
/// Parses the config line-by-line: a Host block starts at `Host <name>` and ends
/// at the next `Host ` line or EOF.
fn remove_stale_host_blocks(config: &str, names: &std::collections::HashSet<String>) -> String {
    let lines: Vec<&str> = config.lines().collect();
    let mut result = Vec::new();
    let mut skip = false;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("Host ") && !trimmed.starts_with("Host *") {
            let host_name = trimmed.strip_prefix("Host ").unwrap().trim();
            if names.contains(host_name) {
                skip = true;
                continue;
            } else {
                skip = false;
            }
        } else if skip {
            // Inside a block we're removing: skip indented lines or blank lines
            if trimmed.is_empty() || trimmed.starts_with('#') || line.starts_with(' ') || line.starts_with('\t') {
                continue;
            } else {
                // Non-indented, non-empty line that isn't a Host — stop skipping
                skip = false;
            }
        }
        if !skip {
            result.push(*line);
        }
    }

    // Trim trailing blank lines that may be left behind
    while result.last().map_or(false, |l| l.trim().is_empty()) {
        result.pop();
    }
    if result.is_empty() {
        String::new()
    } else {
        format!("{}\n", result.join("\n"))
    }
}

// ── Cheatcodes ────────────────────────────────────────────────────────────────

fn print_cheatcodes() {
    let sections: &[(&str, &[(&str, &str)])] = &[
        ("EC2 Instances", &[
            ("List all instances + state",
             "aws ec2 describe-instances --query 'Reservations[*].Instances[*]' --output json"),
            ("Start an instance",
             "aws ec2 start-instances --instance-ids <id>"),
            ("Stop an instance",
             "aws ec2 stop-instances --instance-ids <id>"),
            ("Force-stop an instance",
             "aws ec2 stop-instances --instance-ids <id> --force"),
            ("Change instance type",
             "aws ec2 modify-instance-attribute --instance-id <id> --instance-type <type>"),
        ]),
        ("SSM / Tunnels", &[
            ("List SSM-online instances",
             "aws ssm describe-instance-information --output json"),
            ("Direct port-forward tunnel",
             "aws ssm start-session --target <id> --document-name AWS-StartPortForwardingSession \\\n  --parameters '{\"portNumber\":[\"8000\"],\"localPortNumber\":[\"8080\"]}'"),
            ("Tunnel via bastion to remote host",
             "aws ssm start-session --target <bastion-id> --document-name AWS-StartPortForwardingSessionToRemoteHost \\\n  --parameters '{\"host\":[\"10.0.1.42\"],\"portNumber\":[\"8000\"],\"localPortNumber\":[\"8080\"]}'"),
            ("Run a shell command on an instance",
             "aws ssm send-command --instance-ids <id> --document-name AWS-RunShellScript \\\n  --parameters 'commands=[\"dig +short <host>\"]'"),
            ("Poll command result",
             "aws ssm get-command-invocation --command-id <cmd-id> --instance-id <id>"),
        ]),
        ("ALB / Load Balancers", &[
            ("List all ALBs",
             "aws elbv2 describe-load-balancers --output json"),
            ("List target groups for an ALB",
             "aws elbv2 describe-target-groups --load-balancer-arn <arn>"),
            ("Get healthy targets",
             "aws elbv2 describe-target-health --target-group-arn <arn>"),
        ]),
        ("Security Groups / ENIs", &[
            ("Find ENI by private IP",
             "aws ec2 describe-network-interfaces \\\n  --filters Name=addresses.private-ip-address,Values=<ip>"),
            ("Inspect security group rules",
             "aws ec2 describe-security-groups --group-ids <sg-id>"),
        ]),
        ("ECR", &[
            ("List images in a repository",
             "aws ecr describe-images --repository-name <repo> [--region <region>] --output json"),
            ("List all repositories",
             "aws ecr describe-repositories --output json"),
        ]),
        ("Auth / Identity", &[
            ("SSO login",
             "aws sso login [--profile <profile>]"),
            ("Check current identity",
             "aws sts get-caller-identity"),
            ("Get configured region",
             "aws configure get region"),
        ]),
    ];

    println!("awsx2 cheatcodes — raw AWS CLI commands used under the hood\n");
    for (section, entries) in sections.iter() {
        println!("── {} {}", section, "─".repeat(60usize.saturating_sub(section.len() + 4)));
        for (label, cmd) in entries.iter() {
            println!("  # {}", label);
            for line in cmd.lines() {
                println!("  {}", line);
            }
            println!();
        }
    }
}

// ── TUI ───────────────────────────────────────────────────────────────────────

fn run_tui() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.refresh_instances();
    app.refresh_tunnels();

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| tui::ui::render(f, &app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_global_key(&mut app, key);
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick_spinner();
            app.poll_bg();
            last_tick = Instant::now();
        }

        if app.quit { break; }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_global_key(app: &mut App, key: KeyEvent) {
    // Handle open popup first
    match app.popup.clone() {
        Popup::None => {}

        Popup::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                app.popup = Popup::None;
            }
            return;
        }

        Popup::Input { tag, .. } => {
            match key.code {
                KeyCode::Esc => { app.popup = Popup::None; }
                KeyCode::Enter => {
                    let val = if let Popup::Input { ref value, .. } = app.popup {
                        value.clone()
                    } else { String::new() };
                    app.popup = Popup::None;
                    dispatch_input(app, tag, val);
                }
                KeyCode::Backspace => {
                    if let Popup::Input { ref mut value, .. } = app.popup { value.pop(); }
                }
                KeyCode::Char(c) => {
                    if let Popup::Input { ref mut value, .. } = app.popup { value.push(c); }
                }
                _ => {}
            }
            return;
        }

        Popup::Confirm { tag, selected_yes, .. } => {
            match key.code {
                KeyCode::Esc => { app.popup = Popup::None; }
                KeyCode::Left | KeyCode::Right | KeyCode::Tab
                | KeyCode::Char('h') | KeyCode::Char('l') => {
                    if let Popup::Confirm { ref mut selected_yes, .. } = app.popup {
                        *selected_yes = !*selected_yes;
                    }
                }
                KeyCode::Enter => {
                    app.popup = Popup::None;
                    dispatch_confirm(app, tag, selected_yes);
                }
                _ => {}
            }
            return;
        }

        Popup::Select { tag, .. } => {
            match key.code {
                KeyCode::Esc => { app.popup = Popup::None; }
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Popup::Select { ref mut selected, .. } = app.popup {
                        if *selected > 0 { *selected -= 1; }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Popup::Select { ref mut selected, ref items, .. } = app.popup {
                        if *selected + 1 < items.len() { *selected += 1; }
                    }
                }
                KeyCode::Enter => {
                    let val = if let Popup::Select { ref items, selected, .. } = app.popup {
                        items.get(selected).cloned().unwrap_or_default()
                    } else { String::new() };
                    app.popup = Popup::None;
                    dispatch_input(app, tag, val);
                }
                _ => {}
            }
            return;
        }

        Popup::Result { .. } | Popup::Loading { .. } => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                app.popup = Popup::None;
            }
            return;
        }
    }

    // Global keys (no popup open)
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => {
            app.quit = true;
        }
        (KeyCode::Char('?'), _) => {
            app.popup = Popup::Help;
        }
        (KeyCode::Tab, KeyModifiers::NONE) => {
            app.tab = app.tab.next();
        }
        (KeyCode::BackTab, _) => {
            app.tab = app.tab.prev();
        }
        _ => match app.tab {
            Tab::Instances => pages::instances::handle_key(app, key),
            Tab::Tunnels   => pages::tunnels::handle_key(app, key),
            Tab::Tools     => pages::tools::handle_key(app, key),
            Tab::Vpn       => pages::vpn::handle_key(app, key),
        },
    }
}

fn dispatch_input(app: &mut App, tag: InputTag, value: String) {
    match tag {
        InputTag::LoginProfile | InputTag::ResolveUrl | InputTag::TestPort
        | InputTag::SwitchProfile | InputTag::SwitchRegion => {
            pages::tools::handle_input(app, tag, value);
        }
        InputTag::NewTunnelPattern
        | InputTag::NewTunnelLocalPort
        | InputTag::NewTunnelRemotePort
        | InputTag::NewTunnelUrl
        | InputTag::NewTunnelUrlLocalPort
        | InputTag::NewTunnelUrlRemotePort
        | InputTag::NewTunnelBastionPattern
        | InputTag::NewTunnelBastionHost
        | InputTag::NewTunnelBastionLocalPort
        | InputTag::NewTunnelBastionRemotePort => {
            pages::tunnels::handle_input(app, tag, value);
        }
        InputTag::VpnMfaCode
        | InputTag::VpnSetupUsername
        | InputTag::VpnSetupPassword
        | InputTag::VpnSetupOvpnPath => {
            pages::vpn::handle_input(app, tag, value);
        }
    }
}

fn dispatch_confirm(app: &mut App, tag: ConfirmTag, confirmed: bool) {
    match tag {
        ConfirmTag::StopTunnel(_) | ConfirmTag::StopAllTunnels => {
            pages::tunnels::handle_confirm(app, tag, confirmed);
        }
        ConfirmTag::StopInstance | ConfirmTag::ForceStopInstance => {
            pages::instances::handle_confirm(app, tag, confirmed);
        }
    }
}
