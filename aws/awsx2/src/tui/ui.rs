//! Top-level UI rendering: frame layout, header, tabs, status bar, popups.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Tabs, Wrap},
    Frame,
};

use super::app::{App, Popup, Tab};
use super::pages;

// ── Color palette ─────────────────────────────────────────────────────────────

pub const C_BORDER: Color = Color::Cyan;
pub const C_GOLD:   Color = Color::Yellow;
pub const C_OK:     Color = Color::Green;
pub const C_DANGER: Color = Color::Red;
pub const C_DIM:    Color = Color::DarkGray;
pub const C_TEXT:   Color = Color::White;

// ── Spinner frames ────────────────────────────────────────────────────────────

const SPINNER: &[char] = &['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];

pub fn spinner_char(tick: u8) -> char {
    SPINNER[(tick as usize) % SPINNER.len()]
}

// ── Top-level render ──────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));
    f.render_widget(outer_block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // header
            Constraint::Length(2),  // tabs
            Constraint::Min(1),     // body
            Constraint::Length(1),  // status bar
        ])
        .split(inner);

    render_header(f, app, vchunks[0]);
    render_tabs(f, app, vchunks[1]);
    render_body(f, app, vchunks[2]);
    render_status_bar(f, app, vchunks[3]);

    if app.loading {
        render_loading(f, app, area);
    }
    render_popup(f, app, area);
}

// ── Header ────────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let hchunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // ASCII logo
    let logo = Paragraph::new(vec![
        Line::from(Span::styled(
            "  █████╗ ██╗    ██╗███████╗██╗  ██╗██████╗",
            Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " ██╔══██╗██║    ██║██╔════╝╚██╗██╔╝╚════██╗",
            Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " ███████║██║ █╗ ██║███████╗ ╚███╔╝  █████╔╝",
            Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " ██╔══██║██║███╗██║╚════██║ ██╔██╗  ╚═══██╗",
            Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD),
        )),
    ]);
    f.render_widget(logo, hchunks[0]);

    // Profile + region info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Profile:   ", Style::default().fg(C_DIM)),
            Span::styled(&app.profile, Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Region:    ", Style::default().fg(C_DIM)),
            Span::styled(&app.region, Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Instances: ", Style::default().fg(C_DIM)),
            Span::styled(app.instances.len().to_string(), Style::default().fg(C_TEXT)),
            Span::styled("  Tunnels: ", Style::default().fg(C_DIM)),
            Span::styled(app.tunnels.len().to_string(), Style::default().fg(C_TEXT)),
        ]),
        Line::from(""),
    ]).alignment(Alignment::Right);
    f.render_widget(info, hchunks[1]);
}

// ── Tabs ──────────────────────────────────────────────────────────────────────

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::titles().iter().map(|t| Line::from(Span::raw(*t))).collect();

    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .highlight_style(
            Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED),
        )
        .style(Style::default().fg(C_DIM))
        .divider(Span::styled(" │ ", Style::default().fg(C_DIM)));

    let tabs_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(C_DIM));
    f.render_widget(tabs_block, area);

    let tabs_inner = Rect { x: area.x + 2, y: area.y, width: area.width.saturating_sub(2), height: area.height };
    f.render_widget(tabs, tabs_inner);
}

// ── Body ──────────────────────────────────────────────────────────────────────

