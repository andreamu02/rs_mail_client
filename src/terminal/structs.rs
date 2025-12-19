use ratatui::widgets::{List, ListState};

#[derive(Debug, Default)]
pub struct AppState {
    pub items: Vec<TodoItem>,
    pub list_state: ListState,
    pub is_add_new: bool,
    pub input_value: String,
}

impl AppState {
    pub fn default() -> Self {
        AppState {
            items: Vec::<TodoItem>::default(),
            list_state: ListState::default(),
            is_add_new: false,
            input_value: String::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct TodoItem {
    pub is_done: bool,
    pub description: String,
}

pub enum FormAction {
    None,
    Submit,
    Escape,
}
