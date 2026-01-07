use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::store::repo::MailRepository;
use crate::terminal::state::{AppState, Focus, ViewMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum KeyDispatch {
    Handled,
    #[default]
    Unhandled,
    Quit,
}

pub fn handle_key(key: KeyEvent, state: &mut AppState, repo: &dyn MailRepository) -> Result<bool> {
    let local = match state.focus {
        Focus::List => handle_list_keys(key, state)?,
        Focus::Body => handle_body_keys(key, state)?,
        Focus::Help => handle_help_keys(key, state)?,
    };

    match local {
        KeyDispatch::Quit => return Ok(true),
        KeyDispatch::Handled => return Ok(false),
        KeyDispatch::Unhandled => {}
    }

    match handle_global_keys(key, state, repo)? {
        KeyDispatch::Quit => Ok(true),
        _ => Ok(false),
    }
}

fn handle_global_keys(
    key: KeyEvent,
    state: &mut AppState,
    repo: &dyn MailRepository,
) -> Result<KeyDispatch> {
    match key.code {
        KeyCode::Char('q') => Ok(KeyDispatch::Quit),

        KeyCode::Char('h') => {
            state.previous = Some(state.mode);
            state.previous_focus = Some(state.focus);

            state.mode = ViewMode::Help;
            state.focus = Focus::Help;
            Ok(KeyDispatch::Handled)
        }

        KeyCode::Esc => {
            if state.mode == ViewMode::Split {
                state.close_email();
                return Ok(KeyDispatch::Handled);
            }
            Ok(KeyDispatch::Unhandled)
        }

        KeyCode::Enter => {
            state.open_selected(repo)?;
            Ok(KeyDispatch::Handled)
        }

        KeyCode::Tab => {
            state.toggle_focus();
            Ok(KeyDispatch::Handled)
        }

        KeyCode::Char('i') => {
            state.toggle_images(repo)?;
            Ok(KeyDispatch::Handled)
        }

        KeyCode::Char('r') => {
            state.page_next(repo)?;
            if state.items.is_empty() {
                #[cfg(unix)]
                {
                    let _ = crate::ipc::send(&crate::ipc::Request::SyncPage {
                        page: state.page,
                        page_size: state.page_size,
                    });
                }
                state.reload_page(repo)?;
            }
            Ok(KeyDispatch::Handled)
        }

        KeyCode::Char('R') => {
            state.page_prev(repo)?;
            Ok(KeyDispatch::Handled)
        }

        _ => Ok(KeyDispatch::Unhandled),
    }
}

fn handle_list_keys(key: KeyEvent, state: &mut AppState) -> Result<KeyDispatch> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_selection(1);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_selection(-1);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::Home => {
            state.list_state.select(Some(0));
            Ok(KeyDispatch::Handled)
        }
        KeyCode::End => {
            if !state.items.is_empty() {
                state.list_state.select(Some(state.items.len() - 1));
            }
            Ok(KeyDispatch::Handled)
        }
        _ => Ok(KeyDispatch::Unhandled),
    }
}

fn handle_body_keys(key: KeyEvent, state: &mut AppState) -> Result<KeyDispatch> {
    if state.mode != ViewMode::Split {
        return Ok(KeyDispatch::Unhandled);
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.scroll_body(1);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.scroll_body(-1);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::PageDown => {
            state.scroll_body(10);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::PageUp => {
            state.scroll_body(-10);
            Ok(KeyDispatch::Handled)
        }
        KeyCode::Home => {
            state.body_scroll = 0;
            Ok(KeyDispatch::Handled)
        }
        _ => Ok(KeyDispatch::Unhandled),
    }
}

fn handle_help_keys(key: KeyEvent, state: &mut AppState) -> Result<KeyDispatch> {
    if state.mode != ViewMode::Help {
        return Ok(KeyDispatch::Unhandled);
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Char('q') | KeyCode::Esc => {
            state.mode = state.previous.take().unwrap_or_default();
            state.focus = state.previous_focus.take().unwrap_or(Focus::List);
            Ok(KeyDispatch::Handled)
        }
        _ => Ok(KeyDispatch::Unhandled),
    }
}
