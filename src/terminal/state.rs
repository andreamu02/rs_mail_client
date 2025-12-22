use anyhow::Result;
use ratatui::widgets::ListState;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::store::repo::MailRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Body,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    ListOnly = 0,
    Split = 1,
    Menu = 2,
    Help = 3,
}

pub struct AppState {
    pub page: u32,
    pub page_size: u32,

    pub items: Vec<EmailSummary>,
    pub list_state: ListState,

    /// The email currently opened in the right panel (only when Split)
    pub opened_id: Option<EmailId>,
    pub body: Option<EmailBody>,
    pub body_scroll: u16,

    pub focus: Focus,
    pub mode: ViewMode,
    pub previous_focus: Option<Focus>,
    pub previous: Option<ViewMode>,
}

impl AppState {
    pub fn new() -> Self {
        let mut s = Self {
            page: 0,
            page_size: 20,
            items: vec![],
            list_state: ListState::default(),
            opened_id: None,
            body: None,
            body_scroll: 0,
            focus: Focus::List,
            mode: ViewMode::ListOnly,
            previous: None,
            previous_focus: None,
        };
        s.list_state.select(Some(0));
        s
    }

    pub fn reload_page(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.items = repo.list_page(self.page, self.page_size)?;
        if self.items.is_empty() {
            self.list_state.select(None);
        } else if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        Ok(())
    }

    pub fn current_selected_id(&self) -> Option<EmailId> {
        let idx = self.list_state.selected()?;
        self.items.get(idx).map(|e| e.id)
    }

    pub fn try_select_id(&mut self, id: EmailId) {
        if let Some(pos) = self.items.iter().position(|x| x.id == id) {
            self.list_state.select(Some(pos));
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            self.list_state.select(None);
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let len = self.items.len() as i32;
        let next = (cur + delta).clamp(0, len - 1) as usize;
        self.list_state.select(Some(next));
        // IMPORTANT: do NOT load body here (you only want to open on Enter)
    }

    pub fn open_selected(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.mode = ViewMode::Split;
        self.focus = Focus::Body;
        self.body_scroll = 0;

        self.opened_id = self.current_selected_id();
        self.body = None;

        if let Some(id) = self.opened_id {
            self.body = repo.get_body(id)?;
        }
        Ok(())
    }

    pub fn open_uid(&mut self, repo: &dyn MailRepository, id: EmailId) -> Result<()> {
        // Used by notification click: open directly
        self.mode = ViewMode::Split;
        self.focus = Focus::Body;
        self.body_scroll = 0;

        self.opened_id = Some(id);
        self.body = repo.get_body(id)?;

        // Try highlight it in list if present
        self.try_select_id(id);

        Ok(())
    }

    pub fn close_email(&mut self) {
        self.mode = ViewMode::ListOnly;
        self.focus = Focus::List;
        self.opened_id = None;
        self.body = None;
        self.body_scroll = 0;
    }

    pub fn toggle_focus(&mut self) {
        if self.mode == ViewMode::Help {
            self.focus = Focus::Help;
            return;
        }
        if self.mode != ViewMode::Split {
            return;
        }
        self.focus = match self.focus {
            Focus::List => Focus::Body,
            Focus::Body => Focus::List,
            _ => self.focus,
        };
    }

    pub fn scroll_body(&mut self, delta: i32) {
        if self.mode != ViewMode::Split {
            return;
        }
        if delta < 0 {
            self.body_scroll = self.body_scroll.saturating_sub((-delta) as u16);
        } else {
            self.body_scroll = self.body_scroll.saturating_add(delta as u16);
        }
    }

    pub fn page_next(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.page = self.page.saturating_add(1);
        self.reload_page(repo)?;
        if !self.items.is_empty() {
            self.list_state.select(Some(0));
        }
        Ok(())
    }

    pub fn page_prev(&mut self, repo: &dyn MailRepository) -> Result<()> {
        if self.page == 0 {
            return Ok(());
        }
        self.page -= 1;
        self.reload_page(repo)?;
        if !self.items.is_empty() {
            self.list_state.select(Some(self.items.len() - 1));
        }
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