fn render_body(f: &mut Frame, app: &App, area: Rect) {
    match app.tab {
        Tab::Instances => pages::instances::render(f, app, area),
        Tab::Tunnels   => pages::tunnels::render(f, app, area),
        Tab::Tools     => pages::tools::render(f, app, area),
        Tab::Vpn       => pages::vpn::render(f, app, area),
    }
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.tab {
        Tab::Instances => " [Tab] Switch  [s] Start  [S] Stop  [f] Force-stop  [r] Refresh  [/] Filter  [?] Help  [q] Quit",
        Tab::Tunnels   => " [Tab] Switch  [n] By instance  [u] By URL  [b] Via bastion  [d] Stop  [A] Stop all  [r] Refresh  [?] Help  [q] Quit",
        Tab::Tools     => " [Tab] Switch  [j/k] Navigate  [Enter] Execute  [?] Help  [q] Quit",
        Tab::Vpn       => " [Tab] Switch  [j/k] Navigate  [Enter] Execute  [r] Refresh status  [?] Help  [q] Quit",
    };

    let text = if let Some(ref msg) = app.status_msg {
        Line::from(vec![
            Span::styled(" > ", Style::default().fg(C_GOLD)),
            Span::styled(msg.as_str(), Style::default().fg(C_TEXT)),
        ])
    } else {
        Line::from(Span::styled(hints, Style::default().fg(C_DIM)))
    };

    f.render_widget(Paragraph::new(text), area);
}

// ── Loading overlay ───────────────────────────────────────────────────────────

fn render_loading(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(40, 3, area);
    f.render_widget(Clear, popup_area);
    let msg = format!(" {} {} ", spinner_char(app.spinner_tick), app.loading_message);
    let p = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .style(Style::default().fg(C_GOLD))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(C_BORDER)));
    f.render_widget(p, popup_area);
}

// ── Popups ────────────────────────────────────────────────────────────────────

fn render_popup(f: &mut Frame, app: &App, area: Rect) {
    match &app.popup {
        Popup::None => {}
        Popup::Help => render_help(f, area),
        Popup::Input { title, placeholder, value, .. } => {
            render_input_popup(f, area, title, placeholder, value);
        }
        Popup::Select { title, items, selected, .. } => {
            render_select_popup(f, area, title, items, *selected);
        }
        Popup::Confirm { message, selected_yes, .. } => {
            render_confirm(f, area, message, *selected_yes);
        }
        Popup::Result { title, body, is_error } => {
            render_result(f, area, title, body, *is_error);
        }
        Popup::Loading { message } => {
            let popup_area = centered_rect(50, 3, area);
            f.render_widget(Clear, popup_area);
            let p = Paragraph::new(format!(" {} {} ", spinner_char(app.spinner_tick), message))
                .alignment(Alignment::Center)
                .style(Style::default().fg(C_GOLD))
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(C_BORDER)));
            f.render_widget(p, popup_area);
        }
    }
}

fn render_input_popup(f: &mut Frame, area: Rect, title: &str, placeholder: &str, value: &str) {
    let popup_area = centered_rect(60, 7, area);
    f.render_widget(Clear, popup_area);

    let display = if value.is_empty() {
        Span::styled(placeholder, Style::default().fg(C_DIM))
    } else {
        Span::styled(value, Style::default().fg(C_TEXT))
    };

    let text = vec![
        Line::from(""),
        Line::from(display),
        Line::from(""),
        Line::from(Span::styled("[Enter] Confirm  [Esc] Cancel  [Backspace] Delete", Style::default().fg(C_DIM))),
    ];

    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .title_style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_BORDER)),
        );
    f.render_widget(p, popup_area);
}

fn render_confirm(f: &mut Frame, area: Rect, message: &str, selected_yes: bool) {
    let popup_area = centered_rect(55, 8, area);
    f.render_widget(Clear, popup_area);

    let cancel_style = if !selected_yes {
        Style::default().fg(Color::Black).bg(C_GOLD)
    } else {
        Style::default().fg(C_DIM)
    };
    let ok_style = if selected_yes {
        Style::default().fg(Color::Black).bg(C_DANGER)
    } else {
        Style::default().fg(C_DIM)
    };

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(C_TEXT))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [ Cancel ]  ", cancel_style),
            Span::raw("     "),
            Span::styled("  [ Yes ]  ", ok_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("[Tab/←/→] Toggle  [Enter] Confirm  [Esc] Cancel", Style::default().fg(C_DIM))),
    ];

    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Confirm ")
                .title_style(Style::default().fg(C_DANGER).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(C_DANGER)),
        );
    f.render_widget(p, popup_area);
}

