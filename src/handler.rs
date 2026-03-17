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

use crate::app::{AnalysisCategory, App, AppResult, AppState, ProcessSortColumn, ProcessGroupSortColumn, ProcessViewState, SelectedTab, SortDirection};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key_events(key_event: KeyEvent, app: &mut App) -> AppResult<()> {
    // Handle search mode separately
    if app.state == AppState::Searching {
        match key_event.code {
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Enter => {
                app.execute_search();
            }
            KeyCode::Esc => {
                app.cancel_search();
            }
            _ => {}
        }
        return Ok(());
    }

    // Handle analysis prompt mode
    if app.state == AppState::AnalysisPrompt {
        match key_event.code {
            KeyCode::Char(c) => {
                app.analysis_prompt_input.push(c);
                // Reset selection to top when typing
                app.analysis_suggestion_selected = 0;
            }
            KeyCode::Backspace => {
                app.analysis_prompt_input.pop();
                // Reset selection to top when deleting
                app.analysis_suggestion_selected = 0;
            }
            KeyCode::Tab => {
                // Tab autocomplete - fill in the first matching PID
                let input_lower = app.analysis_prompt_input.to_lowercase();
                if let Some(suggestion) = app.analysis_process_suggestions
                    .iter()
                    .find(|s| {
                        s.pid.to_lowercase().contains(&input_lower)
                            || s.name.to_lowercase().contains(&input_lower)
                    })
                {
                    app.analysis_prompt_input = suggestion.pid.clone();
                }
            }
            KeyCode::Up => {
                // Navigate up in suggestions
                if app.analysis_suggestion_selected > 0 {
                    app.analysis_suggestion_selected -= 1;
                }
            }
            KeyCode::Down => {
                // Navigate down in suggestions
                let input_lower = app.analysis_prompt_input.to_lowercase();
                let filtered_count = if input_lower.is_empty() {
                    app.analysis_process_suggestions.len().min(15)
                } else {
                    app.analysis_process_suggestions
                        .iter()
                        .filter(|s| {
                            s.pid.to_lowercase().contains(&input_lower)
                                || s.name.to_lowercase().contains(&input_lower)
                        })
                        .take(15)
                        .count()
                };

                if app.analysis_suggestion_selected + 1 < filtered_count {
                    app.analysis_suggestion_selected += 1;
                }
            }
            KeyCode::Enter => {
                app.execute_process_analysis_prompt();
            }
            KeyCode::Esc => {
                app.cancel_process_analysis_prompt();
            }
            _ => {}
        }
        return Ok(());
    }

    // Handle stack detail view
    if app.state == AppState::AnalysisStackDetail {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Backspace => {
                app.state = AppState::Running;
            }
            _ => {}
        }
        return Ok(());
    }

    // Handle message detail view
    if app.state == AppState::AnalysisMessageDetail {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Backspace => {
                app.state = AppState::Running;
            }
            _ => {}
        }
        return Ok(());
    }

    match app.selected_tab {
        SelectedTab::Memory => match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.quit();
            }
            KeyCode::Char('c') | KeyCode::Char('C')
                if key_event.modifiers == KeyModifiers::CONTROL =>
            {
                app.quit();
            }
            KeyCode::Right => app.next_tab(),
            KeyCode::Left => app.prev_tab(),

            // Scrolling for legacy text-based analysis (when analysis_text is not empty)
            KeyCode::Char('j') | KeyCode::Down if !app.analysis_text.is_empty() => {
                app.analysis_scroll = app.analysis_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up if !app.analysis_text.is_empty() => {
                app.analysis_scroll = app.analysis_scroll.saturating_sub(1);
            }
            KeyCode::Char('f') | KeyCode::PageDown if !app.analysis_text.is_empty() => {
                app.analysis_scroll = app.analysis_scroll.saturating_add(10);
            }
            KeyCode::Char('b') | KeyCode::PageUp if !app.analysis_text.is_empty() => {
                app.analysis_scroll = app.analysis_scroll.saturating_sub(10);
            }
            KeyCode::Char('g') | KeyCode::Home if !app.analysis_text.is_empty() => {
                app.analysis_scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::End if !app.analysis_text.is_empty() => {
                app.analysis_scroll = 10000;
            }

            // Up/Down: Navigate between categories (only when NOT showing legacy text)
            KeyCode::Up if app.analysis_text.is_empty() => {
                if let Some(selected) = app.analysis_category_state.selected() {
                    if selected > 0 {
                        app.analysis_category_state.select(Some(selected - 1));
                        app.analysis_category = match selected - 1 {
                            0 => AnalysisCategory::ProcessInfo,
                            1 => AnalysisCategory::Stack,
                            2 => AnalysisCategory::Messages,
                            3 => AnalysisCategory::Heap,
                            _ => AnalysisCategory::ProcessInfo,
                        };
                    }
                }
            }
            KeyCode::Down if app.analysis_text.is_empty() => {
                if let Some(selected) = app.analysis_category_state.selected() {
                    if selected < 3 {
                        app.analysis_category_state.select(Some(selected + 1));
                        app.analysis_category = match selected + 1 {
                            0 => AnalysisCategory::ProcessInfo,
                            1 => AnalysisCategory::Stack,
                            2 => AnalysisCategory::Messages,
                            3 => AnalysisCategory::Heap,
                            _ => AnalysisCategory::ProcessInfo,
                        };
                    }
                }
            }

            // j/k: Navigate within items (stack frames or messages) - only when NOT showing legacy text
            KeyCode::Char('j') if app.analysis_text.is_empty() => {
                match app.analysis_category {
                    AnalysisCategory::Stack => {
                        if !app.stack_frames.is_empty() {
                            app.selected_stack_idx = (app.selected_stack_idx + 1)
                                .min(app.stack_frames.len() - 1);
                        }
                    }
                    AnalysisCategory::Messages => {
                        if !app.messages.is_empty() {
                            app.selected_message_idx = (app.selected_message_idx + 1)
                                .min(app.messages.len() - 1);
                        }
                    }
                    AnalysisCategory::Heap => {
                        app.analysis_scroll = app.analysis_scroll.saturating_add(1);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('k') if app.analysis_text.is_empty() => {
                match app.analysis_category {
                    AnalysisCategory::Stack => {
                        app.selected_stack_idx = app.selected_stack_idx.saturating_sub(1);
                    }
                    AnalysisCategory::Messages => {
                        app.selected_message_idx = app.selected_message_idx.saturating_sub(1);
                    }
                    AnalysisCategory::Heap => {
                        app.analysis_scroll = app.analysis_scroll.saturating_sub(1);
                    }
                    _ => {}
                }
            }

            // PageDown/PageUp: Fast scrolling for heap view
            KeyCode::PageDown if app.analysis_text.is_empty() && app.analysis_category == AnalysisCategory::Heap => {
                app.analysis_scroll = app.analysis_scroll.saturating_add(20);
            }
            KeyCode::PageUp if app.analysis_text.is_empty() && app.analysis_category == AnalysisCategory::Heap => {
                app.analysis_scroll = app.analysis_scroll.saturating_sub(20);
            }

            // Home/End: Jump to top/bottom of heap view
            KeyCode::Home if app.analysis_text.is_empty() && app.analysis_category == AnalysisCategory::Heap => {
                app.analysis_scroll = 0;
            }
            KeyCode::End if app.analysis_text.is_empty() && app.analysis_category == AnalysisCategory::Heap => {
                app.analysis_scroll = app.heap_text.lines.len().saturating_sub(1) as u16;
            }

            // H: Load heap on demand
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.load_heap_for_analysis();
            }

            // Enter: Context-sensitive action
            KeyCode::Enter => {
                match app.analysis_category {
                    AnalysisCategory::Stack => {
                        // Show detailed view of selected stack frame with all variables
                        if !app.stack_frames.is_empty() && app.selected_stack_idx < app.stack_frames.len() {
                            app.state = AppState::AnalysisStackDetail;
                        }
                    }
                    AnalysisCategory::Messages => {
                        // Show detailed view of selected message
                        if !app.messages.is_empty() && app.selected_message_idx < app.messages.len() {
                            app.state = AppState::AnalysisMessageDetail;
                        }
                    }
                    AnalysisCategory::Heap => {
                        app.load_heap_for_analysis();
                    }
                    _ => {}
                }
            }

            // Backspace: Go back in navigation history
            KeyCode::Backspace => {
                if app.analysis_navigation_history.len() > 1 {
                    // Remove current PID
                    app.analysis_navigation_history.pop();
                    // Get previous PID
                    if let Some(prev_pid) = app.analysis_navigation_history.last() {
                        let pid = prev_pid.clone();
                        app.run_process_analysis(&pid);
                    }
                }
            }

            // B: Back to Process tab (browse mode)
            KeyCode::Char('b') | KeyCode::Char('B') if app.analysis_text.is_empty() => {
                app.selected_tab = SelectedTab::Process;
            }

            // Legacy commands (for backwards compatibility with old text-based analysis)
            KeyCode::Char('m') | KeyCode::Char('M') => {
                app.run_memory_analysis(10);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.start_process_analysis_prompt();
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.run_registered_analysis();
            }

            KeyCode::Char('?') => {
                app.show_help = !app.show_help;
            }
            _ => {}
        },
        SelectedTab::ProcessGroup => {
            match key_event.code {
                KeyCode::Char('q') => app.quit(),
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
                KeyCode::Enter | KeyCode::Char('p') | KeyCode::Char('P') => {
                    // Analyze the selected process (from process group) - switch to Memory tab
                    if let Some(table_state) = app.table_states.get(&SelectedTab::ProcessGroup) {
                        if let Some(selected) = table_state.selected() {
                            if let Some(list) = app.tab_lists.get(&SelectedTab::ProcessGroup) {
                                if selected < list.len() {
                                    let pid = list[selected].clone();
                                    app.run_process_analysis(&pid);
                                    app.selected_tab = SelectedTab::Memory;
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Char('/') => {
                    app.start_search();
                }
                KeyCode::Char('n') => {
                    app.next_search_match();
                }
                KeyCode::Char('N') => {
                    app.prev_search_match();
                }
                KeyCode::PageDown | KeyCode::Char('f') => {
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len();
                            if amount_items == 0 {
                                table_state.select(None);
                            } else if let Some(selected) = table_state.selected() {
                                let new_selected = (selected + 10).min(amount_items - 1);
                                table_state.select(Some(new_selected));
                            } else {
                                table_state.select(Some(0));
                            }
                        }
                    }
                }
                KeyCode::PageUp | KeyCode::Char('b') => {
                    if let Some(list) = app.tab_lists.get(&app.selected_tab) {
                        if let Some(table_state) = app.table_states.get_mut(&app.selected_tab) {
                            let amount_items = list.len();
                            if amount_items == 0 {
                                table_state.select(None);
                            } else if let Some(selected) = table_state.selected() {
                                let new_selected = selected.saturating_sub(10);
                                table_state.select(Some(new_selected));
                            } else {
                                table_state.select(Some(amount_items - 1));
                            }
                        }
                    }
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
                KeyCode::Char('q') => app.quit(),
                KeyCode::Char('c') | KeyCode::Char('C')
                    if key_event.modifiers == KeyModifiers::CONTROL =>
                {
                    app.quit()
                }
                KeyCode::Right => app.next_tab(),
                KeyCode::Left => app.prev_tab(),
                KeyCode::Down | KeyCode::Char('j') => {
                    if app.fullscreen_mode {
                        app.inspect_scroll = app.inspect_scroll.saturating_add(1);
                    } else {
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
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if app.fullscreen_mode {
                        app.inspect_scroll = app.inspect_scroll.saturating_sub(1);
                    } else {
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
                }
                KeyCode::Enter | KeyCode::Char('p') | KeyCode::Char('P') => {
                    // Analyze the selected process - switch to Memory tab
                    let pid = app.get_selected_pid();
                    if !pid.is_empty() {
                        app.run_process_analysis(&pid);
                        app.selected_tab = SelectedTab::Memory;
                    }
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    app.process_view_state = ProcessViewState::Stack;
                    app.inspect_scroll = 0;
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    app.process_view_state = ProcessViewState::Heap;
                    app.inspect_scroll = 0;
                }
                KeyCode::Char('m') | KeyCode::Char('M') => {
                    app.process_view_state = ProcessViewState::MessageQueue;
                    app.inspect_scroll = 0;
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    app.fullscreen_mode = !app.fullscreen_mode;
                    app.inspect_scroll = 0;
                }
                KeyCode::Char('/') => {
                    app.start_search();
                }
                KeyCode::Char('n') => {
                    app.next_search_match();
                }
                KeyCode::Char('N') => {
                    app.prev_search_match();
                }
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::PageDown if app.fullscreen_mode => {
                    app.inspect_scroll = app.inspect_scroll.saturating_add(20);
                }
                KeyCode::PageUp if app.fullscreen_mode => {
                    app.inspect_scroll = app.inspect_scroll.saturating_sub(20);
                }
                KeyCode::Home if app.fullscreen_mode => {
                    app.inspect_scroll = 0;
                }
                KeyCode::End if app.fullscreen_mode => {
                    app.inspect_scroll = 10000; // Large number to scroll to bottom
                }
                KeyCode::PageDown if !app.fullscreen_mode => {
                    if let Some(table_state) = app.table_states.get_mut(&SelectedTab::Process) {
                        let amount_items = app.tab_lists[&SelectedTab::Process].len();
                        if amount_items == 0 {
                            table_state.select(None);
                        } else if let Some(selected) = table_state.selected() {
                            let new_selected = (selected + 10).min(amount_items - 1);
                            table_state.select(Some(new_selected));
                        } else {
                            table_state.select(Some(0));
                        }
                    }
                }
                KeyCode::PageUp if !app.fullscreen_mode => {
                    if let Some(table_state) = app.table_states.get_mut(&SelectedTab::Process) {
                        let amount_items = app.tab_lists[&SelectedTab::Process].len();
                        if amount_items == 0 {
                            table_state.select(None);
                        } else if let Some(selected) = table_state.selected() {
                            let new_selected = selected.saturating_sub(10);
                            table_state.select(Some(new_selected));
                        } else {
                            table_state.select(Some(amount_items - 1));
                        }
                    }
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
                KeyCode::Char('q') => app.quit(),
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
