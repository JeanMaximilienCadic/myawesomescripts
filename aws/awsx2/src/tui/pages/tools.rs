//! Tools tab: static menu with Login, Resolve URL, Test Port, Stop All Tunnels.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::tui::app::{App, BgMessage, InputTag, Popup};
use crate::tui::ui::{C_BORDER, C_DIM, C_GOLD, C_TEXT};

const TOOLS: &[(&str, &str)] = &[
    ("Switch Profile",   "Change active AWS profile (reads ~/.aws/config)"),
    ("Switch Region",    "Change active AWS region (e.g. us-east-1)"),
    ("Login",            "Run aws sso login for a profile"),
    ("Resolve URL",      "Trace DNS -> EC2 / ALB / Fargate"),
    ("Test Port",        "Check if a local tunnel port is open"),
    ("Stop All Tunnels", "Kill all session-manager-plugin processes"),
];

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(1)])
        .split(area);

    render_menu(f, app, chunks[0]);
    render_description(f, app, chunks[1]);
}

fn render_menu(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = TOOLS.iter().map(|(name, _)| {
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(*name, Style::default().fg(C_TEXT)),
        ]))
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(" Tools ")
                .title_style(Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD)),
        )
        .highlight_style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    state.select(Some(app.tool_selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_description(f: &mut Frame, app: &App, area: Rect) {
    let (name, desc) = TOOLS.get(app.tool_selected).copied().unwrap_or(("", ""));
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {}", name), Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(format!("  {}", desc), Style::default().fg(C_DIM))),
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
        KeyCode::Up   | KeyCode::Char('k') => { if app.tool_selected > 0 { app.tool_selected -= 1; } }
        KeyCode::Down | KeyCode::Char('j') => { if app.tool_selected + 1 < TOOLS.len() { app.tool_selected += 1; } }
        KeyCode::Enter => execute_tool(app),
        _ => {}
    }
}

fn execute_tool(app: &mut App) {
    match app.tool_selected {
        0 => {
            let profiles = crate::aws::list_profiles();
            let current = app.profile.clone();
            let selected = profiles.iter().position(|p| *p == current).unwrap_or(0);
            app.popup = Popup::Select {
                title: "Switch AWS Profile".into(),
                items: profiles,
                selected,
                tag: InputTag::SwitchProfile,
            };
        }
        1 => {
            app.popup = Popup::Input {
                title: "Switch AWS Region".into(),
                placeholder: "e.g. us-east-1, ap-northeast-1".into(),
                value: app.region.clone(),
                tag: InputTag::SwitchRegion,
            };
        }
        2 => {
            app.popup = Popup::Input {
                title: "AWS SSO Login — Profile".into(),
                placeholder: app.profile.clone(),
                value: app.profile.clone(),
                tag: InputTag::LoginProfile,
            };
        }
        3 => {
            app.popup = Popup::Input {
                title: "Resolve URL or Hostname".into(),
                placeholder: "e.g. https://app.internal.example.com/".into(),
                value: String::new(),
                tag: InputTag::ResolveUrl,
            };
        }
        4 => {
            app.popup = Popup::Input {
                title: "Test Local Port".into(),
                placeholder: "e.g. 18000".into(),
                value: String::new(),
                tag: InputTag::TestPort,
            };
        }
        5 => {
            crate::tunnel::stop_all_tunnels();
            app.popup = Popup::Result {
                title: "Done".into(),
                body: "All SSM tunnel processes stopped.".into(),
                is_error: false,
            };
            app.refresh_tunnels();
        }
        _ => {}
    }
}

pub fn handle_input(app: &mut App, tag: InputTag, value: String) {
    match tag {
        InputTag::SwitchProfile => {
            if value.is_empty() { return; }
            std::env::set_var("AWS_PROFILE", &value);
            app.profile = value.clone();
            app.status_msg = Some(format!("Profile → {}  (refreshing...)", value));
            app.refresh_instances();
        }
        InputTag::SwitchRegion => {
            let region = value.trim().to_string();
            if region.is_empty() { return; }
            std::env::set_var("AWS_DEFAULT_REGION", &region);
            app.region = region.clone();
            app.status_msg = Some(format!("Region → {}  (refreshing...)", region));
            app.refresh_instances();
        }
        InputTag::LoginProfile => {
            let profile_str = value.clone();
            let profile_opt = if profile_str.is_empty() { None } else { Some(profile_str.clone()) };
            let tx = app.tx.clone();
            app.popup = Popup::Loading { message: format!("aws sso login --profile {}...", profile_str) };
            std::thread::spawn(move || {
                let result = crate::aws::sso_login(profile_opt.as_deref())
                    .and_then(|_| crate::aws::get_caller_identity(profile_opt.as_deref()));
                let _ = tx.send(BgMessage::ActionDone(result));
            });
        }
        InputTag::ResolveUrl => {
            let url = value.clone();
            let tx = app.tx.clone();
            app.popup = Popup::Loading { message: format!("Resolving {}...", url) };
            std::thread::spawn(move || {
                let result = crate::aws::resolve_dns_report(&url, None);
                let _ = tx.send(BgMessage::ActionDone(result));
            });
        }
        InputTag::TestPort => {
            let port: u16 = value.parse().unwrap_or(0);
            let ok = crate::tunnel::test_port(port);
            app.popup = Popup::Result {
                title: format!("Port {} Test", port),
                body: if ok {
                    format!("Port {} is OPEN (tunnel active or service running)", port)
                } else {
                    format!("Port {} is CLOSED", port)
                },
                is_error: !ok,
            };
        }
        _ => {}
    }
}
