//! Tunnels tab: table of active SSM tunnels + multi-step creation wizard.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Cell, Row, Table, TableState},
    Frame,
};
use ratatui::layout::Rect;

use crate::error::Result as AppResult;
use crate::models::TunnelProcess;
use crate::tui::app::{App, BgMessage, ConfirmTag, InputTag, Popup, WizardBuf};
use crate::tui::ui::{C_BORDER, C_DANGER, C_GOLD, C_OK};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("#").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Local Port").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Remote").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Instance / Bastion").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Status / Latency").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("PID").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
    ]).height(1);

    let rows: Vec<Row> = app.tunnels.iter().enumerate().map(|(i, t)| {
        let status_cell = match (t.port_open, t.latency_ms) {
            (true, Some(ms)) => Cell::from(format!("● OK  {}ms", ms)).style(Style::default().fg(C_OK)),
            (true, None)     => Cell::from("▲ OPEN").style(Style::default().fg(crate::tui::ui::C_GOLD)),
            _                => Cell::from("◌ DOWN").style(Style::default().fg(C_DANGER)),
        };
        let remote = match &t.remote_host {
            Some(h) => format!("{}:{}", h, t.remote_port),
            None    => format!(":{}", t.remote_port),
        };
        Row::new(vec![
            Cell::from((i + 1).to_string()),
            Cell::from(format!("localhost:{}", t.local_port)),
            Cell::from(remote),
            Cell::from(t.instance_name.clone()),
            status_cell,
            Cell::from(t.pid.to_string()),
        ]).height(1)
    }).collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(16),
        Constraint::Percentage(28),
        Constraint::Percentage(30),
        Constraint::Length(14),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(" Tunnels ")
                .title_style(Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD)),
        )
        .row_highlight_style(
            Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default();
    if !app.tunnels.is_empty() { state.select(Some(app.tunnel_selected)); }
    f.render_stateful_widget(table, area, &mut state);
}

// ── Key handling ──────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut App, key: KeyEvent) {
    let count = app.tunnels.len();
    match key.code {
        KeyCode::Up   | KeyCode::Char('k') => { if app.tunnel_selected > 0 { app.tunnel_selected -= 1; } }
        KeyCode::Down | KeyCode::Char('j') => { if app.tunnel_selected + 1 < count { app.tunnel_selected += 1; } }
        KeyCode::Char('r') => { app.refresh_tunnels(); }
        KeyCode::Char('n') => start_wizard_by_instance(app),
        KeyCode::Char('u') => start_wizard_by_url(app),
        KeyCode::Char('b') => start_wizard_by_bastion(app),
        KeyCode::Char('d') | KeyCode::Delete => confirm_stop_tunnel(app),
        KeyCode::Char('A') => confirm_stop_all(app),
        _ => {}
    }
}

fn start_wizard_by_instance(app: &mut App) {
    app.wizard_buf = WizardBuf::default();
    app.popup = Popup::Input {
        title: "New Tunnel — Instance Name Pattern".into(),
        placeholder: "e.g. web-server, bastion".into(),
        value: String::new(),
        tag: InputTag::NewTunnelPattern,
    };
}

fn start_wizard_by_url(app: &mut App) {
    app.wizard_buf = WizardBuf::default();
    app.popup = Popup::Input {
        title: "New Tunnel — Target URL (auto-selects bastion)".into(),
        placeholder: "e.g. http://mlflow.internal.example.com/".into(),
        value: String::new(),
        tag: InputTag::NewTunnelUrl,
    };
}

fn start_wizard_by_bastion(app: &mut App) {
    app.wizard_buf = WizardBuf::default();
    app.popup = Popup::Input {
        title: "Remote Tunnel — Bastion Name Pattern".into(),
        placeholder: "e.g. bastion".into(),
        value: String::new(),
        tag: InputTag::NewTunnelBastionPattern,
    };
}

