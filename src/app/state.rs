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

use crate::config::CommonColors;
use crate::parser::*;
use ratatui::{
    text::Text,
    widgets::{ListState, Table, TableState},
};
use std::collections::{HashMap, HashSet};
use std::error;
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, FromRepr};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

/// Analysis category for Master-Detail view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisCategory {
    ProcessInfo,
    Stack,
    Messages,
    Heap,
}

impl AnalysisCategory {
    pub fn as_str(&self) -> &str {
        match self {
            AnalysisCategory::ProcessInfo => "Process Info",
            AnalysisCategory::Stack => "Stack",
            AnalysisCategory::Messages => "Messages",
            AnalysisCategory::Heap => "Heap",
        }
    }

    pub fn all() -> Vec<AnalysisCategory> {
        vec![
            AnalysisCategory::ProcessInfo,
            AnalysisCategory::Stack,
            AnalysisCategory::Messages,
            AnalysisCategory::Heap,
        ]
    }
}

/// Stack frame data for interactive navigation
#[derive(Debug, Clone)]
pub struct StackFrameData {
    pub index: usize,
    pub address: String,
    pub return_addr: String,
    pub function: String,
    pub module: String,
    pub offset: usize,
    pub arity: usize,
    pub variables: Vec<(String, String)>, // (raw, decoded)
}

/// Message data for interactive navigation
#[derive(Debug, Clone)]
pub struct MessageData {
    pub index: usize,
    pub address: String,
    pub value_raw: String,
    pub value_decoded: String,
}

/// Process suggestion data for the analysis prompt
#[derive(Debug, Clone)]
pub struct ProcessSuggestion {
    pub pid: String,
    pub name: String,
    pub memory: i64,
    pub stack_heap: i64,
    pub bin_vheap: i64,
    pub message_queue_length: i64,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppState {
    #[default]
    Running,
    Searching,
    AnalysisPrompt,
    AnalysisStackDetail,  // Viewing detailed stack frame variables
    AnalysisMessageDetail,  // Viewing detailed message content
    Quitting,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ProcessViewState {
    Heap,
    #[default]
    Stack,
    MessageQueue,
}

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter, PartialEq, Eq, Hash, Debug)]
pub enum SelectedTab {
    #[default]
    #[strum(to_string = "General Information")]
    General,
    #[strum(to_string = "Process Group Info")]
    ProcessGroup,
    #[strum(to_string = "Process Details")]
    Process,
    #[strum(to_string = "Memory Analysis")]
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortColumn {
    Pid = 0,
    Name = 1,
    MessageQueueLength = 2,
    Memory = 3,
    TotalBinVHeap = 4,
    BinVHeap = 5,
    BinVHeapUnused = 6,
    OldBinVHeap = 7,
    OldBinVHeapUnused = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessGroupSortColumn {
    Pid = 0,
    Name = 1,
    TotalMemorySize = 2,
    ChildrenCount = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Application.
pub struct App<'a> {
    /// header
    pub header: String,
    pub state: AppState,
    pub selected_tab: SelectedTab,

    /// parser
    pub parser: parser::CDParser,
    pub filepath: String,
    pub crash_dump: types::CrashDump,
    pub index_map: IndexMap,
    pub ancestor_map: HashMap<String, Vec<String>>,

    /// process information list
    pub tab_lists: HashMap<SelectedTab, Vec<String>>,
    pub tab_rows: HashMap<SelectedTab, Vec<ratatui::widgets::Row<'a>>>,

    pub inspecting_pid: String,
    pub inspect_scroll: u16,
    pub fullscreen_mode: bool,

    pub table_states: HashMap<SelectedTab, TableState>,

    pub process_group_table: Table<'a>,
    pub process_group_sort_column: ProcessGroupSortColumn,
    pub process_group_sort_direction: SortDirection,

    pub process_view_table: Table<'a>,
    pub process_view_state: ProcessViewState,
    pub process_sort_column: ProcessSortColumn,
    pub process_sort_direction: SortDirection,
    pub process_readonly_view: Option<dashmap::ReadOnlyView<String, InfoOrIndex<ProcInfo>>>,

    pub footer_text: HashMap<SelectedTab, String>,

    pub colors: CommonColors,
    pub show_help: bool,

    // Search state
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_match_index: usize,

    // Analysis state
    pub analysis_text: String,
    pub analysis_scroll: u16,
    pub analysis_prompt_input: String,
    pub analysis_process_suggestions: Vec<ProcessSuggestion>,
    pub analysis_suggestion_selected: usize, // Index of selected suggestion

    // Master-Detail analysis state
    pub analysis_category: AnalysisCategory,
    pub analysis_category_state: ListState,
    pub stack_frames: Vec<StackFrameData>,
    pub messages: Vec<MessageData>,
    pub heap_loaded_pids: HashSet<String>, // Track which PIDs have loaded heap data
    pub heap_text: Text<'a>,
    pub selected_stack_idx: usize,
    pub selected_message_idx: usize,
    pub analysis_pid: String,
    pub analysis_proc_info: Option<types::ProcInfo>,
    pub analysis_navigation_history: Vec<String>, // PID history for back navigation
}

impl Default for App<'_> {
    fn default() -> Self {
        Self {
            state: AppState::Running,
            selected_tab: SelectedTab::General,
            parser: parser::CDParser::default(),
            filepath: "".to_string(),
            crash_dump: types::CrashDump::new(),
            index_map: IndexMap::new(),
            ancestor_map: HashMap::new(),
            header: "ERL CRASH DUMP VIEWER".to_string(),
            tab_lists: HashMap::from_iter(SelectedTab::iter().map(|tab| (tab, vec![]))),
            tab_rows: HashMap::from_iter(SelectedTab::iter().map(|tab| (tab, vec![]))),
            table_states: HashMap::from_iter(
                SelectedTab::iter().map(|tab| (tab, TableState::default())),
            ),
            process_group_table: Table::default(),
            process_group_sort_column: ProcessGroupSortColumn::TotalMemorySize,
            process_group_sort_direction: SortDirection::Descending,

            process_view_state: ProcessViewState::default(),
            process_view_table: Table::default(),
            process_sort_column: ProcessSortColumn::BinVHeap,
            process_sort_direction: SortDirection::Descending,
            process_readonly_view: None,
            footer_text: HashMap::new(),
            inspecting_pid: "".to_string(),
            inspect_scroll: 0,
            fullscreen_mode: false,
            colors: CommonColors::default(),
            show_help: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_match_index: 0,
            analysis_text: String::new(),
            analysis_scroll: 0,
            analysis_prompt_input: String::new(),
            analysis_process_suggestions: Vec::new(),
            analysis_suggestion_selected: 0,

            // Master-Detail analysis state
            analysis_category: AnalysisCategory::ProcessInfo,
            analysis_category_state: ListState::default(),
            stack_frames: Vec::new(),
            messages: Vec::new(),
            heap_loaded_pids: HashSet::new(),
            heap_text: Text::default(),
            selected_stack_idx: 0,
            selected_message_idx: 0,
            analysis_pid: String::new(),
            analysis_proc_info: None,
            analysis_navigation_history: Vec::new(),
        }
    }
}
