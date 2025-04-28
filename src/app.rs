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
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect, Size},
    style::{palette::tailwind, Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Cell, Clear, HighlightSpacing, Paragraph, Row, StatefulWidget, Table, TableState,
        Tabs, Widget, Wrap,
    },
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::error;
use std::io;
use std::time::Instant;
use tui_scrollview::{ScrollView, ScrollViewState};

use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, FromRepr};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

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
    pub tab_rows: HashMap<SelectedTab, Vec<Row<'a>>>,

    pub inspecting_pid: String,
    pub inspect_scroll_state: ScrollViewState,

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
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    #[default]
    Running,
    Quitting,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ProcessViewState {
    Heap,
    #[default]
    Stack,
    MessageQueue,
}

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter, PartialEq, Eq, Hash)]
pub enum SelectedTab {
    #[default]
    #[strum(to_string = "General Information")]
    General,
    #[strum(to_string = "Process Group Info")]
    ProcessGroup,
    #[strum(to_string = "Process Info")]
    Process,
    #[strum(to_string = "Inspector")]
    Inspect,
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
            inspect_scroll_state: ScrollViewState::default(),
            colors: CommonColors::default(),
            show_help: false,
        }
    }
}

impl App<'_> {
    /// Constructs a new instance of [`App`].
    pub fn new(filepath: String, colors: Option<CommonColors> ) -> Self {
        let now = Instant::now();
        let parser = parser::CDParser::new(&filepath).unwrap();
        let mut ret = Self::default();

        ret.filepath = filepath.clone();
        if let Some(colors) = colors { 
            ret.colors = colors;
        }

        ret.index_map = parser.build_index().unwrap();
        ret.crash_dump = parser.parse(&ret.index_map).unwrap();

        ret.ancestor_map = parser::CDParser::create_descendants_table(&ret.crash_dump.processes);
        ret.crash_dump.group_info_map =
            parser::CDParser::calculate_group_info(&ret.ancestor_map, &ret.crash_dump.processes);

        let read_only_processes = ret.crash_dump.processes.clone().into_read_only();
        ret.process_readonly_view = Some(read_only_processes);

        ret.sort_and_update_process_table();
        ret.sort_and_update_process_group_table();

        ///////// Process Group Info

        ret.footer_text.insert(SelectedTab::Process, "Press S for Stack, H for Heap, M for Message Queue | I to inspect contents |  < > to change tabs | Press q to quit".to_string());
        ret.footer_text.insert(
            SelectedTab::Inspect,
            " <-, -> to change tabs | q to quit | ? for help".to_string(),
        );

        if let Some(state) = ret.table_states.get_mut(&SelectedTab::Process) {
            if !ret.tab_lists[&SelectedTab::Process].is_empty() {
                state.select(Some(0));
            }
        }

        if let Some(state) = ret.table_states.get_mut(&SelectedTab::ProcessGroup) {
            if !ret.tab_lists[&SelectedTab::ProcessGroup].is_empty() {
                state.select(Some(0));
            }
        }

        ret.inspect_scroll_state = ScrollViewState::default();

        let elapsed = now.elapsed();
        println!("Building everything took: {:.2?}", elapsed);

        ret
    }

    pub fn sort_and_update_process_group_table(&mut self) {
        let column = self.process_group_sort_column;
        let direction = self.process_group_sort_direction;

        let mut sorted_keys: Vec<(&String, &GroupInfo)> = self
            .crash_dump
            .group_info_map
            .par_iter() // Use parallel iterator
            .collect();
        sorted_keys.par_sort_by(|a, b| match (a.1, b.1) {
            (grp_a, grp_b) => match direction {
                SortDirection::Ascending => match column {
                    ProcessGroupSortColumn::Pid => grp_a.pid.cmp(&grp_b.pid),
                    ProcessGroupSortColumn::Name => grp_a.name.cmp(&grp_b.name),
                    ProcessGroupSortColumn::ChildrenCount => grp_a.children.len().cmp(&grp_b.children.len()),
                    ProcessGroupSortColumn::TotalMemorySize => {
                        grp_a.total_memory_size.cmp(&grp_b.total_memory_size)
                    }
                },
                SortDirection::Descending => match column {
                    ProcessGroupSortColumn::Pid => grp_b.pid.cmp(&grp_a.pid),
                    ProcessGroupSortColumn::Name => grp_b.name.cmp(&grp_a.name),
                    ProcessGroupSortColumn::ChildrenCount => grp_b.children.len().cmp(&grp_a.children.len()),
                    ProcessGroupSortColumn::TotalMemorySize => {
                        grp_b.total_memory_size.cmp(&grp_a.total_memory_size)
                    }
                },
            },
        });

        let selected_row_style = Style::default()
            .fg(self.colors.highlight_text)
            .bg(self.colors.highlight_background);
        let header_style = Style::default()
            .fg(self.colors.header_text)
            .bg(self.colors.header_background);

        let sorted_key_list: Vec<String> = sorted_keys
            .into_par_iter() // Use parallel iterator
            .map(|(key, _)| key.clone())
            .collect();
        self.tab_lists
            .get_mut(&SelectedTab::ProcessGroup)
            .map(|val| {
                *val = sorted_key_list;
            });
        let process_group_rows: Vec<Row> = self.tab_lists[&SelectedTab::ProcessGroup]
            .par_iter()
            .map(|group| {
                let group_info = self.crash_dump.group_info_map.get(group).unwrap();
                let item = group_info.ref_array();
                Row::new(item)
            })
            .collect();

        let header_cells = GroupInfo::headers()
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let indicator = if i == self.process_group_sort_column as usize {
                    match self.process_group_sort_direction {
                        SortDirection::Ascending => " ▲",
                        SortDirection::Descending => " ▼",
                    }
                } else {
                    ""
                };
                Cell::from(format!("{}{}", h, indicator))
            })
            .collect::<Vec<_>>();

        let process_group_header = Row::new(header_cells).style(header_style).height(1);

        self.process_group_table = Table::new(
            process_group_rows,
            [
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(30),
            ],
        )
        .header(process_group_header)
        .row_highlight_style(selected_row_style)
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().title(SelectedTab::Process.to_string()));
    }

    pub fn sort_and_update_process_table(&mut self) {
        let column = self.process_sort_column;
        let direction = self.process_sort_direction;
        let processes_map = &self.crash_dump.processes;

        let mut pids_to_sort: Vec<(&String, &InfoOrIndex<ProcInfo>)> = self
            .process_readonly_view
            .as_ref()
            .unwrap()
            .iter()
            .collect();

        pids_to_sort.par_sort_by(|a, b| match (a.1, b.1) {
            (InfoOrIndex::Info(proc_a), InfoOrIndex::Info(proc_b)) => match direction {
                SortDirection::Ascending => match column {
                    ProcessSortColumn::Pid => proc_a.pid.cmp(&proc_b.pid),
                    ProcessSortColumn::Name => proc_a.name.cmp(&proc_b.name),
                    ProcessSortColumn::Memory => proc_a.memory.cmp(&proc_b.memory),
                    ProcessSortColumn::TotalBinVHeap => {
                        proc_a.total_bin_vheap.cmp(&proc_b.total_bin_vheap)
                    }
                    ProcessSortColumn::BinVHeap => proc_a.bin_vheap.cmp(&proc_b.bin_vheap),
                    ProcessSortColumn::BinVHeapUnused => {
                        proc_a.bin_vheap_unused.cmp(&proc_b.bin_vheap_unused)
                    }
                    ProcessSortColumn::OldBinVHeap => {
                        proc_a.old_bin_vheap.cmp(&proc_b.old_bin_vheap)
                    }
                    ProcessSortColumn::OldBinVHeapUnused => proc_a
                        .old_bin_vheap_unused
                        .cmp(&proc_b.old_bin_vheap_unused),
                    ProcessSortColumn::MessageQueueLength => proc_a
                        .message_queue_length
                        .cmp(&proc_b.message_queue_length),
                },
                // This is very gross, but it's very fast
                SortDirection::Descending => match column {
                    ProcessSortColumn::Pid => proc_b.pid.cmp(&proc_a.pid),
                    ProcessSortColumn::Name => proc_b.name.cmp(&proc_a.name),
                    ProcessSortColumn::Memory => proc_b.memory.cmp(&proc_a.memory),
                    ProcessSortColumn::TotalBinVHeap => {
                        proc_b.total_bin_vheap.cmp(&proc_a.total_bin_vheap)
                    }
                    ProcessSortColumn::BinVHeap => proc_b.bin_vheap.cmp(&proc_a.bin_vheap),
                    ProcessSortColumn::BinVHeapUnused => {
                        proc_b.bin_vheap_unused.cmp(&proc_a.bin_vheap_unused)
                    }
                    ProcessSortColumn::OldBinVHeap => {
                        proc_b.old_bin_vheap.cmp(&proc_a.old_bin_vheap)
                    }
                    ProcessSortColumn::OldBinVHeapUnused => proc_b
                        .old_bin_vheap_unused
                        .cmp(&proc_a.old_bin_vheap_unused),
                    ProcessSortColumn::MessageQueueLength => proc_b
                        .message_queue_length
                        .cmp(&proc_a.message_queue_length),
                },
            },
            _ => unreachable!(),
        });

        let sorted_key_list: Vec<String> = pids_to_sort
            .into_par_iter()
            .map(|(key, _)| key.clone())
            .collect();
        self.tab_lists.get_mut(&SelectedTab::Process).map(|val| {
            *val = sorted_key_list;
        });

        let process_rows: Vec<Row> = self.tab_lists[&SelectedTab::Process]
            .par_iter()
            .map(|pid| {
                match processes_map.get(pid) {
                    Some(process_ref) => match *process_ref.value() {
                        InfoOrIndex::Info(ref proc_info) => {
                            // Use the updated ref_array which has 8 elements
                            Row::new(proc_info.ref_array())
                        }
                        _ => Row::new(vec![format!("Index: {}", pid)]),
                    },
                    None => Row::new(vec![format!("Not Found: {}", pid)]),
                }
            })
            .collect();

        let selected_row_style = Style::default()
            .fg(self.colors.highlight_text)
            .bg(self.colors.highlight_background);
        let header_style = Style::default()
            .fg(self.colors.header_text)
            .bg(self.colors.header_background);

        let header_cells = ProcInfo::headers()
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let indicator = if i == self.process_sort_column as usize {
                    match self.process_sort_direction {
                        SortDirection::Ascending => " ▲",
                        SortDirection::Descending => " ▼",
                    }
                } else {
                    ""
                };
                Cell::from(format!("{}{}", h, indicator))
            })
            .collect::<Vec<_>>();

        let process_header = Row::new(header_cells).style(header_style).height(1);

        let constraints = vec![
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(25),
        ];

        self.process_view_table = Table::new(process_rows, constraints) // Use updated constraints
            .header(process_header)
            .row_highlight_style(selected_row_style)
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().title(SelectedTab::Process.to_string()));

        if let Some(state) = self.table_states.get_mut(&SelectedTab::Process) {
            state.select(Some(0));
        }
    }

    /// Handles the tick event of the terminal.
    pub fn tick(&self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.state = AppState::Quitting;
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = self.selected_tab.next()
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = self.selected_tab.previous()
    }

    pub fn get_heap_info(&self, pid: &str) -> io::Result<Text> {
        self.parser
            .get_heap_info(&self.crash_dump, &self.filepath, pid, &self.colors)
    }

    pub fn get_stack_info(&self, pid: &str) -> io::Result<Text> {
        self.parser
            .get_stack_info(&self.crash_dump, &self.filepath, pid, &self.colors)
    }

    pub fn get_message_queue_info(&self, pid: &str) -> io::Result<Text> {
        self.parser
            .get_message_queue_info(&self.crash_dump, &self.filepath, pid, &self.colors)
    }
}