fn confirm_stop_tunnel(app: &mut App) {
    if let Some(t) = app.selected_tunnel() {
        let idx = app.tunnel_selected;
        app.popup = Popup::Confirm {
            message: format!("Stop tunnel localhost:{} -> {}?", t.local_port, t.instance_name),
            tag: ConfirmTag::StopTunnel(idx),
            selected_yes: false,
        };
    }
}

fn confirm_stop_all(app: &mut App) {
    if !app.tunnels.is_empty() {
        app.popup = Popup::Confirm {
            message: format!("Stop all {} active tunnel(s)?", app.tunnels.len()),
            tag: ConfirmTag::StopAllTunnels,
            selected_yes: false,
        };
    }
}

pub fn handle_confirm(app: &mut App, tag: ConfirmTag, confirmed: bool) {
    if !confirmed { return; }
    match tag {
        ConfirmTag::StopTunnel(idx) => {
            if let Some(t) = app.tunnels.get(idx) {
                let pid = t.pid;
                crate::tunnel::stop_tunnel(pid);
                app.tunnels.remove(idx);
                app.tunnel_selected = app.tunnel_selected.min(app.tunnels.len().saturating_sub(1));
                app.status_msg = Some(format!("Stopped tunnel PID {}", pid));
            }
        }
        ConfirmTag::StopAllTunnels => {
            crate::tunnel::stop_all_tunnels();
            app.tunnels.clear();
            app.tunnel_selected = 0;
            app.status_msg = Some("All tunnels stopped".into());
        }
        _ => {}
    }
}

