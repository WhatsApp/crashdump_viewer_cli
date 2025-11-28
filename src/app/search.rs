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

use super::state::{App, AppState, SelectedTab};
use crate::parser::types::InfoOrIndex;

impl App<'_> {
    pub fn start_search(&mut self) {
        self.state = AppState::Searching;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
    }

    pub fn cancel_search(&mut self) {
        self.state = AppState::Running;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
    }

    pub fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_match_index = 0;

        if self.search_query.is_empty() {
            self.state = AppState::Running;
            return;
        }

        // Try to compile the regex
        let re = match regex::Regex::new(&self.search_query) {
            Ok(r) => r,
            Err(_) => {
                // Invalid regex, just cancel
                self.state = AppState::Running;
                return;
            }
        };

        // Search in the current tab's list
        let list = match self.selected_tab {
            SelectedTab::Process | SelectedTab::ProcessGroup => {
                &self.tab_lists[&self.selected_tab]
            }
            _ => {
                self.state = AppState::Running;
                return;
            }
        };

        // Find matches in PIDs and process names
        for (idx, pid) in list.iter().enumerate() {
            // Check if PID matches
            if re.is_match(pid) {
                self.search_matches.push(idx);
                continue;
            }

            // Check if process name matches
            if let Some(process) = self.crash_dump.processes.get(pid) {
                if let InfoOrIndex::Info(ref proc_info) = *process.value() {
                    if let Some(ref name) = proc_info.name {
                        if re.is_match(name) {
                            self.search_matches.push(idx);
                        }
                    }
                }
            }
        }

        // If we have matches, jump to the first one
        if !self.search_matches.is_empty() {
            if let Some(table_state) = self.table_states.get_mut(&self.selected_tab) {
                table_state.select(Some(self.search_matches[0]));
            }
        }

        self.state = AppState::Running;
    }

    pub fn next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }

        self.search_match_index = (self.search_match_index + 1) % self.search_matches.len();
        let target_idx = self.search_matches[self.search_match_index];

        if let Some(table_state) = self.table_states.get_mut(&self.selected_tab) {
            table_state.select(Some(target_idx));
        }
    }

    pub fn prev_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }

        if self.search_match_index == 0 {
            self.search_match_index = self.search_matches.len() - 1;
        } else {
            self.search_match_index -= 1;
        }

        let target_idx = self.search_matches[self.search_match_index];

        if let Some(table_state) = self.table_states.get_mut(&self.selected_tab) {
            table_state.select(Some(target_idx));
        }
    }
}
