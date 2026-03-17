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

// Module declarations
mod analysis;
mod search;
mod state;
mod tables;

// Re-export the public API
pub use state::{
    AnalysisCategory, App, AppResult, AppState, MessageData, ProcessGroupSortColumn,
    ProcessSortColumn, ProcessSuggestion, ProcessViewState, SelectedTab, SortDirection,
    StackFrameData,
};

use crate::config::CommonColors;
use crate::parser::*;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{palette::tailwind, Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Cell, Clear, HighlightSpacing, Paragraph, Row, StatefulWidget, Table, Tabs, Widget,
        Wrap,
    },
};
use std::io;
use std::time::Instant;
use strum::IntoEnumIterator;

// Basic App methods
impl App<'_> {
    pub fn new(filepath: String, colors: Option<CommonColors>) -> Result<Self, io::Error> {
        let now = Instant::now();

        let parser = parser::CDParser::new(&filepath)?;

        let mut ret = Self::default();

        ret.filepath = filepath.clone();
        if let Some(colors) = colors {
            ret.colors = colors;
        }

        ret.index_map = parser.build_index()?;
        if ret.index_map.get(&Tag::Proc) == None {
            println!("Couldn't find any processes! Are you sure this crash dump is valid?");
            return Err(io::Error::other(
                "Invalid crash dump detected. Found no `proc` sections.",
            ));
        }
        ret.crash_dump = parser.parse(&ret.index_map)?;

        ret.ancestor_map =
            parser::CDParser::create_descendants_table(&ret.crash_dump.processes);
        ret.crash_dump.group_info_map = parser::CDParser::calculate_group_info(
            &ret.ancestor_map,
            &ret.crash_dump.processes,
        );

        let read_only_processes = ret.crash_dump.processes.clone().into_read_only();
        ret.process_readonly_view = Some(read_only_processes);

        ret.sort_and_update_process_table();
        ret.sort_and_update_process_group_table();

        ///////// Process Group Info

        ret.footer_text.insert(SelectedTab::Process, "Press S for Stack, H for Heap, M for Message Queue | < > to change tabs | Press q to quit".to_string());

        // .get_mut() returns Option, but it's okay if it's None here
        // as we just skip the select call.
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

        let elapsed = now.elapsed();
        println!("Building everything took: {:.2?}", elapsed);

        Ok(ret)
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

// UI rendering code - Separated because this is the UI code
// We need this here in order to render stuff *within* App state
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
            if let Some(process_table_state) = self.table_states.get(&SelectedTab::Process) {
                let selected_item = process_table_state.selected().unwrap_or(0);
                if selected_item < self.tab_lists[&SelectedTab::Process].len() {
                    return self.tab_lists[&SelectedTab::Process][selected_item].clone();
                }
            }
        }
        String::new()
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
            SelectedTab::Memory => self.selected_tab.render_memory(inner_area, buf, self),
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
                Line::from("  PgUp / b      : Page Up (10 items)"),
                Line::from("  PgDown / f    : Page Down (10 items)"),
                Line::from("  /             : Start Search (regex)"),
                Line::from("  n             : Next Search Match"),
                Line::from("  N             : Previous Search Match"),
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
                Line::from("  PgUp / b      : Page Up (10 items)"),
                Line::from("  PgDown / f    : Page Down (10 items)"),
                Line::from("  /             : Start Search (regex)"),
                Line::from("  n             : Next Search Match"),
                Line::from("  N             : Previous Search Match"),
                Line::from("  S             : View Process Stack (in lower pane)"),
                Line::from("  H             : View Process Heap (in lower pane)"),
                Line::from("  M             : View Process Message Queue (in lower pane)"),
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
                // --- Analysis Tab Section ---
                Line::from("Analysis Tab:"),
                Line::from("  M             : Run Memory Analysis (top 10 processes)"),
                Line::from("  P             : Process Analysis (opens selection prompt)"),
                Line::from("  R             : Show Registered Processes"),
                Line::from("  Up / k        : Scroll Up"),
                Line::from("  Down / j      : Scroll Down"),
                Line::from("  PgUp / b      : Page Up"),
                Line::from("  PgDown / f    : Page Down"),
                Line::from("  Home / g      : Go to Top"),
                Line::from("  End / G       : Go to Bottom"),
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

            let paragraph = Paragraph::new(help_text).block(block).wrap(Wrap { trim: true });

            let area = centered_rect(70, 90, area);

            Clear.render(area, buf);
            paragraph.render(area, buf);
        }

        // Render search popup if in search mode
        if self.state == AppState::Searching {
            let search_prompt = if !self.search_query.is_empty() {
                format!("Search (regex): {}_", self.search_query)
            } else {
                "Search (regex): _".to_string()
            };

            let block = Block::bordered()
                .title(Line::from(vec![
                    Span::styled(
                        " Search ",
                        Style::default().fg(self.colors.header_text).bold(),
                    ),
                    Span::styled(
                        "[Enter to search, Esc to cancel]",
                        Style::default().fg(self.colors.default_text),
                    ),
                ]))
                .border_style(Style::default().fg(self.colors.border_color))
                .style(Style::default().bg(self.colors.background_color));

            let paragraph = Paragraph::new(search_prompt).block(block);

            let search_area = centered_rect(60, 10, area);
            Clear.render(search_area, buf);
            paragraph.render(search_area, buf);
        }

        // Render analysis prompt popup if in prompt mode
        if self.state == AppState::AnalysisPrompt {
            let prompt_text = if !self.analysis_prompt_input.is_empty() {
                format!("Enter PID or process name: {}_", self.analysis_prompt_input)
            } else {
                "Enter PID or process name: _".to_string()
            };

            // Build suggestions with autocomplete filtering
            let input_lower = self.analysis_prompt_input.to_lowercase();
            let filtered_suggestions: Vec<&ProcessSuggestion> = if input_lower.is_empty() {
                self.analysis_process_suggestions.iter().take(15).collect()
            } else {
                self.analysis_process_suggestions
                    .iter()
                    .filter(|s| {
                        s.pid.to_lowercase().contains(&input_lower)
                            || s.name.to_lowercase().contains(&input_lower)
                    })
                    .take(15)
                    .collect()
            };

            // Ensure selection is within bounds
            let selected_idx = if filtered_suggestions.is_empty() {
                0
            } else {
                self.analysis_suggestion_selected
                    .min(filtered_suggestions.len() - 1)
            };

            // Build table rows
            let rows: Vec<Row> = filtered_suggestions
                .iter()
                .enumerate()
                .map(|(idx, suggestion)| {
                    let display_name = if suggestion.name.is_empty() {
                        "<unnamed>".to_string()
                    } else {
                        suggestion.name.clone()
                    };

                    let row_style = if idx == selected_idx {
                        Style::default().bg(Color::DarkGray).fg(Color::White).bold()
                    } else {
                        Style::default()
                    };

                    Row::new(vec![
                        Cell::from(suggestion.pid.clone()),
                        Cell::from(display_name),
                        Cell::from(types::human_bytes(suggestion.memory)),
                        Cell::from(types::human_bytes(suggestion.stack_heap)),
                        Cell::from(types::human_bytes(suggestion.bin_vheap)),
                        Cell::from(suggestion.message_queue_length.to_string()),
                    ])
                    .style(row_style)
                })
                .collect();

            let header = Row::new(vec![
                Cell::from("PID"),
                Cell::from("Name"),
                Cell::from("Memory"),
                Cell::from("Stack+Heap"),
                Cell::from("BinVHeap"),
                Cell::from("MsgQ"),
            ])
            .style(Style::default().fg(Color::Yellow).bold())
            .height(1);

            let table = Table::new(
                rows,
                vec![
                    Constraint::Length(15), // PID
                    Constraint::Length(30), // Name
                    Constraint::Length(12), // Memory
                    Constraint::Length(12), // Stack+Heap
                    Constraint::Length(12), // BinVHeap
                    Constraint::Length(6),  // MsgQ
                ],
            )
            .header(header)
            .block(Block::default());

            let popup_area = centered_rect(95, 85, area);

            let block = Block::bordered()
                .title(Line::from(vec![Span::styled(
                    " Process Selection ",
                    Style::default().fg(self.colors.header_text).bold(),
                )]))
                .border_style(Style::default().fg(self.colors.border_color))
                .style(Style::default().bg(self.colors.background_color));

            let content_area = block.inner(popup_area);
            let content_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Length(3), // Input prompt
                    Constraint::Min(0),    // Table
                    Constraint::Length(2), // Help text
                ])
                .split(content_area);

            let prompt_paragraph =
                Paragraph::new(prompt_text).style(Style::default().fg(self.colors.default_text));

            let help_text = if filtered_suggestions.is_empty() {
                "[No matches - type to search | Esc to cancel]"
            } else {
                "[↑↓ to select | Enter to analyze | Tab to autocomplete | Esc to cancel]"
            };

            let help_paragraph = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);

            Clear.render(popup_area, buf);
            block.render(popup_area, buf);
            prompt_paragraph.render(content_layout[0], buf);
            Widget::render(&table, content_layout[1], buf);
            help_paragraph.render(content_layout[2], buf);
        }

        // Render stack detail popup if viewing stack frame details
        if self.state == AppState::AnalysisStackDetail {
            if self.selected_stack_idx < self.stack_frames.len() {
                let frame = &self.stack_frames[self.selected_stack_idx];

                let mut detail_lines = vec![
                    Line::from(vec![
                        Span::styled("Function: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            format!(
                                "{}:{}/{} + {}",
                                frame.module, frame.function, frame.arity, frame.offset
                            ),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Address: ", Style::default().fg(Color::Yellow).bold()),
                        Span::raw(&frame.address),
                    ]),
                    Line::from(vec![
                        Span::styled("Return: ", Style::default().fg(Color::Yellow).bold()),
                        Span::raw(&frame.return_addr),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Variables:",
                        Style::default().fg(Color::Yellow).bold(),
                    )),
                    Line::from(""),
                ];

                if frame.variables.is_empty() {
                    detail_lines.push(Line::from(Span::styled(
                        "No variables in this frame",
                        Style::default().fg(Color::DarkGray).italic(),
                    )));
                } else {
                    for (idx, (raw, decoded)) in frame.variables.iter().enumerate() {
                        detail_lines.push(Line::from(vec![
                            Span::styled(
                                format!("[{}] ", idx),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::styled("Raw: ", Style::default().fg(Color::Yellow)),
                            Span::raw(raw),
                        ]));
                        detail_lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled("Decoded: ", Style::default().fg(Color::Green)),
                            Span::styled(decoded, Style::default().fg(Color::Cyan)),
                        ]));
                        detail_lines.push(Line::from(""));
                    }
                }

                detail_lines.push(Line::from(""));
                detail_lines.push(Line::from(Span::styled(
                    "[Enter/Esc/q to close]",
                    Style::default().fg(Color::DarkGray),
                )));

                let popup_area = centered_rect(85, 85, area);

                let block = Block::bordered()
                    .title(Line::from(vec![Span::styled(
                        format!(" Stack Frame {} Details ", frame.index),
                        Style::default().fg(self.colors.header_text).bold(),
                    )]))
                    .border_style(Style::default().fg(Color::Cyan))
                    .style(Style::default().bg(self.colors.background_color));

                let paragraph = Paragraph::new(detail_lines)
                    .block(block)
                    .style(Style::default().fg(self.colors.default_text))
                    .wrap(Wrap { trim: false });

                Clear.render(popup_area, buf);
                paragraph.render(popup_area, buf);
            }
        }

        // Render message detail popup if viewing message details
        if self.state == AppState::AnalysisMessageDetail {
            if self.selected_message_idx < self.messages.len() {
                let message = &self.messages[self.selected_message_idx];

                let detail_lines = vec![
                    Line::from(vec![
                        Span::styled("Message Index: ", Style::default().fg(Color::Yellow).bold()),
                        Span::raw(message.index.to_string()),
                    ]),
                    Line::from(vec![
                        Span::styled("Address: ", Style::default().fg(Color::Yellow).bold()),
                        Span::raw(&message.address),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Raw Value:",
                        Style::default().fg(Color::Yellow).bold(),
                    )),
                    Line::from(message.value_raw.as_str()),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Decoded Value:",
                        Style::default().fg(Color::Green).bold(),
                    )),
                    Line::from(Span::styled(
                        message.value_decoded.as_str(),
                        Style::default().fg(Color::Cyan),
                    )),
                    Line::from(""),
                    Line::from(""),
                    Line::from(Span::styled(
                        "[Enter/Esc/q to close]",
                        Style::default().fg(Color::DarkGray),
                    )),
                ];

                let popup_area = centered_rect(85, 60, area);

                let block = Block::bordered()
                    .title(Line::from(vec![Span::styled(
                        format!(" Message {} Details ", message.index),
                        Style::default().fg(self.colors.header_text).bold(),
                    )]))
                    .border_style(Style::default().fg(Color::Green))
                    .style(Style::default().bg(self.colors.background_color));

                let paragraph = Paragraph::new(detail_lines)
                    .block(block)
                    .style(Style::default().fg(self.colors.default_text))
                    .wrap(Wrap { trim: false });

                Clear.render(popup_area, buf);
                paragraph.render(popup_area, buf);
            }
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
        let preamble_text = app.crash_dump.preamble.lock().unwrap().format();
        let process_count = app.index_map[&Tag::Proc].len();
        let ets_count = app.index_map[&Tag::Ets].len();
        let fn_count = app.index_map[&Tag::Fun].len();

        let memory_info_text = app.crash_dump.memory.lock().unwrap().format();

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
            Span::styled(
                "Process Count: ",
                Style::default().fg(app.colors.info_preamble),
            ),
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

    fn render_process(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        // Always show the normal process list view (removed analysis_pid check)
        // If in fullscreen mode, only show the selected view (Stack/Heap/MessageQueue)
        if app.fullscreen_mode {
            let selected_item;
            {
                let Some(process_table_state) = app.table_states.get(&SelectedTab::Process)
                else {
                    let error_text = Paragraph::new("Error: Process table state not found")
                        .style(Style::default().fg(Color::Red))
                        .alignment(Alignment::Center);
                    Widget::render(&error_text, area, buf);
                    return;
                };
                selected_item = process_table_state.selected().unwrap_or(0);
            }

            if selected_item >= app.tab_lists[&SelectedTab::Process].len() {
                let error_text = Paragraph::new("Error: Invalid process selection")
                    .style(Style::default().fg(Color::Red))
                    .alignment(Alignment::Center);
                Widget::render(&error_text, area, buf);
                return;
            }

            let selected_pid = &app.tab_lists[&SelectedTab::Process][selected_item];

            let (inspect_info_title, inspect_info_text) = match app.process_view_state {
                ProcessViewState::Stack => (
                    "Decoded Stack [S] - Press F to exit fullscreen",
                    app.get_stack_info(selected_pid).unwrap_or_else(|e| {
                        Text::from(format!("Error loading stack: {}", e))
                    }),
                ),
                ProcessViewState::Heap => (
                    "Decoded Heap [H] - Press F to exit fullscreen",
                    app.get_heap_info(selected_pid).unwrap_or_else(|e| {
                        Text::from(format!("Error loading heap: {}", e))
                    }),
                ),
                ProcessViewState::MessageQueue => (
                    "Decoded Message Queue [M] - Press F to exit fullscreen",
                    app.get_message_queue_info(selected_pid)
                        .unwrap_or_else(|e| {
                            Text::from(format!("Error loading message queue: {}", e))
                        }),
                ),
            };

            let proc_view = Paragraph::new(inspect_info_text)
                .block(Block::bordered().title(inspect_info_title))
                .style(Style::default().fg(app.colors.default_text))
                .scroll((app.inspect_scroll, 0))
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Left);

            Widget::render(&proc_view, area, buf);
            return;
        }

        // Normal mode: table on top (50%), details below (50%)
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let selected_item;
        {
            let Some(process_table_state) =
                app.table_states.get_mut(&SelectedTab::Process)
            else {
                let error_text = Paragraph::new("Error: Process table state not found")
                    .style(Style::default().fg(Color::Red))
                    .alignment(Alignment::Center);
                Widget::render(&error_text, area, buf);
                return;
            };
            selected_item = process_table_state.selected().unwrap_or(0);
            StatefulWidget::render(
                &app.process_view_table,
                outer_layout[0],
                buf,
                process_table_state,
            );
        }

        if selected_item >= app.tab_lists[&SelectedTab::Process].len() {
            let error_text = Paragraph::new("Error: Invalid process selection")
                .style(Style::default().fg(Color::Red))
                .alignment(Alignment::Center);
            Widget::render(&error_text, outer_layout[1], buf);
            return;
        }

        let selected_pid = &app.tab_lists[&SelectedTab::Process][selected_item];
        let selected_process_result = app.crash_dump.processes.get(selected_pid);

        // Bottom section: show process details on left (25%), selected view on right (75%)
        let bottom_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(outer_layout[1]);

        let process_info_text: Text;
        let active_proc_info: Option<types::ProcInfo> = match &selected_process_result {
            Some(process_ref) => match *process_ref.value() {
                InfoOrIndex::Info(ref proc_info) => Some(proc_info.clone()),
                _ => None,
            },
            None => None,
        };

        process_info_text = if let Some(ref proc_info) = active_proc_info {
            proc_info.format_as_ratatui_text(&app.colors)
        } else if selected_process_result.is_some() {
            Text::raw(format!("Index for pid: {:?}", selected_pid).to_string())
        } else {
            Text::raw(format!("Process not found: {:?}", selected_pid).to_string())
        };

        let (view_indicator, inspect_info_title, inspect_info_text) = match app.process_view_state
        {
            ProcessViewState::Stack => {
                app.inspecting_pid = selected_pid.clone();
                (
                    "[S]",
                    "Stack",
                    app.get_stack_info(selected_pid).unwrap_or_else(|e| {
                        Text::from(format!("Error loading stack: {}", e))
                    }),
                )
            }
            ProcessViewState::Heap => {
                app.inspecting_pid = selected_pid.clone();
                (
                    "[H]",
                    "Heap",
                    app.get_heap_info(selected_pid).unwrap_or_else(|e| {
                        Text::from(format!("Error loading heap: {}", e))
                    }),
                )
            }
            ProcessViewState::MessageQueue => {
                app.inspecting_pid = selected_pid.clone();
                (
                    "[M]",
                    "Message Queue",
                    app.get_message_queue_info(selected_pid)
                        .unwrap_or_else(|e| {
                            Text::from(format!("Error loading message queue: {}", e))
                        }),
                )
            }
        };

        let detail_block = Paragraph::new(process_info_text)
            .block(Block::bordered().title("Process Details"))
            .style(Style::default().fg(app.colors.default_text))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        let title_with_indicator = format!(
            "{} {} - S/H/M to switch | F for fullscreen",
            view_indicator, inspect_info_title
        );
        let proc_view = Paragraph::new(inspect_info_text)
            .block(Block::bordered().title(title_with_indicator))
            .style(Style::default().fg(app.colors.default_text))
            .scroll((app.inspect_scroll, 0))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        Widget::render(&detail_block, bottom_layout[0], buf);
        Widget::render(&proc_view, bottom_layout[1], buf);
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

        let Some(group_table_state) = app.table_states.get_mut(&SelectedTab::ProcessGroup)
        else {
            let error_text = Paragraph::new("Error: Process group table state not found")
                .style(Style::default().fg(Color::Red))
                .alignment(Alignment::Center);
            Widget::render(&error_text, area, buf);
            return;
        };

        let selected_item = group_table_state.selected().unwrap_or(0);

        if selected_item >= app.tab_lists[&SelectedTab::ProcessGroup].len() {
            let error_text = Paragraph::new("Error: Invalid process group selection")
                .style(Style::default().fg(Color::Red))
                .alignment(Alignment::Center);
            Widget::render(&error_text, area, buf);
            return;
        }

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
            Some(child_pids) => child_pids
                .iter() // Use iter() here as we are just borrowing the child_pids
                .map(|child_pid| match app.crash_dump.processes.get(child_pid) {
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
                })
                .collect(),
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

    fn render_memory(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        // Memory Analysis tab - shows process analysis interface
        // Use P/M/R keys to analyze processes
        self.render_process_analysis(area, buf, app);
    }

    fn render_process_analysis(self, area: Rect, buf: &mut Buffer, app: &mut App) {
        // Don't render analysis data if we're in a special state (prompt or detail views)
        // The popup will be rendered separately over the base view
        if app.state == AppState::AnalysisPrompt
            || app.state == AppState::AnalysisStackDetail
            || app.state == AppState::AnalysisMessageDetail
        {
            // Show a placeholder or last view - the popup will render on top
            // For now, just show help text as placeholder
            let help_text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Analysis Tab",
                    Style::default().fg(Color::DarkGray).bold(),
                )),
            ];
            let help_paragraph = Paragraph::new(help_text)
                .block(Block::bordered().title("Process Analysis"))
                .style(Style::default().fg(app.colors.default_text))
                .alignment(Alignment::Center);
            Widget::render(&help_paragraph, area, buf);
            return;
        }

        // Check if there's legacy analysis_text to display (from M or R commands)
        if !app.analysis_text.is_empty() {
            // Don't add a border - the text already has its own ASCII borders
            let analysis_paragraph = Paragraph::new(app.analysis_text.as_str())
                .style(Style::default().fg(app.colors.default_text))
                .scroll((app.analysis_scroll, 0))
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Left);

            analysis_paragraph.render(area, buf);
            return;
        }

        // Check if we have process data to display
        if app.analysis_pid.is_empty() {
            // Show help message
            let help_text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No process selected for analysis",
                    Style::default().fg(Color::DarkGray).bold(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press P to select a process to analyze",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Use j/k or arrows to scroll | q to quit",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let help_paragraph = Paragraph::new(help_text)
                .block(Block::bordered().title("Process Analysis"))
                .style(Style::default().fg(app.colors.default_text))
                .alignment(Alignment::Center);

            Widget::render(&help_paragraph, area, buf);
            return;
        }

        // Check for error state
        if app.analysis_pid.starts_with("ERROR:") {
            let error_text = Paragraph::new(app.analysis_pid.as_str())
                .block(Block::bordered().title("Process Analysis"))
                .style(Style::default().fg(Color::Red))
                .alignment(Alignment::Center);
            Widget::render(&error_text, area, buf);
            return;
        }

        // Master-Detail layout: Left pane (35%) for categories, Right pane (65%) for details
        let main_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        // Build category list items
        let category_items = vec![
            "▸ Process Info".to_string(),
            format!("▸ Stack ({} frames)", app.stack_frames.len()),
            format!("▸ Messages ({})", app.messages.len()),
            if app.heap_loaded_pids.contains(&app.analysis_pid) {
                "▸ Heap (loaded)".to_string()
            } else {
                "▸ Heap (press H)".to_string()
            },
        ];

        let category_list = ratatui::widgets::List::new(category_items)
            .block(Block::bordered().title(format!("Process: {}", app.analysis_pid)))
            .style(Style::default().fg(app.colors.default_text))
            .highlight_style(Style::default().bg(Color::DarkGray).bold())
            .highlight_symbol("▶ ");

        // Render category list with state
        StatefulWidget::render(
            category_list,
            main_layout[0],
            buf,
            &mut app.analysis_category_state,
        );

        // Render detail pane based on selected category
        match app.analysis_category {
            AnalysisCategory::ProcessInfo => {
                if let Some(ref proc_info) = app.analysis_proc_info {
                    // Build process info table (2 columns: Field | Value)
                    let mut info_rows = Vec::new();

                    if let Some(ref name) = proc_info.name {
                        info_rows.push(Row::new(vec!["Name", name.as_str()]));
                    }
                    info_rows.push(Row::new(vec!["PID", proc_info.pid.as_str()]));
                    info_rows.push(Row::new(vec!["State", proc_info.state.as_str()]));

                    let spawned_as = proc_info.spawned_as.as_deref().unwrap_or("N/A");
                    info_rows.push(Row::new(vec!["Spawned as", spawned_as]));

                    if let Some(ref spawned_by) = proc_info.spawned_by {
                        info_rows.push(Row::new(vec!["Spawned by", spawned_by.as_str()]));
                    }

                    let mq_len = proc_info.message_queue_length.to_string();
                    let reductions = proc_info.reductions.to_string();
                    let memory = types::human_bytes(proc_info.memory);
                    let stack_heap = types::human_bytes(proc_info.stack_heap);
                    let old_heap = types::human_bytes(proc_info.old_heap);
                    let heap_unused = types::human_bytes(proc_info.heap_unused);
                    let old_heap_unused = types::human_bytes(proc_info.old_heap_unused);
                    let bin_vheap = types::human_bytes(proc_info.bin_vheap);
                    let old_bin_vheap = types::human_bytes(proc_info.old_bin_vheap);
                    let bin_vheap_unused = types::human_bytes(proc_info.bin_vheap_unused);
                    let old_bin_vheap_unused = types::human_bytes(proc_info.old_bin_vheap_unused);
                    let total_bin_vheap = types::human_bytes(proc_info.total_bin_vheap);
                    let heap_fragments = proc_info.number_of_heap_fragments.to_string();
                    let heap_fragment_data = types::human_bytes(proc_info.heap_fragment_data);
                    let arity = proc_info.arity.to_string();
                    let pc_str = format!(
                        "{} ({} + {})",
                        proc_info.program_counter.address,
                        proc_info.program_counter.function,
                        proc_info.program_counter.offset
                    );
                    let state_str = proc_info.internal_state.join(" | ");
                    let links_str = proc_info.link_list.join(", ");

                    info_rows.push(Row::new(vec!["Message Queue Len", &mq_len]));
                    info_rows.push(Row::new(vec!["Reductions", &reductions]));
                    info_rows.push(Row::new(vec!["Memory", &memory]));
                    info_rows.push(Row::new(vec!["Stack+Heap", &stack_heap]));
                    info_rows.push(Row::new(vec!["OldHeap", &old_heap]));
                    info_rows.push(Row::new(vec!["Heap Unused", &heap_unused]));
                    info_rows.push(Row::new(vec!["OldHeap Unused", &old_heap_unused]));
                    info_rows.push(Row::new(vec!["BinVHeap", &bin_vheap]));
                    info_rows.push(Row::new(vec!["OldBinVHeap", &old_bin_vheap]));
                    info_rows.push(Row::new(vec!["BinVHeap Unused", &bin_vheap_unused]));
                    info_rows.push(Row::new(vec!["OldBinVHeap Unused", &old_bin_vheap_unused]));
                    info_rows.push(Row::new(vec!["TotalBinVHeap", &total_bin_vheap]));
                    info_rows.push(Row::new(vec!["Heap Fragments", &heap_fragments]));
                    info_rows.push(Row::new(vec!["Heap Fragment Data", &heap_fragment_data]));
                    info_rows.push(Row::new(vec!["Arity", &arity]));
                    info_rows.push(Row::new(vec!["Program Counter", &pc_str]));

                    // Add internal state if present
                    if !proc_info.internal_state.is_empty() {
                        info_rows.push(Row::new(vec!["Internal State", &state_str]));
                    }

                    // Add link list if present
                    if !proc_info.link_list.is_empty() {
                        info_rows.push(Row::new(vec!["Link List", &links_str]));
                    }

                    let info_table = Table::new(
                        info_rows,
                        vec![Constraint::Length(20), Constraint::Min(40)],
                    )
                    .block(Block::bordered().title("Process Information"))
                    .style(Style::default().fg(app.colors.default_text))
                    .header(
                        Row::new(vec!["Field", "Value"])
                            .style(Style::default().bold().fg(app.colors.header_text)),
                    );

                    Widget::render(&info_table, main_layout[1], buf);
                } else {
                    let error_text = Paragraph::new("No process info available")
                        .block(Block::bordered().title("Process Information"))
                        .style(Style::default().fg(Color::Red))
                        .alignment(Alignment::Center);
                    Widget::render(&error_text, main_layout[1], buf);
                }
            }

            AnalysisCategory::Stack => {
                if app.stack_frames.is_empty() {
                    let empty_text = Paragraph::new("No stack frames available")
                        .block(Block::bordered().title("Stack Trace"))
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(Alignment::Center);
                    Widget::render(&empty_text, main_layout[1], buf);
                } else {
                    // Build rows for stack frames table
                    let stack_rows: Vec<Row> = app
                        .stack_frames
                        .iter()
                        .map(|frame| {
                            let idx_str = format!("{}", frame.index);
                            let func_str =
                                format!("{}:{}/{}", frame.module, frame.function, frame.arity);
                            let addr_str = frame.address.clone();
                            let vars_count = format!("{}", frame.variables.len());

                            Row::new(vec![
                                Cell::from(idx_str),
                                Cell::from(func_str),
                                Cell::from(addr_str),
                                Cell::from(vars_count),
                            ])
                        })
                        .collect();

                    // Create TableState for stack selection
                    let mut stack_table_state = ratatui::widgets::TableState::default();
                    stack_table_state.select(Some(app.selected_stack_idx));

                    let stack_table = Table::new(
                        stack_rows,
                        vec![
                            Constraint::Length(6),
                            Constraint::Min(30),
                            Constraint::Length(18),
                            Constraint::Length(6),
                        ],
                    )
                    .block(Block::bordered().title(format!(
                        "Stack Trace ({} frames) - j/k to navigate, Enter to view variables",
                        app.stack_frames.len()
                    )))
                    .style(Style::default().fg(app.colors.default_text))
                    .header(
                        Row::new(vec!["Frame", "Function", "Address", "Vars"])
                            .style(Style::default().bold().fg(app.colors.header_text)),
                    )
                    .row_highlight_style(
                        Style::default()
                            .bg(app.colors.highlight_background)
                            .fg(app.colors.highlight_text),
                    )
                    .highlight_spacing(HighlightSpacing::Always);

                    StatefulWidget::render(&stack_table, main_layout[1], buf, &mut stack_table_state);
                }
            }

            AnalysisCategory::Messages => {
                if app.messages.is_empty() {
                    let empty_text = Paragraph::new("No messages in queue")
                        .block(Block::bordered().title("Message Queue"))
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(Alignment::Center);
                    Widget::render(&empty_text, main_layout[1], buf);
                } else {
                    // Build rows for messages table
                    let message_rows: Vec<Row> = app
                        .messages
                        .iter()
                        .map(|msg| {
                            let idx_str = format!("{}", msg.index);
                            let addr_str = msg.address.clone();
                            let decoded_preview = if msg.value_decoded.len() > 50 {
                                format!("{}...", &msg.value_decoded[..47])
                            } else {
                                msg.value_decoded.clone()
                            };

                            Row::new(vec![
                                Cell::from(idx_str),
                                Cell::from(addr_str),
                                Cell::from(decoded_preview),
                            ])
                        })
                        .collect();

                    // Create TableState for message selection
                    let mut message_table_state = ratatui::widgets::TableState::default();
                    message_table_state.select(Some(app.selected_message_idx));

                    let msg_table = Table::new(
                        message_rows,
                        vec![
                            Constraint::Length(6),
                            Constraint::Length(18),
                            Constraint::Min(40),
                        ],
                    )
                    .block(Block::bordered().title(format!(
                        "Message Queue ({}) - j/k to navigate, Enter to view full message",
                        app.messages.len()
                    )))
                    .style(Style::default().fg(app.colors.default_text))
                    .header(
                        Row::new(vec!["Msg #", "Address", "Value"])
                            .style(Style::default().bold().fg(app.colors.header_text)),
                    )
                    .row_highlight_style(
                        Style::default()
                            .bg(app.colors.highlight_background)
                            .fg(app.colors.highlight_text),
                    )
                    .highlight_spacing(HighlightSpacing::Always);

                    StatefulWidget::render(&msg_table, main_layout[1], buf, &mut message_table_state);
                }
            }

            AnalysisCategory::Heap => {
                if !app.heap_loaded_pids.contains(&app.analysis_pid) {
                    let help_text = vec![
                        Line::from(""),
                        Line::from(Span::styled(
                            "Heap data not loaded",
                            Style::default().fg(Color::Yellow).bold(),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Press H to load heap information",
                            Style::default().fg(Color::DarkGray),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "(Loading heap may take a few seconds)",
                            Style::default().fg(Color::DarkGray).italic(),
                        )),
                    ];

                    let help_paragraph = Paragraph::new(help_text)
                        .block(Block::bordered().title("Heap Data"))
                        .style(Style::default().fg(app.colors.default_text))
                        .alignment(Alignment::Center);

                    Widget::render(&help_paragraph, main_layout[1], buf);
                } else {
                    // Show heap data with scroll instructions
                    let title = format!(
                        "Heap Data (↑↓ or j/k to scroll, line {}/{})",
                        app.analysis_scroll + 1,
                        app.heap_text.lines.len().max(1)
                    );

                    let heap_paragraph = Paragraph::new(app.heap_text.clone())
                        .block(Block::bordered().title(title))
                        .style(Style::default().fg(app.colors.default_text))
                        .scroll((app.analysis_scroll, 0))
                        .wrap(Wrap { trim: false });

                    Widget::render(&heap_paragraph, main_layout[1], buf);
                }
            }
        }
    }

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::General => tailwind::BLUE,
            Self::Process => tailwind::EMERALD,
            Self::ProcessGroup => tailwind::INDIGO,
            Self::Memory => tailwind::ORANGE,
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