/// Wizard: handle input step completion for tunnel creation.
pub fn handle_input(app: &mut App, tag: InputTag, value: String) {
    match tag {
        // === By instance: pattern -> local port -> remote port ===
        InputTag::NewTunnelPattern => {
            app.wizard_buf.pattern = value;
            app.popup = Popup::Input {
                title: "New Tunnel — Local Port".into(),
                placeholder: "e.g. 18000".into(),
                value: String::new(),
                tag: InputTag::NewTunnelLocalPort,
            };
        }
        InputTag::NewTunnelLocalPort => {
            app.wizard_buf.local_port = value;
            app.popup = Popup::Input {
                title: "New Tunnel — Remote Port".into(),
                placeholder: "e.g. 8000".into(),
                value: "8000".into(),
                tag: InputTag::NewTunnelRemotePort,
            };
        }
        InputTag::NewTunnelRemotePort => {
            app.wizard_buf.remote_port = value;
            let pattern     = app.wizard_buf.pattern.clone();
            let local_port: u16  = app.wizard_buf.local_port.parse().unwrap_or(18000);
            let remote_port: u16 = app.wizard_buf.remote_port.parse().unwrap_or(8000);
            let tx = app.tx.clone();
            app.popup = Popup::Loading { message: format!("Connecting to *{}*...", pattern) };
            std::thread::spawn(move || {
                let result = crate::tunnel::start_tunnel_by_pattern(&pattern, local_port, remote_port, None);
                let _ = tx.send(BgMessage::TunnelStarted(result));
            });
        }

        // === By URL: url -> local port ===
        InputTag::NewTunnelUrl => {
            app.wizard_buf.url = value;
            app.popup = Popup::Input {
                title: "New Tunnel — Local Port".into(),
                placeholder: "e.g. 8080".into(),
                value: "8080".into(),
                tag: InputTag::NewTunnelUrlLocalPort,
            };
        }
        InputTag::NewTunnelUrlLocalPort => {
            app.wizard_buf.local_port = value;
            app.popup = Popup::Input {
                title: "New Tunnel — Remote Port (blank = auto-detect)".into(),
                placeholder: "e.g. 8501 (leave empty to auto-detect)".into(),
                value: String::new(),
                tag: InputTag::NewTunnelUrlRemotePort,
            };
        }
        InputTag::NewTunnelUrlRemotePort => {
            app.wizard_buf.remote_port = value;
            let url = app.wizard_buf.url.clone();
            let local_port: u16 = app.wizard_buf.local_port.parse().unwrap_or(8080);
            let remote_port: Option<u16> = app.wizard_buf.remote_port.parse().ok();
            let tx = app.tx.clone();
            app.popup = Popup::Loading { message: "Resolving via ALB / bastions...".into() };
            std::thread::spawn(move || {
                let host = crate::aws::strip_url_to_host(&url);
                // Try smart ALB resolution first
                let result = try_alb_tunnel_bg(&host, &url, local_port, remote_port);
                let _ = tx.send(BgMessage::TunnelStarted(result));
            });
        }

        // === By bastion: bastion -> host -> local port -> remote port ===
        InputTag::NewTunnelBastionPattern => {
            app.wizard_buf.bastion = value;
            app.popup = Popup::Input {
                title: "Remote Tunnel — Target Host (private IP)".into(),
                placeholder: "e.g. 10.0.1.42".into(),
                value: String::new(),
                tag: InputTag::NewTunnelBastionHost,
            };
        }
        InputTag::NewTunnelBastionHost => {
            app.wizard_buf.host = value;
            app.popup = Popup::Input {
                title: "Remote Tunnel — Local Port".into(),
                placeholder: "e.g. 8501".into(),
                value: "8501".into(),
                tag: InputTag::NewTunnelBastionLocalPort,
            };
        }
        InputTag::NewTunnelBastionLocalPort => {
            app.wizard_buf.local_port = value;
            app.popup = Popup::Input {
                title: "Remote Tunnel — Remote Port".into(),
                placeholder: "e.g. 8501".into(),
                value: "8501".into(),
                tag: InputTag::NewTunnelBastionRemotePort,
            };
        }
        InputTag::NewTunnelBastionRemotePort => {
            app.wizard_buf.remote_port = value;
            let bastion     = app.wizard_buf.bastion.clone();
            let host        = app.wizard_buf.host.clone();
            let local_port: u16  = app.wizard_buf.local_port.parse().unwrap_or(8501);
            let remote_port: u16 = app.wizard_buf.remote_port.parse().unwrap_or(8501);
            let tx = app.tx.clone();
            app.popup = Popup::Loading { message: format!("Connecting via {}...", bastion) };
            std::thread::spawn(move || {
                let result = crate::tunnel::start_remote_tunnel_via_pattern(&bastion, &host, local_port, remote_port, None);
                let _ = tx.send(BgMessage::TunnelStarted(result));
            });
        }

        _ => {}
    }
}

/// Try smart ALB resolution, fall back to bastions. Used by the TUI wizard in a bg thread.
fn try_alb_tunnel_bg(host: &str, url: &str, local_port: u16, remote_port: Option<u16>) -> AppResult<TunnelProcess> {
    // Try ALB-aware resolution
    if let Some(alb_arn) = crate::aws::find_alb_for_hostname(host, None).unwrap_or(None) {
        let targets = crate::aws::get_alb_healthy_targets(&alb_arn, remote_port, None).unwrap_or_default();
        for (target_ip, target_port) in &targets {
            let target_sgs = crate::aws::get_target_sg_ids(target_ip, None).unwrap_or_default();
            if target_sgs.is_empty() { continue; }
            let allowed = crate::aws::get_allowed_source_sgs(&target_sgs, *target_port, None).unwrap_or_default();
            if let Some(hop) = crate::aws::find_ssm_hop_by_sgs(&allowed, None).unwrap_or(None) {
                return crate::tunnel::start_remote_tunnel_via_instance(
                    &hop.id, &hop.name, target_ip, local_port, *target_port, None,
                );
            }
        }
    }
    // Fall back to bastions
    crate::tunnel::start_url_tunnel_via_any_bastion(url, local_port, None)
}
