use crate::app::{App, AppResult};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handles the key events and updates the state of [`App`].
pub fn handle_key_events(key_event: KeyEvent, app: &mut App) -> AppResult<()> {
    match key_event.code {
        // Exit application on `ESC` or `q`
        KeyCode::Esc | KeyCode::Char('q') => {
            app.quit();
        }
        // Exit application on `Ctrl-C`
        KeyCode::Char('c') | KeyCode::Char('C') => {
            if key_event.modifiers == KeyModifiers::CONTROL {
                app.quit();
            }
        }
        // Tab switching
        KeyCode::Char('l') | KeyCode::Right => app.next_tab(),
        KeyCode::Char('h') | KeyCode::Left => app.prev_tab(),

        KeyCode::Down => {
            // Get the ListState for the currently selected tab
            if let Some(list_state) = app.list_states.get_mut(&app.selected_tab) {
                if let Some(selected) = list_state.selected() {
                    let amount_items = app.tab_lists[&app.selected_tab].len();
                    if selected >= amount_items - 1 {
                        list_state.select(Some(0));
                    } else {
                        list_state.select(Some(selected + 1));
                    }
                }
            }
        }

        KeyCode::Up => {
            // Get the ListState for the currently selected tab
            if let Some(list_state) = app.list_states.get_mut(&app.selected_tab) {
                if let Some(selected) = list_state.selected() {
                    let amount_items = app.tab_lists[&app.selected_tab].len();
                    if selected > 0 {
                        list_state.select(Some(selected - 1));
                    } else {
                        list_state.select(Some(amount_items - 1));
                    }
                }
            }
        }

        // Other handlers you could add here.
        _ => {}
    }
    Ok(())
}
