use anyhow::Result;
use ratatui::widgets::ListState;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::store::repo::MailRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Body,
}

pub struct AppState {
    pub page: u32,
    pub page_size: u32,

    pub items: Vec<EmailSummary>,
    pub list_state: ListState,

    pub selected_id: Option<EmailId>,
    pub body: Option<EmailBody>,
    pub body_scroll: u16,

    pub focus: Focus,
}

impl AppState {
    pub fn new() -> Self {
        let mut s = Self {
            page: 0,
            page_size: 20,
            items: vec![],
            list_state: ListState::default(),
            selected_id: None,
            body: None,
            body_scroll: 0,
            focus: Focus::List,
        };
        s.list_state.select(Some(0));
        s
    }

    pub fn reload_page(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.items = repo.list_page(self.page, self.page_size)?;
        if self.items.is_empty() {
            self.list_state.select(None);
            self.selected_id = None;
        } else {
            // keep existing selection if possible
            if self.list_state.selected().is_none() {
                self.list_state.select(Some(0));
            }
            self.selected_id = self.current_selected_id();
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
            self.selected_id = Some(id);
            self.body_scroll = 0;
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            self.list_state.select(None);
            self.selected_id = None;
            return;
        }

        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let len = self.items.len() as i32;
        let next = (cur + delta).clamp(0, len - 1) as usize;

        self.list_state.select(Some(next));
        self.selected_id = self.current_selected_id();
        self.body_scroll = 0;
    }

    pub fn load_selected_body(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.body = None;
        if let Some(id) = self.current_selected_id() {
            self.selected_id = Some(id);
            self.body = repo.get_body(id)?;
        }
        Ok(())
    }

    pub fn page_next(&mut self, repo: &dyn MailRepository) -> Result<()> {
        self.page = self.page.saturating_add(1);
        self.reload_page(repo)?;
        if !self.items.is_empty() {
            self.list_state.select(Some(0));
            self.selected_id = self.current_selected_id();
        }
        self.body_scroll = 0;
        self.load_selected_body(repo)?;
        Ok(())
    }

    pub fn page_prev(&mut self, repo: &dyn MailRepository) -> Result<()> {
        if self.page == 0 {
            return Ok(());
        }
        self.page -= 1;
        self.reload_page(repo)?;
        if !self.items.is_empty() {
            // nice UX: land at bottom item when going back
            let last = self.items.len().saturating_sub(1);
            self.list_state.select(Some(last));
            self.selected_id = self.current_selected_id();
        }
        self.body_scroll = 0;
        self.load_selected_body(repo)?;
        Ok(())
    }

    pub fn scroll_body(&mut self, delta: i32) {
        if delta < 0 {
            let d = (-delta) as u16;
            self.body_scroll = self.body_scroll.saturating_sub(d);
        } else {
            self.body_scroll = self.body_scroll.saturating_add(delta as u16);
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::List => Focus::Body,
            Focus::Body => Focus::List,
        };
    }
}
