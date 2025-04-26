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

use crate::app::{App, AppResult, ProcessSortColumn, ProcessGroupSortColumn, ProcessViewState, SelectedTab, SortDirection};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key_events(key_event: KeyEvent, app: &mut App) -> AppResult<()> {
    match app.selected_tab {
        SelectedTab::Inspect => match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.quit();
            }
            KeyCode::Char('c') | KeyCode::Char('C')
                if key_event.modifiers == KeyModifiers::CONTROL =>
            {
                app.quit();
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                app.selected_tab = SelectedTab::Process;
            }
            KeyCode::Right => app.next_tab(),
            KeyCode::Left => app.prev_tab(),
            KeyCode::Char('j') | KeyCode::Down => app.inspect_scroll_state.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => app.inspect_scroll_state.scroll_up(),
            KeyCode::Char('f') | KeyCode::PageDown => app.inspect_scroll_state.scroll_page_down(),
            KeyCode::Char('b') | KeyCode::PageUp => app.inspect_scroll_state.scroll_page_up(),
            KeyCode::Char('g') | KeyCode::Home => app.inspect_scroll_state.scroll_to_top(),
            KeyCode::Char('G') | KeyCode::End => app.inspect_scroll_state.scroll_to_bottom(),
            KeyCode::Char('?') => {
                app.show_help = !app.show_help;
            }
            _ => {}
        },
        SelectedTab::ProcessGroup => {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => app.quit(),
                KeyCode::Char('c') | KeyCode::Char('C')
                    if key_event.modifiers == KeyModifiers::CONTROL =>
                {
                    app.quit()
                }
                KeyCode::Right => app.next_tab(),
                KeyCode::Left => app.prev_tab(),

                KeyCode::Down => {
                    // Get the list first
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        // Then get the mutable state
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len(); // Use the list directly
                            if amount_items == 0 {
                                // Handle empty list case
                                table_state.select(None);
                            } else if let Some(selected) = table_state.selected() {
                                if selected < amount_items - 1 {
                                    table_state.select(Some(selected + 1));
                                } else {
                                    table_state.select(Some(0)); // Wrap to top
                                }
                            } else {
                                // Nothing selected, select the first item
                                table_state.select(Some(0));
                            }
                        }
                    }
                }
                KeyCode::Up => {
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len();
                            if amount_items == 0 {

                            } else if let Some(selected) = table_state.selected() {
                                if selected > 0 {
                                    table_state.select(Some(selected - 1));
                                } else {
                                    table_state.select(Some(amount_items - 1));
                                }
                            } else {
                                table_state.select(Some(amount_items - 1));
                            }
                        }
                    }
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Char(c) if key_event.modifiers == KeyModifiers::SHIFT => {
                    let mut new_sort_column: Option<ProcessGroupSortColumn> = None;
                    match c {
                        'T' => new_sort_column = Some(ProcessGroupSortColumn::TotalMemorySize),
                        'P' => new_sort_column = Some(ProcessGroupSortColumn::Pid),
                        'N' => new_sort_column = Some(ProcessGroupSortColumn::Name),
                        'C' => new_sort_column = Some(ProcessGroupSortColumn::ChildrenCount),
                        _ => {} // Ignore other Shift+key combinations
                    }
                    if let Some(sort_col) = new_sort_column {
                        if app.process_group_sort_column == sort_col {
                            app.process_group_sort_direction = match app.process_group_sort_direction {
                                SortDirection::Ascending => SortDirection::Descending,
                                SortDirection::Descending => SortDirection::Ascending,
                            };
                        } else {
                            app.process_group_sort_column = sort_col;
                            match sort_col {
                                ProcessGroupSortColumn::Pid | ProcessGroupSortColumn::Name => {
                                    app.process_group_sort_direction = SortDirection::Ascending;
                                }
                                _ => {
                                    // Numerical columns
                                    app.process_group_sort_direction = SortDirection::Descending;
                                }
                            }
                        }
                        app.sort_and_update_process_group_table();
                    }
                }


                _ => {}
            }
        }
        SelectedTab::Process => {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => app.quit(),
                KeyCode::Char('c') | KeyCode::Char('C')
                    if key_event.modifiers == KeyModifiers::CONTROL =>
                {
                    app.quit()
                }
                KeyCode::Right => app.next_tab(),
                KeyCode::Left => app.prev_tab(),
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(table_state) = app.table_states.get_mut(&SelectedTab::Process) {
                        if let Some(selected) = table_state.selected() {
                            let amount_items = app.tab_lists[&SelectedTab::Process].len();
                            if selected < amount_items.saturating_sub(1) {
                                table_state.select(Some(selected + 1));
                            } else if amount_items > 0 {
                                table_state.select(Some(0));
                            }
                        } else if !app.tab_lists[&SelectedTab::Process].is_empty() {
                            table_state.select(Some(0));
                        }
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(table_state) = app.table_states.get_mut(&SelectedTab::Process) {
                        if let Some(selected) = table_state.selected() {
                            let amount_items = app.tab_lists[&SelectedTab::Process].len();
                            if selected > 0 {
                                table_state.select(Some(selected - 1));
                            } else if amount_items > 0 {
                                table_state.select(Some(amount_items - 1));
                            }
                        } else if !app.tab_lists[&SelectedTab::Process].is_empty() {
                            let amount_items = app.tab_lists[&SelectedTab::Process].len();
                            table_state.select(Some(amount_items - 1));
                        }
                    }
                }
                KeyCode::Char('s') => app.process_view_state = ProcessViewState::Stack,
                KeyCode::Char('h') => app.process_view_state = ProcessViewState::Heap,
                KeyCode::Char('m') => app.process_view_state = ProcessViewState::MessageQueue,
                KeyCode::Char('i') | KeyCode::Char('I') => app.selected_tab = SelectedTab::Inspect,
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Char(c) if key_event.modifiers == KeyModifiers::SHIFT => {
                    let mut new_sort_column: Option<ProcessSortColumn> = None;
                    match c {
                        'Q' => new_sort_column = Some(ProcessSortColumn::MessageQueueLength),
                        'M' => new_sort_column = Some(ProcessSortColumn::Memory),
                        'T' => new_sort_column = Some(ProcessSortColumn::TotalBinVHeap),
                        'B' => new_sort_column = Some(ProcessSortColumn::BinVHeap),
                        'P' => new_sort_column = Some(ProcessSortColumn::Pid),
                        'N' => new_sort_column = Some(ProcessSortColumn::Name),
                        'U' => new_sort_column = Some(ProcessSortColumn::BinVHeapUnused),
                        'O' => new_sort_column = Some(ProcessSortColumn::OldBinVHeap),
                        'V' => new_sort_column = Some(ProcessSortColumn::OldBinVHeapUnused),
                        _ => {} // Ignore other Shift+key combinations
                    }

                    if let Some(sort_col) = new_sort_column {
                        if app.process_sort_column == sort_col {
                            app.process_sort_direction = match app.process_sort_direction {
                                SortDirection::Ascending => SortDirection::Descending,
                                SortDirection::Descending => SortDirection::Ascending,
                            };
                        } else {
                            app.process_sort_column = sort_col;
                            match sort_col {
                                ProcessSortColumn::Pid | ProcessSortColumn::Name => {
                                    app.process_sort_direction = SortDirection::Ascending;
                                }
                                _ => {
                                    // Numerical columns
                                    app.process_sort_direction = SortDirection::Descending;
                                }
                            }
                        }
                        app.sort_and_update_process_table();
                    }
                }

                _ => {} // Ignore other keys
            }
        }
        _ => {
            // Handling for other tabs (General, ProcessGroup, etc.)
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => app.quit(),
                KeyCode::Char('c') | KeyCode::Char('C')
                    if key_event.modifiers == KeyModifiers::CONTROL =>
                {
                    app.quit()
                }
                KeyCode::Right => app.next_tab(),
                KeyCode::Left => app.prev_tab(),

                KeyCode::Down => {
                    // Get the list first
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        // Then get the mutable state
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len(); // Use the list directly
                            if amount_items == 0 {
                                // Handle empty list case
                                table_state.select(None);
                            } else if let Some(selected) = table_state.selected() {
                                if selected < amount_items - 1 {
                                    table_state.select(Some(selected + 1));
                                } else {
                                    table_state.select(Some(0)); // Wrap to top
                                }
                            } else {
                                // Nothing selected, select the first item
                                table_state.select(Some(0));
                            }
                        }
                    }
                }
                KeyCode::Up => {
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len();
                            if amount_items == 0 {

                            } else if let Some(selected) = table_state.selected() {
                                if selected > 0 {
                                    table_state.select(Some(selected - 1));
                                } else {
                                    table_state.select(Some(amount_items - 1));
                                }
                            } else {
                                table_state.select(Some(amount_items - 1));
                            }
                        }
                    }
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
