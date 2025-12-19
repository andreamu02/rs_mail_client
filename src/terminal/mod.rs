pub mod events;
pub mod state;
pub mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};

use crate::store::repo::MailRepository;
use crate::terminal::events::handle_key;
use crate::terminal::state::AppState;
use crate::terminal::ui::render;

pub fn run_tui(repo: &dyn MailRepository, open_id: Option<u32>) -> Result<()> {
    let mut state = AppState::new();
    state.reload_page(repo)?;

    if let Some(id) = open_id {
        state.try_select_id(id);
        state.load_selected_body(repo)?;
    } else {
        state.load_selected_body(repo)?;
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = (|| -> Result<()> {
        loop {
            terminal.draw(|f| render(f, &state))?;

            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => {
                        if handle_key(k, &mut state, repo)? {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}
