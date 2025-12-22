use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, Padding, Paragraph, Wrap},
};

use crate::terminal::state::{AppState, Focus, ViewMode};

pub fn render(f: &mut Frame, state: &mut AppState) {
    let [main_area, footer_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
        .margin(1)
        .areas::<2>(f.area());

    match state.mode {
        ViewMode::ListOnly => render_list_only(f, main_area, state),
        ViewMode::Split => render_split(f, main_area, state),
        ViewMode::Menu => render_menu(f, main_area, state),
        ViewMode::Help => render_help(f, main_area, state),
    }

    render_footer(f, footer_area, state);
}

fn render_list_only(f: &mut Frame, area: Rect, state: &mut AppState) {
    render_list(f, area, state, " inbox  (enter to open) ");
}

fn render_split(f: &mut Frame, area: Rect, state: &mut AppState) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
            .areas::<2>(area);

    render_list(f, left, state, &format!(" Inbox (page {}) ", state.page));
    render_email(f, right, state);
}

fn render_help(f: &mut Frame, area: Rect, state: &mut AppState) {
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .padding(Padding {
            left: 0,
            right: 2,
            top: 0,
            bottom: 0,
        });

    f.render_widget(block, area);
}

fn render_menu(f: &mut Frame, area: Rect, state: &mut AppState) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
            .areas::<2>(area);

    render_list(f, left, state, &format!(" Inbox (page {}) ", state.page));
    render_email(f, right, state);
}

fn render_list(f: &mut Frame, area: Rect, state: &mut AppState, title: &str) {
    let border_color = if state.focus == Focus::List {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .padding(Padding {
            left: 0,
            right: 2,
            top: 0,
            bottom: 0,
        });

    let selected = state.list_state.selected();

    let items: Vec<ListItem> = state
        .items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let is_sel = Some(i) == selected;

            let prefix = if is_sel { "▶ " } else { "  " };

            let top = Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(if is_sel {
                            Color::Yellow
                        } else {
                            Color::DarkGray
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    e.from_name.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" — "),
                Span::styled(
                    e.subject.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]);

            let bottom = Line::from(vec![
                Span::raw("  "),
                Span::styled(e.snippet.clone(), Style::default().fg(Color::Gray)),
            ]);

            let sel_style = Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

            let base_style = Style::default();

            ListItem::new(Text::from(vec![top, bottom])).style(if is_sel {
                sel_style
            } else {
                base_style
            })
        })
        .collect();

    // We set highlight_symbol to "" because we style rows ourselves
    let list = List::new(items).block(block);

    f.render_stateful_widget(list, area, &mut state.list_state);
}

fn render_email(f: &mut Frame, area: Rect, state: &AppState) {
    let border_color = if state.focus == Focus::Body {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Email ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let [meta_area, body_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas::<2>(inner);

    let (from_name, subject) = opened_email_meta(state);

    let meta = Text::from(vec![
        Line::from(vec![
            Span::styled("From: ", Style::default().fg(Color::Gray)),
            Span::styled(
                from_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Subject: ", Style::default().fg(Color::Gray)),
            Span::styled(subject, Style::default().add_modifier(Modifier::BOLD)),
        ]),
    ]);

    f.render_widget(Paragraph::new(meta), meta_area);

    let body_text = match &state.body {
        Some(b) => b.body.clone(),
        None => "Body not cached yet.\nRun daemon or request via IPC.".to_string(),
    };

    let formatted = format_body_for_tui(&body_text);

    let p = Paragraph::new(formatted)
        .wrap(Wrap { trim: false })
        .scroll((0, state.body_scroll));

    f.render_widget(p, body_area);
}

fn opened_email_meta(state: &AppState) -> (String, String) {
    let Some(id) = state.opened_id else {
        return ("(none)".to_string(), "(press Enter to open)".to_string());
    };

    if let Some(s) = state.items.iter().find(|x| x.id == id) {
        (s.from_name.clone(), s.subject.clone())
    } else {
        ("(not in current page)".to_string(), format!("UID {id}"))
    }
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hint = match state.mode {
        ViewMode::ListOnly => "j/k move  Enter open  r next20  R prev20  q quit",
        ViewMode::Split => "j/k move/scroll  Tab focus  Esc back  r next20  R prev20  q quit",
        ViewMode::Menu => "m Menu",
        ViewMode::Help => "h help",
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::Gray)),
        area,
    );
}

/// Make bodies readable:
/// - truncate absurd tracking-link lines
/// - break long tokens so wrapping works
fn format_body_for_tui(input: &str) -> String {
    let trimmed = trim_long_link_lines(input, 220);
    break_long_tokens(&trimmed, 70)
}

fn trim_long_link_lines(input: &str, max_chars: usize) -> String {
    input
        .lines()
        .map(|line| {
            let l = line.trim_end_matches('\r');
            if l.contains("http") && l.chars().count() > max_chars {
                let prefix: String = l.chars().take(max_chars).collect();
                format!("{prefix}…")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn break_long_tokens(input: &str, max_token_len: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut token_len = 0usize;

    for ch in input.chars() {
        if ch.is_whitespace() {
            token_len = 0;
            out.push(ch);
            continue;
        }

        token_len += 1;
        out.push(ch);

        if token_len >= max_token_len {
            out.push(' ');
            token_len = 0;
        }
    }

    out
}
