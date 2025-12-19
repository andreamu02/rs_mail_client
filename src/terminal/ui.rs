use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::terminal::state::{AppState, Focus};

pub fn render(f: &mut Frame, state: &AppState) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
            .margin(1)
            .areas(f.area());

    let list_border = if state.focus == Focus::List {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let body_border = if state.focus == Focus::Body {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    // LEFT: list
    let list_block = Block::default()
        .title(format!(" Inbox (page {}) ", state.page))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(list_border));

    let items: Vec<ListItem> = state
        .items
        .iter()
        .map(|e| {
            let subj = Span::styled(
                e.subject.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            );
            let snip = Span::styled(e.snippet.clone(), Style::default().fg(Color::Gray));
            ListItem::new(Text::from(vec![Line::from(subj), Line::from(snip)]))
        })
        .collect();

    let list = List::new(items)
        .block(list_block)
        .highlight_symbol("➜ ")
        .highlight_style(Style::default().fg(Color::Green));

    f.render_stateful_widget(list, left, &mut state.list_state.clone());

    // RIGHT: body
    let body_block = Block::default()
        .title(" Email ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(body_border));

    let body_text = match &state.body {
        Some(b) => b.body.clone(),
        None => {
            if state.items.is_empty() {
                "No cached emails.\nRun: rs_mail_client daemon\n(or fetch/store emails first)."
                    .to_string()
            } else {
                "Body not cached for this email yet.\nRun: rs_mail_client daemon\n(to fetch and store bodies).".to_string()
            }
        }
    };

    let p = Paragraph::new(body_text)
        .block(body_block)
        .wrap(Wrap { trim: false })
        .scroll((0, state.body_scroll));

    f.render_widget(p, right);

    // Footer hint (optional)
    let footer = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area())[1];
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" move  "),
        Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" focus  "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" next20  "),
        Span::styled("R", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" prev20  "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]));
    f.render_widget(hint, footer);
}
