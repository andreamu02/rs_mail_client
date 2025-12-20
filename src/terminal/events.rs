use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::store::repo::MailRepository;
use crate::terminal::state::{AppState, Focus, ViewMode};

pub fn handle_key(key: KeyEvent, state: &mut AppState, repo: &dyn MailRepository) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),

        KeyCode::Esc => {
            if state.mode == ViewMode::Split {
                state.close_email();
                return Ok(false);
            }
            return Ok(true);
        }

        KeyCode::Enter => {
            // Only open on Enter
            state.open_selected(repo)?;
            return Ok(false);
        }

        KeyCode::Tab => {
            state.toggle_focus();
            return Ok(false);
        }

        KeyCode::Char('r') => {
            state.page_next(repo)?;
            // If not cached, ask daemon to sync this page and reload
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
            return Ok(false);
        }

        KeyCode::Char('R') => {
            state.page_prev(repo)?;
            return Ok(false);
        }

        _ => {}
    }

    match state.focus {
        Focus::List => handle_list_keys(key, state),
        Focus::Body => handle_body_keys(key, state),
    }
}

fn handle_list_keys(key: KeyEvent, state: &mut AppState) -> Result<bool> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => state.move_selection(1),
        KeyCode::Up | KeyCode::Char('k') => state.move_selection(-1),
        KeyCode::Home => state.list_state.select(Some(0)),
        KeyCode::End => {
            if !state.items.is_empty() {
                state.list_state.select(Some(state.items.len() - 1));
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_body_keys(key: KeyEvent, state: &mut AppState) -> Result<bool> {
    if state.mode != ViewMode::Split {
        return Ok(false);
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => state.scroll_body(1),
        KeyCode::Up | KeyCode::Char('k') => state.scroll_body(-1),
        KeyCode::PageDown => state.scroll_body(10),
        KeyCode::PageUp => state.scroll_body(-10),
        KeyCode::Home => state.body_scroll = 0,
        _ => {}
    }
    Ok(false)
}
