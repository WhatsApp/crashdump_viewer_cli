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

use crate::parser::*;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{palette::tailwind, Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Cell, HighlightSpacing, Paragraph, Row, StatefulWidget, Table, TableState, Tabs,
        Widget,
    },
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::error;
use std::io;
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

    pub table_states: HashMap<SelectedTab, TableState>,

    pub process_group_table: Table<'a>,

    pub process_view_table: Table<'a>,
    pub process_view_state: ProcessViewState,

    pub footer_text: HashMap<SelectedTab, String>,
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
    // #[strum(to_string = "Index")]
    // Index,
    #[strum(to_string = "Process Group Info")]
    ProcessGroup,
    #[strum(to_string = "Process Info")]
    Process,
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
            process_view_state: ProcessViewState::default(),
            process_view_table: Table::default(),
            footer_text: HashMap::new(),
        }
    }
}

impl App<'_> {
    /// Constructs a new instance of [`App`].
    pub fn new(filepath: String) -> Self {
        let parser = parser::CDParser::new(&filepath).unwrap();
        let idx = parser.build_index().unwrap();

        let mut ret = Self::default();
        ret.filepath = filepath.clone();
        ret.index_map = idx.clone();
        ret.process_view_state = ProcessViewState::default();

        // store the index
        // let idx_str = parser::CDParser::format_index(&idx);
        // ret.tab_lists.get_mut(&SelectedTab::Index).map(|val| {
        //     *val = idx_str;
        // });

        let crash_dump = parser.parse().unwrap();
        ret.crash_dump = crash_dump;

        //println!("heap addrs: {:?}", ret.crash_dump.all_heap_addresses);
        //println!("binaries: {:?}", ret.crash_dump.visited_binaries);

        ret.ancestor_map = parser::CDParser::create_descendants_table(&ret.crash_dump.processes);
        // for every ancestor:<children> mapping, we need to calculate the GroupInfo for each one if the pid exists
        let group_info =
            parser::CDParser::calculate_group_info(&ret.ancestor_map, &ret.crash_dump.processes);
        ret.crash_dump.group_info_map = group_info;
        //    let all_processes = &mut ret.crash_dump.processes;

        // set the process list to be a tuple of [pid, name, heap_size, msgq_len]
        // we need to be able to sort an array based on the msgqlength as well

        //////////////// Individual Process View
        // let mut sorted_keys = ret
        //     .crash_dump
        //     .processes
        //     .iter()
        //     .collect::<Vec<(&String, &InfoOrIndex<ProcInfo>)>>();
        // sorted_keys.sort_by(|a, b| match (a.1, b.1) {
        //     (InfoOrIndex::Info(proc_info_a), InfoOrIndex::Info(proc_info_b)) => {
        //         proc_info_b.memory.cmp(&proc_info_a.memory)
        //     }
        //     _ => unreachable!(),
        // });

        // let sorted_key_list = sorted_keys
        //     .into_iter()
        //     .map(|(key, _)| key.clone())
        //     .collect::<Vec<String>>();

        // ret.tab_lists.get_mut(&SelectedTab::Process).map(|val| {
        //     *val = sorted_key_list;
        // });

        // let process_rows: Vec<Row> = ret.tab_lists[&SelectedTab::Process]
        //     .iter()
        //     .map(|pid| match ret.crash_dump.processes.get(pid).unwrap() {
        //         InfoOrIndex::Info(proc_info) => {
        //             let item = proc_info.ref_array();
        //             Row::new(item)
        //         }
        //         _ => {
        //             unreachable!();
        //         }
        //     })
        //     .collect();
        let mut sorted_keys: Vec<(&String, &InfoOrIndex<ProcInfo>)> = ret
            .crash_dump
            .processes
            .par_iter() // Use parallel iterator
            .collect();
        sorted_keys.par_sort_by(|a, b| match (a.1, b.1) {
            (InfoOrIndex::Info(proc_info_a), InfoOrIndex::Info(proc_info_b)) => {
                proc_info_b.bin_vheap.cmp(&proc_info_a.bin_vheap)
            }
            _ => unreachable!(),
        });
        let sorted_key_list: Vec<String> = sorted_keys
            .into_par_iter() // Use parallel iterator
            .map(|(key, _)| key.clone())
            .collect();
        ret.tab_lists.get_mut(&SelectedTab::Process).map(|val| {
            *val = sorted_key_list;
        });
        let process_rows: Vec<Row> = ret.tab_lists[&SelectedTab::Process]
            .par_iter() // Use parallel iterator
            .map(|pid| match ret.crash_dump.processes.get(pid).unwrap() {
                InfoOrIndex::Info(proc_info) => {
                    let item = proc_info.ref_array();
                    Row::new(item)
                }
                _ => {
                    unreachable!();
                }
            })
            .collect();

        let selected_row_style = Style::default().fg(Color::White);
        let selected_col_style = Style::default().fg(Color::White);
        let selected_cell_style = Style::default().fg(Color::White);
        let header_style = Style::default().fg(Color::White).bg(Color::Red);

        let process_header = ProcInfo::headers()
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(header_style)
            .height(1);

        ret.process_view_table = Table::new(
            process_rows,
            [
                Constraint::Length(15),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
                Constraint::Length(25),
            ],
        )
        .header(process_header)
        .row_highlight_style(selected_row_style)
        .column_highlight_style(selected_col_style)
        .cell_highlight_style(selected_cell_style)
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().title(SelectedTab::Process.to_string()))
        .highlight_style(Style::default().bg(Color::Blue));

        ///////// Process Group Info

        let mut sorted_keys: Vec<(&String, &GroupInfo)> = ret
            .crash_dump
            .group_info_map
            .par_iter() // Use parallel iterator
            .collect();
        sorted_keys.par_sort_by(|a, b| b.1.total_memory_size.cmp(&a.1.total_memory_size));
        let sorted_key_list: Vec<String> = sorted_keys
            .into_par_iter() // Use parallel iterator
            .map(|(key, _)| key.clone())
            .collect();
        ret.tab_lists
            .get_mut(&SelectedTab::ProcessGroup)
            .map(|val| {
                *val = sorted_key_list;
            });
        let process_group_rows: Vec<Row> = ret.tab_lists[&SelectedTab::ProcessGroup]
            .par_iter() // Use parallel iterator
            .map(|group| {
                let group_info = ret.crash_dump.group_info_map.get(group).unwrap();
                let item = group_info.ref_array();
                Row::new(item)
            })
            .collect();

        let process_group_headers = GroupInfo::headers()
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(header_style)
            .height(1);

        ret.process_group_table = Table::new(
            process_group_rows,
            [
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(30),
            ],
        )
        .header(process_group_headers)
        .row_highlight_style(selected_row_style)
        .column_highlight_style(selected_col_style)
        .cell_highlight_style(selected_cell_style)
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().title(SelectedTab::Process.to_string()))
        .highlight_style(Style::default().bg(Color::Blue));

        ret.footer_text.insert(SelectedTab::Process, "Press S for Stack, H for Heap, M for Message Queue | < > to change tabs | Press q to quit".to_string());

        // if let Some(state) = ret.table_states.get_mut(&SelectedTab::Index) {
        //     if !ret.tab_lists[&SelectedTab::Index].is_empty() {
        //         state.select(Some(0));
        //     }
        // }

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

        ret
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

    pub fn get_heap_info(&self, pid: &str) -> io::Result<String> {
        self.parser
            .get_heap_info(&self.crash_dump, &self.filepath, pid)
    }

    pub fn get_stack_info(&self, pid: &str) -> io::Result<Text> {
        self.parser
            .get_stack_info(&self.crash_dump, &self.filepath, pid)
        // Ok("".to_string())
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
            //SelectedTab::Index => self.selected_tab.render_index(inner_area, buf, self),
            SelectedTab::Process => self.selected_tab.render_process(inner_area, buf, self),
            SelectedTab::ProcessGroup => self
                .selected_tab
                .render_process_group(inner_area, buf, self),
        }
        let footer_text = self
            .footer_text
            .get(&self.selected_tab)
            .map_or("< > to change tabs | Press q to quit", |v| v);
        render_footer(footer_text, footer_area, buf);
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
            .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
            .collect();

        // Split the memory information text into lines
        let memory_information_lines: Vec<Line> = memory_info_text
            .lines()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
            .collect();

        // Add a header for memory information
        let memory_information_header = Line::from(vec![
            Span::styled("Memory Information:", Style::default().fg(Color::Yellow)),
            Span::raw("\n"),
        ]);

        let process_count = Line::from(vec![
            Span::styled("Process Count: ", Style::default().fg(Color::Cyan)),
            Span::styled(process_count.to_string(), Style::default().fg(Color::White)),
        ]);

        let ets_count = Line::from(vec![
            Span::styled("ETS Tables: ", Style::default().fg(Color::Cyan)),
            Span::styled(ets_count.to_string(), Style::default().fg(Color::White)),
        ]);

        let fn_count = Line::from(vec![
            Span::styled("Funs: ", Style::default().fg(Color::Cyan)),
            Span::styled(fn_count.to_string(), Style::default().fg(Color::White)),
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
            .style(Style::default().fg(Color::White))
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
        let selected_process = app.crash_dump.processes.get(selected_pid).unwrap();

        let process_info_text = match selected_process {
            InfoOrIndex::Info(proc_info) => proc_info.format(),
            InfoOrIndex::Index(_) => unreachable!(),
        };

        let (inspect_info_title, inspect_info_text) = match app.process_view_state {
            ProcessViewState::Stack => ("Decoded Stack", app.get_stack_info(selected_pid).unwrap()),
            _ => todo!(),
            // ProcessViewState::Heap => ("Decoded Heap", app.get_heap_info(selected_pid).unwrap()),
            // ProcessViewState::MessageQueue => ("Decoded Message Queue", "".to_string()),
        };

        //println!("heap info text: {}", heap_info_text);

        let detail_block = Paragraph::new(process_info_text)
            .block(Block::bordered().title("Process Details"))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left);

        let proc_heap = Paragraph::new(inspect_info_text)
            .block(Block::bordered().title(inspect_info_title))
            .style(Style::default().fg(Color::White))
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
            .constraints(vec![Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(outer_layout[1]);

        let group_table_state = app
            .table_states
            .get_mut(&SelectedTab::ProcessGroup)
            .unwrap();

        let selected_item = group_table_state.selected().unwrap_or(0);
        let selected_pid = &app.tab_lists[&SelectedTab::ProcessGroup][selected_item];
        let selected_process = app.crash_dump.processes.get(selected_pid);

        let process_info_text = match selected_process {
            Some(InfoOrIndex::Info(proc_info)) => proc_info.format(),
            _ => "No process info found".to_string(),
        };

        let group_info_text = match app.ancestor_map.get(selected_pid) {
            Some(children) => {
                format!("{:#?}", children)
            }
            _ => "No group info found".to_string(),
        };

        let children_block = Paragraph::new(group_info_text)
            .block(Block::bordered().title("Group Children"))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left);

        let detail_block = Paragraph::new(process_info_text)
            .block(Block::bordered().title("Ancestor Details"))
            .style(Style::default().fg(Color::White))
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

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::General => tailwind::BLUE,
            //Self::Index => tailwind::TEAL,
            Self::Process => tailwind::EMERALD,
            Self::ProcessGroup => tailwind::INDIGO,
        }
    }
}

fn render_title(area: Rect, buf: &mut Buffer) {
    "ERL Crash Dump".render(area, buf);
}

fn render_footer(footer_text: &str, area: Rect, buf: &mut Buffer) {
    Line::raw(footer_text).centered().render(area, buf);
}