// Separated because this is the UI code. We need this here in order to render stuff *within* App state
impl App<'_> {
    pub fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let titles = SelectedTab::iter().map(SelectedTab::title);
        let highlight_style = (Color::default(), self.selected_tab.palette().c700);
        let selected_tab_index = self.selected_tab as usize;
        Tabs::new(titles)
            .highlight_style(highlight_style)
            .select(selected_tab_index)
            .padding("", "")
            .divider(" ")
            .render(area, buf);
    }

    pub fn get_selected_pid(&self) -> String {
        if self.selected_tab == SelectedTab::Process {
            let process_table_state = self.table_states.get(&SelectedTab::Process).unwrap();
            let selected_item = process_table_state.selected().unwrap_or(0);
            self.tab_lists[&SelectedTab::Process][selected_item].clone()
        } else {
            String::new()
        }
    }
}

impl Widget for &mut App<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(1), Min(0), Length(1)]);
        let [header_area, inner_area, footer_area] = vertical.areas(area);

        let horizontal = Layout::horizontal([Min(0), Length(20)]);
        let [tabs_area, title_area] = horizontal.areas(header_area);

        render_title(title_area, buf);
        self.render_tabs(tabs_area, buf);
        match self.selected_tab {
            SelectedTab::General => self.selected_tab.render_general(inner_area, buf, self),
            SelectedTab::Process => self.selected_tab.render_process(inner_area, buf, self),
            SelectedTab::ProcessGroup => self
                .selected_tab
                .render_process_group(inner_area, buf, self),
            SelectedTab::Inspect => self.selected_tab.render_inspect(inner_area, buf, self),
        }
        let footer_text = "<-, -> to change tabs | Press q to quit | ? for help";
        // let footer_text = self
        //     .footer_text
        //     .get(&self.selected_tab)
        //     .map_or("< > to change tabs | Press q to quit | ", |v| v);
        render_footer(footer_text, footer_area, buf);

        if self.show_help {
            // Define the updated and reordered help text
            let help_text = Text::from(vec![
                Line::from(Span::styled("Keyboard Shortcuts", Style::default().bold())),
                Line::from(""),
                Line::from("General Navigation:"),
                Line::from("  q / Esc       : Quit Application"),
                Line::from("  Ctrl+C        : Quit Application"),
                Line::from("  ?             : Toggle this Help Popup"),
                Line::from("  Left / Right  : Change Tabs"),
                Line::from(""),
                // --- Process Group Tab Section (Moved Up) ---
                Line::from("Process Group Tab ('Process Group Info'):"),
                Line::from("  Up / k        : Move Selection Up"),
                Line::from("  Down / j      : Move Selection Down"),
                Line::from("  Shift+T       : Sort by Total Memory Size"),
                Line::from("  Shift+P       : Sort by Pid"),
                Line::from("  Shift+N       : Sort by Name"),
                Line::from("  Shift+C       : Sort by Children Count"),
                Line::from("  (Press same sort key again to toggle Asc/Desc direction)"),
                Line::from(""),
                // --- Process List Tab Section (Moved Down) ---
                Line::from("Process List Tab ('Process Info'):"),
                Line::from("  Up / k        : Move Selection Up"),
                Line::from("  Down / j      : Move Selection Down"),
                Line::from("  S             : View Process Stack (in lower pane)"),
                Line::from("  H             : View Process Heap (in lower pane)"),
                Line::from("  M             : View Process Message Queue (in lower pane)"),
                Line::from("  I             : Inspect Selected View (Stack/Heap/MsgQ) fullscreen"),
                Line::from("  Shift+P       : Sort by Pid"),
                Line::from("  Shift+N       : Sort by Name"),
                Line::from("  Shift+Q       : Sort by Msg Queue Len"),
                Line::from("  Shift+M       : Sort by Memory"),
                Line::from("  Shift+T       : Sort by TotalBinVHeap"),
                Line::from("  Shift+B       : Sort by BinVHeap"),
                Line::from("  Shift+U       : Sort by BinVHeap Unused"),
                Line::from("  Shift+O       : Sort by OldBinVHeap"),
                Line::from("  Shift+V       : Sort by OldBinVHeap Unused"),
                Line::from("  (Press same sort key again to toggle Asc/Desc direction)"),
                Line::from(""),
                // --- Inspect View Section ---
                Line::from("Inspect View:"),
                Line::from("  Up / k        : Scroll Up"),
                Line::from("  Down / j      : Scroll Down"),
                Line::from("  PgUp / b      : Page Up"),
                Line::from("  PgDown / f    : Page Down"),
                Line::from("  Home / g      : Go to Top"),
                Line::from("  End / G       : Go to Bottom"),
                Line::from("  I             : Return to Process List"),
            ]);

            let block = Block::bordered()
                .title(Line::from(vec![
                    Span::styled(
                        " Help ",
                        Style::default().fg(self.colors.header_text).bold(),
                    ),
                    Span::styled(
                        "[?/Esc/q to close]",
                        Style::default().fg(self.colors.default_text),
                    ),
                ]))
                .border_style(Style::default().fg(self.colors.border_color))
                .style(Style::default().bg(self.colors.background_color));

            let paragraph = Paragraph::new(help_text)
                .block(block)
                .wrap(Wrap { trim: true });

            let area = centered_rect(70, 90, area);

            Clear.render(area, buf);
            paragraph.render(area, buf);
        }
    }
}

