pub mod structs;
use color_eyre::eyre::{Ok, Result};
use ratatui::crossterm::event::KeyEvent;
use ratatui::prelude::Stylize;
use ratatui::style::Style;
use ratatui::text::ToSpan;
use ratatui::widgets::{ListState, Padding, Paragraph};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event},
    layout::{Constraint, Layout},
    style::Color,
    widgets::{Block, BorderType, List, ListItem, Widget},
};

use structs::{AppState, FormAction, TodoItem};

pub fn run_terminal() -> Result<()> {
    let mut state = AppState {
        is_add_new: false,
        list_state: ListState::default(),
        items: Vec::<TodoItem>::default(),
        input_value: String::default(),
    };
    state.is_add_new = false;

    color_eyre::install()?;

    let terminal = ratatui::init();
    let result = run(terminal, &mut state);

    ratatui::restore();

    result
}

fn run(mut terminal: DefaultTerminal, app_state: &mut AppState) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, app_state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if app_state.is_add_new {
            match handle_add_new(key, app_state) {
                FormAction::None => {}
                FormAction::Submit => {
                    app_state.is_add_new = false;
                    app_state.items.push(TodoItem {
                        is_done: false,
                        description: app_state.input_value.clone(),
                    });
                    app_state.input_value.clear();
                }
                FormAction::Escape => {
                    app_state.is_add_new = false;
                    app_state.input_value.clear();
                }
            }
        } else {
            if handle_key(key, app_state) {
                break;
            }
        }
    }
    Ok(())
}

fn handle_add_new(key: KeyEvent, app_state: &mut AppState) -> FormAction {
    match key.code {
        event::KeyCode::Enter => {
            return FormAction::Submit;
        }
        event::KeyCode::Esc => {
            return FormAction::Escape;
        }
        event::KeyCode::Char(c) => {
            app_state.input_value.push(c);
        }
        event::KeyCode::Backspace => {
            app_state.input_value.pop();
        }
        _ => {}
    }
    FormAction::None
}

fn handle_key(key: KeyEvent, app_state: &mut AppState) -> bool {
    match key.code {
        event::KeyCode::Esc => {
            return true;
        }
        event::KeyCode::Char(char) => match char {
            'a' => {
                app_state.is_add_new = true;
            }

            'd' => {
                if let Some(index) = app_state.list_state.selected() {
                    app_state.items.remove(index);
                }
            }

            'j' => {
                app_state.list_state.select_next();
            }
            'k' => {
                app_state.list_state.select_previous();
            }
            _ => {}
        },

        _ => {}
    }
    false
}

fn render(frame: &mut Frame, app_state: &mut AppState) {
    let [border_area] = Layout::vertical([Constraint::Fill(1)])
        .margin(1)
        .areas(frame.area());

    if app_state.is_add_new {
        Paragraph::new(app_state.input_value.as_str())
            .block(
                Block::bordered()
                    .title(" Input Description ".to_span().into_centered_line())
                    .fg(Color::Green)
                    .padding(Padding::uniform(1))
                    .border_type(BorderType::Rounded),
            )
            .render(border_area, frame.buffer_mut());
    } else {
        let [inner_area] = Layout::vertical([Constraint::Fill(1)])
            .margin(1)
            .areas(border_area);

        Block::bordered()
            .border_type(BorderType::Rounded)
            .fg(Color::Yellow)
            .render(border_area, frame.buffer_mut());

        let list = List::new(
            app_state
                .items
                .iter()
                .map(|x| ListItem::from(x.description.as_str())),
        )
        .highlight_symbol(">")
        .highlight_style(Style::default().fg(Color::Green));

        frame.render_stateful_widget(list, inner_area, &mut app_state.list_state);
    }
}
