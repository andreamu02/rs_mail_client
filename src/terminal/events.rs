use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::store::repo::MailRepository;
use crate::terminal::state::{AppState, Focus};

pub fn handle_key(key: KeyEvent, state: &mut AppState, repo: &dyn MailRepository) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),

        KeyCode::Tab => {
            state.toggle_focus();
            return Ok(false);
        }

        KeyCode::Enter => {
            // Enter toggles focus and loads body (if needed)
            state.toggle_focus();
            state.load_selected_body(repo)?;
            return Ok(false);
        }

        KeyCode::Char('r') if key.modifiers.is_empty() => {
            state.page_next(repo)?;
            return Ok(false);
        }

        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            // Shift+r => 'R'
            state.page_prev(repo)?;
            return Ok(false);
        }

        KeyCode::Char('R') => {
            state.page_prev(repo)?;
            return Ok(false);
        }

        _ => {}
    }

    match state.focus {
        Focus::List => handle_list_keys(key, state, repo),
        Focus::Body => handle_body_keys(key, state),
    }
}

fn handle_list_keys(
    key: KeyEvent,
    state: &mut AppState,
    repo: &dyn MailRepository,
) -> Result<bool> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_selection(1);
            state.load_selected_body(repo)?;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_selection(-1);
            state.load_selected_body(repo)?;
        }
        KeyCode::Home => {
            state.list_state.select(Some(0));
            state.selected_id = state.current_selected_id();
            state.body_scroll = 0;
            state.load_selected_body(repo)?;
        }
        KeyCode::End => {
            if !state.items.is_empty() {
                state.list_state.select(Some(state.items.len() - 1));
                state.selected_id = state.current_selected_id();
                state.body_scroll = 0;
                state.load_selected_body(repo)?;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_body_keys(key: KeyEvent, state: &mut AppState) -> Result<bool> {
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