fn render_result(f: &mut Frame, area: Rect, title: &str, body: &str, is_error: bool) {
    let lines: Vec<Line> = body.lines().map(|l| Line::from(l.to_string())).collect();
    let height = (lines.len() as u16 + 6).min(area.height.saturating_sub(4));
    let popup_area = centered_rect(65, height, area);
    f.render_widget(Clear, popup_area);

    let border_color = if is_error { C_DANGER } else { C_OK };
    let mut content = vec![Line::from("")];
    content.extend(lines);
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("[Enter/Esc] Close", Style::default().fg(C_DIM))));

    let p = Paragraph::new(content)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .title_style(Style::default().fg(border_color).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );
    f.render_widget(p, popup_area);
}

fn render_select_popup(f: &mut Frame, area: Rect, title: &str, items: &[String], selected: usize) {
    const VISIBLE: usize = 12;
    let height = (items.len().min(VISIBLE) as u16 + 4).max(5);
    let popup_area = centered_rect(50, height, area);
    f.render_widget(Clear, popup_area);

    let scroll_offset = if selected >= VISIBLE { selected - VISIBLE + 1 } else { 0 };
    let mut lines: Vec<Line> = items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(VISIBLE)
        .map(|(i, item)| {
            if i == selected {
                Line::from(Span::styled(
                    format!("▸ {}", item),
                    Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(format!("  {}", item), Style::default().fg(C_TEXT)))
            }
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [j/k] Navigate  [Enter] Select  [Esc] Cancel",
        Style::default().fg(C_DIM),
    )));

    let p = Paragraph::new(lines).block(
        Block::default()
            .title(format!(" {} ", title))
            .title_style(Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(C_BORDER)),
    );
    f.render_widget(p, popup_area);
}

fn render_help(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 30, area);
    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        section_line("Global"),
        key_line("Tab / Shift+Tab", "Cycle tabs"),
        key_line("q / Ctrl+c",      "Quit"),
        key_line("?",               "Toggle this help"),
        Line::from(""),
        section_line("Instances tab"),
        key_line("j/k or Up/Down",  "Navigate rows"),
        key_line("s",               "Start selected instance"),
        key_line("S",               "Stop selected instance"),
        key_line("f",               "Force-stop selected instance"),
        key_line("r",               "Refresh list"),
        key_line("/",               "Filter by name / ID / type"),
        key_line("Esc",             "Clear filter"),
        Line::from(""),
        section_line("Tunnels tab"),
        key_line("j/k or Up/Down",  "Navigate rows"),
        key_line("n",               "New tunnel by instance pattern"),
        key_line("u",               "New tunnel by URL (auto-bastion)"),
        key_line("b",               "New tunnel via specific bastion"),
        key_line("d / Del",         "Stop selected tunnel"),
        key_line("A",               "Stop ALL tunnels"),
        key_line("r",               "Refresh tunnel list"),
        Line::from(""),
        section_line("Tools tab"),
        key_line("j/k or Up/Down",  "Navigate"),
        key_line("Enter",           "Execute selected tool"),
        Line::from(""),
        Line::from(Span::styled("  [Esc / ?] Close", Style::default().fg(C_DIM))),
    ];

    let p = Paragraph::new(lines).block(
        Block::default()
            .title(" Help ")
            .title_style(Style::default().fg(C_BORDER).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(C_BORDER)),
    );
    f.render_widget(p, popup_area);
}

fn section_line(title: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {} ", title),
        Style::default().fg(C_GOLD).add_modifier(Modifier::BOLD),
    ))
}

fn key_line(key: &'static str, desc: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<22}", key), Style::default().fg(C_TEXT)),
        Span::styled(desc, Style::default().fg(C_DIM)),
    ])
}

// ── Layout helpers ────────────────────────────────────────────────────────────

pub fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let vl = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Length(height),
            Constraint::Percentage(20),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vl[1])[1]
}
