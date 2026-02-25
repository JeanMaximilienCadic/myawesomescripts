//! VPN tab: connect/disconnect/setup for AWS Client VPN with SAML auth.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::tui::app::{App, BgMessage, InputTag, Popup};
use crate::tui::ui::{C_BORDER, C_DIM, C_GOLD, C_OK, C_DANGER, C_TEXT};

const VPN_ACTIONS: &[(&str, &str)] = &[
    ("Connect",    "Connect to VPN (enter MFA code)"),
    ("Disconnect", "Disconnect active VPN session"),
    ("Setup",      "Configure SSO credentials and .ovpn path"),
    ("Status",     "Check VPN connection status"),
];

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(1)])
        .split(area);

    render_menu(f, app, chunks[0]);
    render_details(f, app, chunks[1]);
}

fn render_menu(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = VPN_ACTIONS
        .iter()
        .map(|(name, _)| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(*name, Style::default().fg(C_TEXT)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(" VPN ")
                .title_style(Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD)),
        )
        .highlight_style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    state.select(Some(app.vpn_selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_details(f: &mut Frame, app: &App, area: Rect) {
    let (name, desc) = VPN_ACTIONS
        .get(app.vpn_selected)
        .copied()
        .unwrap_or(("", ""));

    let status_color = if app.vpn_status.starts_with("CONNECTED") { C_OK } else { C_DANGER };
    let config = &app.vpn_config;
    let password_display = if config.sso_password.is_empty() { "(not set)" } else { "********" };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status: ", Style::default().fg(C_DIM)),
            Span::styled(&app.vpn_status, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Configuration", Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("  Username:  ", Style::default().fg(C_DIM)),
            Span::styled(
                if config.sso_username.is_empty() { "(not set)" } else { &config.sso_username },
                Style::default().fg(C_TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Password:  ", Style::default().fg(C_DIM)),
            Span::styled(password_display, Style::default().fg(C_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  OVPN file: ", Style::default().fg(C_DIM)),
            Span::styled(
                if config.ovpn_path.is_empty() { "(not set)" } else { &config.ovpn_path },
                Style::default().fg(C_TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  DNS:       ", Style::default().fg(C_DIM)),
            Span::styled(format!("{} ({})", config.dns_server, config.dns_domain), Style::default().fg(C_TEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {} — {}", name, desc),
            Style::default().fg(C_DIM),
        )),
        Line::from(""),
        Line::from(Span::styled("  Press [Enter] to run.", Style::default().fg(C_TEXT))),
    ];

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(C_BORDER)),
    );
    f.render_widget(p, area);
}

// ── Key handling ──────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.vpn_selected > 0 {
                app.vpn_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.vpn_selected + 1 < VPN_ACTIONS.len() {
                app.vpn_selected += 1;
            }
        }
        KeyCode::Enter => execute_action(app),
        KeyCode::Char('r') => {
            app.vpn_status = if crate::vpn::is_connected() {
                format!(
                    "CONNECTED ({})",
                    crate::vpn::get_vpn_ip().unwrap_or_else(|| "?".into())
                )
            } else {
                "DISCONNECTED".into()
            };
        }
        _ => {}
    }
}

fn execute_action(app: &mut App) {
    match app.vpn_selected {
        // Connect
        0 => {
            if app.vpn_config.ovpn_path.is_empty() || app.vpn_config.sso_username.is_empty() {
                app.popup = Popup::Result {
                    title: "VPN Setup Required".into(),
                    body: "Run Setup first to configure credentials and .ovpn path.".into(),
                    is_error: true,
                };
                return;
            }
            app.popup = Popup::Input {
                title: "VPN MFA Code".into(),
                placeholder: "6-digit code from authenticator".into(),
                value: String::new(),
                tag: InputTag::VpnMfaCode,
            };
        }
        // Disconnect
        1 => {
            crate::vpn::disconnect();
            app.vpn_status = "DISCONNECTED".into();
            app.popup = Popup::Result {
                title: "VPN".into(),
                body: "VPN disconnected.".into(),
                is_error: false,
            };
        }
        // Setup
        2 => {
            app.popup = Popup::Input {
                title: "SSO Username/Email".into(),
                placeholder: "e.g. user@company.com".into(),
                value: app.vpn_config.sso_username.clone(),
                tag: InputTag::VpnSetupUsername,
            };
        }
        // Status
        3 => {
            let status = if crate::vpn::is_connected() {
                let ip = crate::vpn::get_vpn_ip().unwrap_or_else(|| "unknown".into());
                let pid = crate::vpn::find_vpn_pid()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "?".into());
                app.vpn_status = format!("CONNECTED ({})", ip);
                format!("VPN: CONNECTED\nIP: {}\nPID: {}", ip, pid)
            } else {
                app.vpn_status = "DISCONNECTED".into();
                "VPN: DISCONNECTED".into()
            };
            app.popup = Popup::Result {
                title: "VPN Status".into(),
                body: status,
                is_error: false,
            };
        }
        _ => {}
    }
}

pub fn handle_input(app: &mut App, tag: InputTag, value: String) {
    match tag {
        InputTag::VpnMfaCode => {
            let mfa = value.trim().to_string();
            if mfa.is_empty() {
                return;
            }
            let config = app.vpn_config.clone();
            let tx = app.tx.clone();
            app.popup = Popup::Loading {
                message: "[1/5] Preparing VPN config...".into(),
            };
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                let result = crate::vpn::connect(&config, &mfa, |msg| {
                    let _ = tx2.send(BgMessage::VpnProgress(msg.to_string()));
                });
                let msg = match &result {
                    Ok(pid) => {
                        let ip = crate::vpn::get_vpn_ip().unwrap_or_else(|| "?".into());
                        Ok(format!("VPN connected!\nIP: {}\nPID: {}", ip, pid))
                    }
                    Err(e) => Err(crate::error::AppError::Vpn(e.to_string())),
                };
                let _ = tx.send(BgMessage::VpnConnected(msg));
            });
        }
        InputTag::VpnSetupUsername => {
            app.vpn_config.sso_username = value;
            app.popup = Popup::Input {
                title: "SSO Password".into(),
                placeholder: "your SSO password".into(),
                value: app.vpn_config.sso_password.clone(),
                tag: InputTag::VpnSetupPassword,
            };
        }
        InputTag::VpnSetupPassword => {
            app.vpn_config.sso_password = value;
            app.popup = Popup::Input {
                title: "Path to .ovpn file".into(),
                placeholder: "e.g. /path/to/client.ovpn".into(),
                value: app.vpn_config.ovpn_path.clone(),
                tag: InputTag::VpnSetupOvpnPath,
            };
        }
        InputTag::VpnSetupOvpnPath => {
            app.vpn_config.ovpn_path = value;
            match crate::vpn::save_config(&app.vpn_config) {
                Ok(_) => {
                    app.popup = Popup::Result {
                        title: "VPN Setup".into(),
                        body: format!(
                            "Config saved!\nUsername: {}\nOVPN: {}",
                            app.vpn_config.sso_username, app.vpn_config.ovpn_path
                        ),
                        is_error: false,
                    };
                }
                Err(e) => {
                    app.popup = Popup::Result {
                        title: "VPN Setup Error".into(),
                        body: e.to_string(),
                        is_error: true,
                    };
                }
            }
        }
        _ => {}
    }
}
