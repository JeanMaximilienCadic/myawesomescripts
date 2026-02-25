//! Instances tab: table + key handlers.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::models::{InstanceState, SsmStatus, TunnelStatus};
use crate::tui::app::{App, BgMessage, ConfirmTag, Popup};
use crate::tui::ui::{C_BORDER, C_DIM, C_DANGER, C_GOLD, C_OK, C_TEXT};

// ── Render ────────────────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let (filter_area, table_area) = if app.instance_filter_active || !app.instance_filter.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    if let Some(fa) = filter_area {
        let bar = Paragraph::new(Line::from(vec![
            Span::styled(" Filter: ", Style::default().fg(C_GOLD)),
            Span::styled(&app.instance_filter, Style::default().fg(C_TEXT)),
            Span::styled("█", Style::default().fg(C_BORDER)),
        ]));
        f.render_widget(bar, fa);
    }

    render_table(f, app, table_area);
}

fn render_table(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Instance ID").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Name").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Type").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("State").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("SSM").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Tunnel").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        Cell::from("Private IP").style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
    ]).height(1);

    let filtered = app.filtered_instances();

    let rows: Vec<Row> = filtered.iter().map(|inst| {
        let state_style = match inst.state {
            InstanceState::Running  => Style::default().fg(C_OK),
            InstanceState::Stopped  => Style::default().fg(C_DANGER),
            _                       => Style::default().fg(Color::Yellow),
        };

        let ssm_cell = match inst.ssm_status {
            SsmStatus::Online  => Cell::from("● Online").style(Style::default().fg(C_OK)),
            SsmStatus::Offline => Cell::from("◌ Offline").style(Style::default().fg(C_DANGER)),
            SsmStatus::Unknown => Cell::from("-").style(Style::default().fg(C_DIM)),
        };

        let tunnel_cell = match &inst.tunnel {
            Some(t) => {
                let label = format!("{}:{}", t.local_port, t.remote_host.as_deref().unwrap_or("?"));
                let color = if t.status == TunnelStatus::Active { C_OK } else { C_DANGER };
                Cell::from(label).style(Style::default().fg(color))
            }
            None => Cell::from("-").style(Style::default().fg(C_DIM)),
        };

        Row::new(vec![
            Cell::from(inst.id.clone()),
            Cell::from(inst.name.clone()),
            Cell::from(inst.instance_type.clone()),
            Cell::from(inst.state.as_str().to_string()).style(state_style),
            ssm_cell,
            tunnel_cell,
            Cell::from(inst.private_ip.clone().unwrap_or_else(|| "-".into())),
        ]).height(1)
    }).collect();

    let widths = [
        Constraint::Length(20),
        Constraint::Percentage(25),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(20),
        Constraint::Length(16),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER))
                .title(" Instances ")
                .title_style(Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD)),
        )
        .row_highlight_style(
            Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default();
    state.select(Some(app.instance_selected));
    f.render_stateful_widget(table, area, &mut state);
}

// ── Key handling ──────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if app.instance_filter_active {
        match key.code {
            KeyCode::Esc   => { app.instance_filter_active = false; app.instance_filter.clear(); }
            KeyCode::Enter => { app.instance_filter_active = false; }
            KeyCode::Backspace => { app.instance_filter.pop(); }
            KeyCode::Char(c) => { app.instance_filter.push(c); app.instance_selected = 0; }
            _ => {}
        }
        return;
    }

    let count = app.filtered_instances().len();
    match key.code {
        KeyCode::Up   | KeyCode::Char('k') => { if app.instance_selected > 0 { app.instance_selected -= 1; } }
        KeyCode::Down | KeyCode::Char('j') => { if app.instance_selected + 1 < count { app.instance_selected += 1; } }
        KeyCode::Char('g') => { app.instance_selected = 0; }
        KeyCode::Char('G') => { app.instance_selected = count.saturating_sub(1); }
        KeyCode::Char('r') => { app.refresh_instances(); }
        KeyCode::Char('/') => { app.instance_filter_active = true; app.instance_filter.clear(); }
        KeyCode::Esc => { if !app.instance_filter.is_empty() { app.instance_filter.clear(); } }
        KeyCode::Char('s') => action_start(app),
        KeyCode::Char('S') => action_stop(app, false),
        KeyCode::Char('f') => action_stop(app, true),
        _ => {}
    }
}

fn action_start(app: &mut App) {
    if let Some(inst) = app.selected_instance().cloned() {
        let tx = app.tx.clone();
        let id = inst.id.clone();
        let name = inst.name.clone();
        app.loading = true;
        app.loading_message = format!("Starting {}...", name);
        std::thread::spawn(move || {
            let result = crate::aws::start_instance(&id, None).map(|_| format!("Started {}", name));
            let _ = tx.send(BgMessage::ActionDone(result));
        });
    }
}

fn action_stop(app: &mut App, force: bool) {
    if let Some(inst) = app.selected_instance().cloned() {
        let msg = if force {
            format!("Force-stop '{}' ({})?\nThis may cause data loss!", inst.name, inst.id)
        } else {
            format!("Stop '{}'?", inst.name)
        };
        app.popup = Popup::Confirm {
            message: msg,
            tag: if force { ConfirmTag::ForceStopInstance } else { ConfirmTag::StopInstance },
            selected_yes: false,
        };
    }
}

pub fn handle_confirm(app: &mut App, tag: ConfirmTag, confirmed: bool) {
    if !confirmed { return; }
    match tag {
        ConfirmTag::StopInstance | ConfirmTag::ForceStopInstance => {
            let force = matches!(tag, ConfirmTag::ForceStopInstance);
            if let Some(inst) = app.selected_instance().cloned() {
                let tx = app.tx.clone();
                let id = inst.id.clone();
                let name = inst.name.clone();
                app.loading = true;
                app.loading_message = if force { "Force-stopping...".into() } else { "Stopping...".into() };
                std::thread::spawn(move || {
                    let result = crate::aws::stop_instance(&id, force, None)
                        .map(|_| format!("{} {}", if force { "Force-stopped" } else { "Stopped" }, name));
                    let _ = tx.send(BgMessage::ActionDone(result));
                });
            }
        }
        _ => {}
    }
}
