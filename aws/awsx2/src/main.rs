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

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "awsx2", about = "AWS management CLI/TUI", version)]
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
            println!("Starting {} ({})...", inst.name, inst.id);
            aws::start_instance(&inst.id, None)?;
            println!("Start command sent.");
        }

        Cmd::Stop { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("Stopping {} ({})...", inst.name, inst.id);
            aws::stop_instance(&inst.id, false, None)?;
            println!("Stop command sent.");
        }

        Cmd::ForceStop { name } => {
            let inst = aws::find_instance_by_name(&name, None)?;
            println!("Force-stopping {} ({})...", inst.name, inst.id);
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
            println!("Switching {} ({}) to {}...", inst.name, inst.id, new_type);
            if inst.state == models::InstanceState::Running {
                println!("Stopping instance first...");
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
            println!("Running: aws sso login{}",
                profile_opt.map(|p| format!(" --profile {}", p)).unwrap_or_default());
            aws::sso_login(profile_opt)?;
            println!("\nVerifying identity...");
            println!("{}", aws::get_caller_identity(profile_opt)?);
        }

        Cmd::Resolve { url } => {
            println!("{}", aws::resolve_dns_report(&url, None)?);
        }

        Cmd::Tunnel { pattern, local_port, remote_port } => {
            if tunnel::test_port(local_port) {
                println!("Port {} already in use (tunnel may be active).", local_port);
                return Ok(());
            }
            println!("Starting tunnel: *{}*:{} -> localhost:{}", pattern, remote_port, local_port);
            let tp = tunnel::start_tunnel_by_pattern(&pattern, local_port, remote_port, None)?;
            println!("Tunnel active: localhost:{} -> {}:{}", tp.local_port, tp.instance_name, tp.remote_port);
        }

        Cmd::TunnelUrl { url, local_port, remote_port, proxy } => {
            if tunnel::test_port(local_port) {
                println!("Port {} already in use.", local_port);
                return Ok(());
            }
            let host = aws::strip_url_to_host(&url);
            println!("Resolving {}...", host);

            // Smart path: URL → ALB → target group → healthy backend → SG → hop instance
            let tunneled = match try_alb_tunnel(&host, local_port, remote_port) {
                Ok(Some(tp)) => {
                    println!(
                        "Tunnel active: localhost:{} -> {}:{} via {}",
                        tp.local_port,
                        tp.remote_host.as_deref().unwrap_or("?"),
                        tp.remote_port,
                        tp.instance_name,
                    );
                    true
                }
                _ => {
                    // Fallback: try all SSM-online bastions directly
                    println!("  Trying bastions...");
                    let tp = tunnel::start_url_tunnel_via_any_bastion(&url, local_port, None)?;
                    println!(
                        "Tunnel active: localhost:{} -> {} via {}",
                        tp.local_port, tp.remote_host.as_deref().unwrap_or("?"), tp.instance_name
                    );
                    true
                }
            };

            if tunneled && proxy {
                println!("Setting up reverse proxy...");
                proxy::setup_proxy(&host, local_port)?;
                println!("Access: http://{}", host);
            }
        }

        Cmd::TunnelDns { url, local_port, remote_port } => {
            if tunnel::test_port(local_port) {
                println!("Port {} already in use.", local_port);
                return Ok(());
            }
            println!("Resolving {} for tunnel...", url);
            let tp = tunnel::start_dns_tunnel(&url, local_port, remote_port, None)?;
            println!("Tunnel active: localhost:{} -> {}:{}", tp.local_port, tp.instance_name, tp.remote_port);
        }

        Cmd::TunnelRemote { bastion, host, local_port, remote_port } => {
            if tunnel::test_port(local_port) {
                println!("Port {} already in use.", local_port);
                return Ok(());
            }
            println!("Starting remote tunnel via *{}* -> {}:{}", bastion, host, remote_port);
            let tp = tunnel::start_remote_tunnel_via_pattern(&bastion, &host, local_port, remote_port, None)?;
            println!("Tunnel active: localhost:{} -> {}:{} via {}", tp.local_port, host, remote_port, tp.instance_name);
        }

        Cmd::TunnelStop => {
            tunnel::stop_all_tunnels();
            if proxy::has_active_proxies() {
                println!("Cleaning up reverse proxies...");
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
        println!("  ALB target: {}:{}", target_ip, target_port);
        println!("  Via: {}", hop.name);

        let tp = tunnel::start_remote_tunnel_via_instance(
            &hop.id, &hop.name, target_ip, local_port, *target_port, None,
        )?;
        return Ok(Some(tp));
    }
    Ok(None)
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
