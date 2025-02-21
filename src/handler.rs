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
            if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                if let Some(selected) = table_state.selected() {
                    let amount_items = app.tab_lists[&app.selected_tab].len();
                    if selected >= amount_items - 1 {
                        table_state.select(Some(0));
                    } else {
                        table_state.select(Some(selected + 1));
                    }
                }
            }
        }

        KeyCode::Up => {
            if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                if let Some(selected) = table_state.selected() {
                    let amount_items = app.tab_lists[&app.selected_tab].len();
                    if selected > 0 {
                        table_state.select(Some(selected - 1));
                    } else {
                        table_state.select(Some(amount_items - 1));
                    }
                }
            }
        }

        // Other handlers you could add here.
        _ => {}
    }
    Ok(())
}