impl SelectedTab {
    /// Get the previous tab, if there is no previous tab return the current tab.
    fn previous(self) -> Self {
        let current_index: usize = self as usize;
        let previous_index = current_index.saturating_sub(1);
        Self::from_repr(previous_index).unwrap_or(self)
    }

    /// Get the next tab, if there is no next tab return the current tab.
    fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);
        Self::from_repr(next_index).unwrap_or(self)
    }
}

impl SelectedTab {
    /// Return tab's name as a styled `Line`
    fn title(self) -> Line<'static> {
        format!("  {self}  ")
            .fg(tailwind::SLATE.c200)
            .bg(self.palette().c900)
            .into()
    }

    fn render_general(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        let preamble_text = app.crash_dump.preamble.format();
        let process_count = app.index_map[&Tag::Proc].len();
        let ets_count = app.index_map[&Tag::Ets].len();
        let fn_count = app.index_map[&Tag::Fun].len();

        let memory_info_text = app.crash_dump.memory.format();

        // Split the preamble text into lines
        let preamble_lines: Vec<Line> = preamble_text
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line,
                    Style::default().fg(app.colors.default_text),
                ))
            })
            .collect();

        // Split the memory information text into lines
        let memory_information_lines: Vec<Line> = memory_info_text
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line,
                    Style::default().fg(app.colors.default_text),
                ))
            })
            .collect();

        // Add a header for memory information
        let memory_information_header = Line::from(vec![
            Span::styled(
                "Memory Information:",
                Style::default().fg(app.colors.info_preamble),
            ),
            Span::raw("\n"),
        ]);

        let process_count = Line::from(vec![
            Span::styled("Process Count: ", Style::default().fg(app.colors.info_preamble)),
            Span::styled(
                process_count.to_string(),
                Style::default().fg(app.colors.default_text),
            ),
        ]);

        let ets_count = Line::from(vec![
            Span::styled("ETS Tables: ", Style::default().fg(app.colors.info_preamble)),
            Span::styled(
                ets_count.to_string(),
                Style::default().fg(app.colors.default_text),
            ),
        ]);

        let fn_count = Line::from(vec![
            Span::styled("Funs: ", Style::default().fg(app.colors.info_preamble)),
            Span::styled(
                fn_count.to_string(),
                Style::default().fg(app.colors.default_text),
            ),
        ]);

        // Combine all lines into a single Text object
        let mut general_info_text = Text::from(preamble_lines);
        general_info_text.extend(vec![memory_information_header]);
        general_info_text.extend(memory_information_lines);
        general_info_text.extend(process_count);
        general_info_text.extend(ets_count);
        general_info_text.extend(fn_count);

        let paragraph = Paragraph::new(general_info_text)
            .block(Block::bordered().title("General Information"))
            .style(Style::default().fg(app.colors.default_text))
            .alignment(Alignment::Left);

        Widget::render(&paragraph, area, buf);
    }

    // fn render_index(self, area: Rect, buf: &mut Buffer, app: &mut App) {
    //     let index_list_state = app.list_states.get_mut(&SelectedTab::Index).unwrap();
    //     let list_items: Vec<ListItem> = app.tab_lists[&SelectedTab::Index]
    //         .iter()
    //         .map(|i| ListItem::new::<&str>(i.as_ref()))
    //         .collect();

    //     let binding = SelectedTab::Index.to_string();
    //     let list = List::new(list_items)
    //         .block(Block::bordered().title(binding.as_str()))
    //         .highlight_symbol(">>")
    //         .repeat_highlight_symbol(true)
    //         .highlight_style(Style::default().bg(Color::Blue));

    //     StatefulWidget::render(list, area, buf, index_list_state);
    // }

    fn render_process(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // split the second side into the info side
        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(outer_layout[1]);

        let selected_item;
        {
            let process_table_state = app.table_states.get_mut(&SelectedTab::Process).unwrap();
            selected_item = process_table_state.selected().unwrap_or(0);
            StatefulWidget::render(
                &app.process_view_table,
                outer_layout[0],
                buf,
                process_table_state,
            );
        }

        let selected_pid = &app.tab_lists[&SelectedTab::Process][selected_item];
        let selected_process_result = app.crash_dump.processes.get(selected_pid);

        let active_proc_info: types::ProcInfo;
        let process_info_text: Text;
        match selected_process_result {
            Some(process_ref) => {
                let text = match *process_ref.value() {
                    InfoOrIndex::Info(ref proc_info) => {
                        let proc_info: &types::ProcInfo = proc_info;
                        active_proc_info = proc_info.clone();
                        active_proc_info.format_as_ratatui_text(&app.colors)
                    }
                    InfoOrIndex::Index(_) => {
                        Text::raw(format!("Index for pid: {:?}", selected_pid).to_string())
                    }
                };
                process_info_text = text;
            }
            None => {
                process_info_text =
                    Text::raw(format!("Process not found: {:?}", selected_pid).to_string());
            }
        };

        let (inspect_info_title, inspect_info_text) = match app.process_view_state {
            ProcessViewState::Stack => {
                app.inspecting_pid = selected_pid.clone();
                ("Decoded Stack", app.get_stack_info(selected_pid).unwrap())
            }
            ProcessViewState::Heap => {
                app.inspecting_pid = selected_pid.clone();

                ("Decoded Heap", app.get_heap_info(selected_pid).unwrap())
            }
            ProcessViewState::MessageQueue => {
                app.inspecting_pid = selected_pid.clone();
                (
                    "Decoded Message Queue",
                    app.get_message_queue_info(selected_pid).unwrap(),
                )
            }
        };

        //println!("heap info text: {}", heap_info_text);

        let detail_block = Paragraph::new(process_info_text)
            .block(Block::bordered().title("Process Details"))
            .style(Style::default().fg(app.colors.default_text))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        let proc_heap = Paragraph::new(inspect_info_text)
            .block(Block::bordered().title(inspect_info_title))
            .style(Style::default().fg(app.colors.default_text))
            .alignment(Alignment::Left);

        Widget::render(&detail_block, inner_layout[0], buf);
        Widget::render(&proc_heap, inner_layout[1], buf);
    }

    fn render_process_group(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // split the second side into the info side
        let inner_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer_layout[1]);

        let group_table_state = app
            .table_states
            .get_mut(&SelectedTab::ProcessGroup)
            .unwrap();

        let selected_item = group_table_state.selected().unwrap_or(0);
        let selected_pid = &app.tab_lists[&SelectedTab::ProcessGroup][selected_item];
        let selected_process_result = app.crash_dump.processes.get(selected_pid);

        let active_proc_info: types::ProcInfo;
        let process_info_text: Text;
        match selected_process_result {
            Some(process_ref) => {
                let text = match *process_ref.value() {
                    InfoOrIndex::Info(ref proc_info) => {
                        let proc_info: &types::ProcInfo = proc_info;
                        active_proc_info = proc_info.clone();
                        active_proc_info.format_as_ratatui_text(&app.colors)
                    }
                    InfoOrIndex::Index(_) => {
                        Text::raw(format!("Index for pid: {:?}", selected_pid).to_string())
                    }
                };
                process_info_text = text;
            }
            None => {
                process_info_text =
                    Text::raw(format!("Process not found: {:?}", selected_pid).to_string());
            }
        };

        let children: Vec<Row> = match app.ancestor_map.get(selected_pid) {
            Some(child_pids) => {
                child_pids
                    .iter() // Use iter() here as we are just borrowing the child_pids
                    .map(|child_pid| {
                        match app.crash_dump.processes.get(child_pid) {
                            Some(child_info_ref) => {
                                match *child_info_ref.value() {
                                    // Dereference the Ref
                                    InfoOrIndex::Info(ref proc_info) => {
                                        Row::new(proc_info.summary_ref_array())
                                    }
                                    InfoOrIndex::Index(_) => {
                                        Row::new(vec![format!("{:?}", child_pid)])
                                    } // Format the pid
                                }
                            }
                            None => {
                                // Handle the case where child_pid is not found in processes
                                Row::new(vec![format!("Info not found: {:?}", child_pid)])
                            }
                        }
                    })
                    .collect()
            }
            None => vec![Row::new(vec!["No data".to_string()])],
        };

        // needs Pid, Name, Reductions, Memory, MsgQ Length,
        let children_block = Table::new(
            children,
            [
                Constraint::Length(15),
                Constraint::Length(60),
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Length(25),
            ],
        )
        .header(
            ["Pid", "Name", "Memory", "Reductions", "MsgQ Length"]
                .iter()
                .map(|&h| Cell::from(h))
                .collect::<Row>()
                .style(
                    Style::default()
                        .fg(app.colors.default_text)
                        .bg(app.colors.header_background),
                ),
        )
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().title("Group Children"));

        let detail_block = Paragraph::new(process_info_text)
            .block(Block::bordered().title("Ancestor Details"))
            .style(Style::default().fg(app.colors.default_text))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        Widget::render(&children_block, inner_layout[0], buf);
        Widget::render(&detail_block, inner_layout[1], buf);
        StatefulWidget::render(
            &app.process_group_table,
            outer_layout[0],
            buf,
            group_table_state,
        );
    }

    fn render_inspect(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        let width = if buf.area.height < 70 {
            buf.area.width - 1
        } else {
            buf.area.width
        };
        let mut scroll_view = ScrollView::new(Size::new(width, 70));

        let inspect_info_text;
        let inspect_info_title;
        {
            let (t1, t2) = match app.process_view_state {
                ProcessViewState::Stack => (
                    "Decoded Stack",
                    app.get_stack_info(&app.inspecting_pid).unwrap(),
                ),
                ProcessViewState::Heap => (
                    "Decoded Heap",
                    app.get_heap_info(&app.inspecting_pid).unwrap(),
                ),
                ProcessViewState::MessageQueue => (
                    "Decoded Message Queue",
                    app.get_message_queue_info(&app.inspecting_pid).unwrap(),
                ),
            };
            inspect_info_title = t1;
            inspect_info_text = t2.clone();
        }

        let proc_info = Paragraph::new(inspect_info_text)
            .block(Block::bordered().title(inspect_info_title))
            .style(Style::default().fg(app.colors.default_text))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        proc_info.render(area, &mut scroll_view.buf_mut());
        scroll_view.render(area, buf, &mut app.inspect_scroll_state);
    }

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::General => tailwind::BLUE,
            Self::Process => tailwind::EMERALD,
            Self::ProcessGroup => tailwind::INDIGO,
            Self::Inspect => tailwind::PURPLE,
        }
    }
}

fn render_title(area: Rect, buf: &mut Buffer) {
    "ERL Crash Dump".render(area, buf);
}

fn render_footer(footer_text: &str, area: Rect, buf: &mut Buffer) {
    Line::raw(footer_text).centered().render(area, buf);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
