// Copyright (c) Meta Platforms, Inc. and affiliates.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::app::{App, AppResult, ProcessViewState, SelectedTab};
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
        KeyCode::Right => app.next_tab(),
        KeyCode::Left => app.prev_tab(),

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

        KeyCode::Char('s') | KeyCode::Char('S') => {
            if app.selected_tab == SelectedTab::Process {
                app.process_view_state = ProcessViewState::Stack;
            }
        }

        KeyCode::Char('h') | KeyCode::Char('H') => {
            if app.selected_tab == SelectedTab::Process {
                app.process_view_state = ProcessViewState::Heap;
            }
        }

        KeyCode::Char('m') | KeyCode::Char('M') => {
            if app.selected_tab == SelectedTab::Process {
                app.process_view_state = ProcessViewState::MessageQueue;
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
