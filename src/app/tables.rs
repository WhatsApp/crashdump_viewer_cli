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

use super::state::{
    App, ProcessGroupSortColumn, ProcessSortColumn, SelectedTab, SortDirection,
};
use crate::parser::types::{GroupInfo, InfoOrIndex, ProcInfo};
use ratatui::{
    style::Style,
    widgets::{Block, Cell, HighlightSpacing, Row, Table},
};
use rayon::prelude::*;

impl App<'_> {
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
                    ProcessGroupSortColumn::ChildrenCount => {
                        grp_a.children.len().cmp(&grp_b.children.len())
                    }
                    ProcessGroupSortColumn::TotalMemorySize => {
                        grp_a.total_memory_size.cmp(&grp_b.total_memory_size)
                    }
                },
                SortDirection::Descending => match column {
                    ProcessGroupSortColumn::Pid => grp_b.pid.cmp(&grp_a.pid),
                    ProcessGroupSortColumn::Name => grp_b.name.cmp(&grp_a.name),
                    ProcessGroupSortColumn::ChildrenCount => {
                        grp_b.children.len().cmp(&grp_a.children.len())
                    }
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

        // Extract keys without unnecessary parallel overhead for simple cloning
        let sorted_key_list: Vec<String> = sorted_keys
            .iter()
            .map(|(key, _)| (*key).clone())
            .collect();
        self.tab_lists
            .get_mut(&SelectedTab::ProcessGroup)
            .map(|val| {
                *val = sorted_key_list;
            });
        let process_group_rows: Vec<Row> = self.tab_lists[&SelectedTab::ProcessGroup]
            .par_iter()
            .filter_map(|group| {
                self.crash_dump
                    .group_info_map
                    .get(group)
                    .map(|group_info| {
                        let item = group_info.ref_array();
                        Row::new(item)
                    })
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
                ratatui::layout::Constraint::Length(30),
                ratatui::layout::Constraint::Length(30),
                ratatui::layout::Constraint::Length(30),
                ratatui::layout::Constraint::Length(30),
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

        // Guard: return early if process_readonly_view is None
        let Some(readonly_view) = self.process_readonly_view.as_ref() else {
            return;
        };

        let mut pids_to_sort: Vec<(&String, &InfoOrIndex<ProcInfo>)> =
            readonly_view.iter().collect();

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

        // Extract keys without unnecessary parallel overhead for simple cloning
        let sorted_key_list: Vec<String> = pids_to_sort
            .iter()
            .map(|(key, _)| (*key).clone())
            .collect();
        self.tab_lists.get_mut(&SelectedTab::Process).map(|val| {
            *val = sorted_key_list;
        });

        let process_rows: Vec<Row> = self.tab_lists[&SelectedTab::Process]
            .par_iter()
            .map(|pid| match processes_map.get(pid) {
                Some(process_ref) => match *process_ref.value() {
                    InfoOrIndex::Info(ref proc_info) => {
                        // Use the updated ref_array which has 8 elements
                        Row::new(proc_info.ref_array())
                    }
                    _ => Row::new(vec![format!("Index: {}", pid)]),
                },
                None => Row::new(vec![format!("Not Found: {}", pid)]),
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
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
            ratatui::layout::Constraint::Length(25),
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
}
