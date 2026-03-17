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
    AnalysisCategory, App, AppState, MessageData, ProcessSuggestion, StackFrameData,
};
use crate::parser::types;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span, Text},
};

impl App<'_> {
    pub fn run_memory_analysis(&mut self, num: usize) {
        use types::InfoOrIndex;
        let mut output = String::new();

        output.push_str(
            "╔══════════════════════════════════════════════════════════════════╗\n",
        );
        output.push_str(
            "║                      MEMORY ANALYSIS                             ║\n",
        );
        output.push_str(
            "╚══════════════════════════════════════════════════════════════════╝\n\n",
        );

        // Memory information
        output.push_str(&self.crash_dump.memory.lock().unwrap().format());
        output.push_str("\n\n");

        // Get processes and sort by different memory metrics
        let mut processes: Vec<_> = self
            .crash_dump
            .processes
            .iter()
            .filter_map(|entry| {
                if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                    Some((entry.key().clone(), proc_info.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Top processes by BinVHeap
        output.push_str(
            "┌─────────────────────────────────────────────────────────────────┐\n",
        );
        output.push_str(
            "│ BinVHeap - TOP 10 PROCESSES                                     │\n",
        );
        output.push_str(
            "├─────────────┬────────────────┬────────────────────────┬─────────┤\n",
        );
        output.push_str(
            "│ BinVHeap    │ PID            │ Name                   │ Memory  │\n",
        );
        output.push_str(
            "├─────────────┼────────────────┼────────────────────────┼─────────┤\n",
        );

        processes.sort_by(|a, b| b.1.bin_vheap.cmp(&a.1.bin_vheap));
        for (pid, proc_info) in processes.iter().take(num) {
            output.push_str(&format!(
                "│ {:11} │ {:<14} │ {:<22} │ {:7} │\n",
                types::human_bytes(proc_info.bin_vheap),
                pid,
                proc_info
                    .name
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(22)
                    .collect::<String>(),
                types::human_bytes(proc_info.memory)
            ));
        }
        output.push_str(
            "└─────────────┴────────────────┴────────────────────────┴─────────┘\n\n",
        );

        // Top processes by total memory
        output.push_str(
            "┌─────────────────────────────────────────────────────────────────┐\n",
        );
        output
            .push_str("│ MEMORY - TOP 10 PROCESSES                                       │\n");
        output.push_str(
            "├─────────────┬────────────────┬────────────────────────┬─────────┤\n",
        );
        output.push_str(
            "│ Memory      │ PID            │ Name                   │ BinVHeap│\n",
        );
        output.push_str(
            "├─────────────┼────────────────┼────────────────────────┼─────────┤\n",
        );

        processes.sort_by(|a, b| b.1.memory.cmp(&a.1.memory));
        for (pid, proc_info) in processes.iter().take(num) {
            output.push_str(&format!(
                "│ {:11} │ {:<14} │ {:<22} │ {:7} │\n",
                types::human_bytes(proc_info.memory),
                pid,
                proc_info
                    .name
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(22)
                    .collect::<String>(),
                types::human_bytes(proc_info.bin_vheap)
            ));
        }
        output.push_str(
            "└─────────────┴────────────────┴────────────────────────┴─────────┘\n\n",
        );

        // Top processes by Message Queue Length
        output.push_str(
            "┌─────────────────────────────────────────────────────────────────┐\n",
        );
        output.push_str(
            "│ MESSAGE QUEUE - TOP 10 PROCESSES                                │\n",
        );
        output.push_str(
            "├─────────────┬────────────────┬────────────────────────┬─────────┤\n",
        );
        output.push_str(
            "│ MsgQ Length │ PID            │ Name                   │ Memory  │\n",
        );
        output.push_str(
            "├─────────────┼────────────────┼────────────────────────┼─────────┤\n",
        );

        processes.sort_by(|a, b| b.1.message_queue_length.cmp(&a.1.message_queue_length));
        for (pid, proc_info) in processes.iter().take(num) {
            output.push_str(&format!(
                "│ {:11} │ {:<14} │ {:<22} │ {:7} │\n",
                proc_info.message_queue_length,
                pid,
                proc_info
                    .name
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(22)
                    .collect::<String>(),
                types::human_bytes(proc_info.memory)
            ));
        }
        output.push_str(
            "└─────────────┴────────────────┴────────────────────────┴─────────┘\n\n",
        );

        // Top process groups by total memory
        output.push_str(
            "┌─────────────────────────────────────────────────────────────────┐\n",
        );
        output.push_str(
            "│ PROCESS GROUPS - TOP 10 BY TOTAL MEMORY                         │\n",
        );
        output.push_str(
            "├─────────────┬────────────────┬────────────────────────┬─────────┤\n",
        );
        output.push_str(
            "│ Total Mem   │ PID            │ Name                   │ Children│\n",
        );
        output.push_str(
            "├─────────────┼────────────────┼────────────────────────┼─────────┤\n",
        );

        let mut groups: Vec<_> = self.crash_dump.group_info_map.iter().collect();
        groups.sort_by(|a, b| b.1.total_memory_size.cmp(&a.1.total_memory_size));
        for (pid, group_info) in groups.iter().take(num) {
            output.push_str(&format!(
                "│ {:11} │ {:<14} │ {:<22} │ {:7} │\n",
                types::human_bytes(group_info.total_memory_size),
                pid,
                group_info.name.chars().take(22).collect::<String>(),
                group_info.children.len()
            ));
        }
        output.push_str(
            "└─────────────┴────────────────┴────────────────────────┴─────────┘\n",
        );

        self.analysis_text = output;
        self.analysis_scroll = 0;
    }

    pub fn run_process_analysis(&mut self, process_id: &str) {
        use types::InfoOrIndex;

        // Clear previous analysis
        self.analysis_scroll = 0;
        self.analysis_pid = String::new();
        self.analysis_proc_info = None;
        self.stack_frames.clear();
        self.messages.clear();
        self.selected_stack_idx = 0;
        self.selected_message_idx = 0;

        // Try to find process by PID first, then by name
        let proc_data = self
            .crash_dump
            .processes
            .get(process_id)
            .or_else(|| {
                self.crash_dump
                    .processes
                    .iter()
                    .find(|entry| {
                        if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                            proc_info.name.as_deref() == Some(process_id)
                        } else {
                            false
                        }
                    })
                    .map(|entry| entry.pair().0.clone())
                    .and_then(|pid| self.crash_dump.processes.get(&pid))
            });

        match proc_data {
            Some(proc_ref) => {
                if let InfoOrIndex::Info(ref proc_info) = *proc_ref.value() {
                    let pid = proc_ref.key().clone();

                    // Store process info
                    self.analysis_pid = pid.clone();
                    self.analysis_proc_info = Some(proc_info.clone());

                    // Load stack frames
                    if let Some(stack_info_ref) = self.crash_dump.processes_stack.get(&pid) {
                        if let InfoOrIndex::Index(ref stack_index) = *stack_info_ref.value() {
                            if let Ok(file) =
                                std::fs::OpenOptions::new().read(true).open(&self.filepath)
                            {
                                if let Ok(contents) =
                                    types::CrashDump::load_section(stack_index, &file)
                                {
                                    if let Ok(generic_section) =
                                        contents.parse::<types::GenericSection>()
                                    {
                                        if let Ok(proc_stack) =
                                            types::ProcStackInfo::from_generic_section(
                                                &generic_section,
                                            )
                                        {
                                            self.stack_frames = proc_stack
                                                .frames
                                                .iter()
                                                .enumerate()
                                                .map(|(idx, frame)| {
                                                    let variables: Vec<(String, String)> = frame
                                                        .variables
                                                        .iter()
                                                        .map(|var| {
                                                            let decoded = self
                                                                .crash_dump
                                                                .parse_datatype(var, 0)
                                                                .unwrap_or_else(|_| var.clone());
                                                            (var.clone(), decoded)
                                                        })
                                                        .collect();

                                                    StackFrameData {
                                                        index: idx,
                                                        address: frame.address.clone(),
                                                        return_addr: frame.return_addr.clone(),
                                                        function: frame.function.clone(),
                                                        module: frame.module.clone(),
                                                        offset: frame.offset,
                                                        arity: frame.arity,
                                                        variables,
                                                    }
                                                })
                                                .collect();
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Load messages
                    if proc_info.message_queue_length > 0 {
                        if let Some(mq_index_ref) = self.crash_dump.processes_messages.get(&pid) {
                            if let InfoOrIndex::Index(ref mq_index) = *mq_index_ref.value() {
                                if let Ok(file) =
                                    std::fs::OpenOptions::new().read(true).open(&self.filepath)
                                {
                                    if let Ok(contents) =
                                        types::CrashDump::load_section(mq_index, &file)
                                    {
                                        if let Ok(generic_section) =
                                            contents.parse::<types::GenericSection>()
                                        {
                                            if let Ok(proc_messages) =
                                                types::ProcMessagesInfo::from_generic_section(
                                                    &generic_section,
                                                )
                                            {
                                                self.messages = proc_messages
                                                    .messages
                                                    .iter()
                                                    .enumerate()
                                                    .map(|(idx, (address, value_raw))| {
                                                        let value_decoded = self
                                                            .crash_dump
                                                            .parse_datatype(value_raw, 0)
                                                            .unwrap_or_else(|_| value_raw.clone());

                                                        MessageData {
                                                            index: idx,
                                                            address: address.clone(),
                                                            value_raw: value_raw.clone(),
                                                            value_decoded,
                                                        }
                                                    })
                                                    .collect();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Reset category to ProcessInfo
                    self.analysis_category = AnalysisCategory::ProcessInfo;
                    self.analysis_category_state.select(Some(0));
                }
            }
            None => {
                // Store error state
                self.analysis_pid = format!("ERROR: Process '{}' not found", process_id);
            }
        }
    }

    pub fn load_heap_for_analysis(&mut self) {
        if !self.analysis_pid.is_empty() && !self.heap_loaded_pids.contains(&self.analysis_pid) {
            // Clone the PID to avoid borrow conflicts
            let pid = self.analysis_pid.clone();

            // Load heap data
            match self.get_heap_info(&pid) {
                Ok(heap_text) => {
                    // Convert Text<'a> to Text<'static> by cloning the lines and converting spans
                    let owned_lines: Vec<Line<'static>> = heap_text
                        .lines
                        .into_iter()
                        .map(|line| {
                            let owned_spans: Vec<Span<'static>> = line
                                .spans
                                .into_iter()
                                .map(|span| Span::styled(span.content.to_string(), span.style))
                                .collect();
                            Line::from(owned_spans)
                        })
                        .collect();
                    self.heap_text = Text::from(owned_lines);
                    self.heap_loaded_pids.insert(pid);
                }
                Err(e) => {
                    self.heap_text = Text::from(vec![
                        Line::from(""),
                        Line::from(Span::styled(
                            format!("Error loading heap: {}", e),
                            Style::default().fg(Color::Red),
                        )),
                    ]);
                    // Don't mark as loaded if there was an error
                }
            }
        }
    }

    pub fn start_process_analysis_prompt(&mut self) {
        use types::InfoOrIndex;

        self.state = AppState::AnalysisPrompt;
        self.analysis_prompt_input.clear();
        self.analysis_suggestion_selected = 0;

        // Build list of suggestions (top 50 processes by memory with all columns)
        let mut suggestions: Vec<ProcessSuggestion> = self
            .crash_dump
            .processes
            .iter()
            .filter_map(|entry| {
                if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                    Some(ProcessSuggestion {
                        pid: entry.key().clone(),
                        name: proc_info.name.clone().unwrap_or_default(),
                        memory: proc_info.memory,
                        stack_heap: proc_info.stack_heap,
                        bin_vheap: proc_info.bin_vheap,
                        message_queue_length: proc_info.message_queue_length,
                    })
                } else {
                    None
                }
            })
            .collect();

        suggestions.sort_by(|a, b| b.memory.cmp(&a.memory));

        self.analysis_process_suggestions = suggestions.into_iter().take(50).collect();
    }

    pub fn execute_process_analysis_prompt(&mut self) {
        let input = self.analysis_prompt_input.trim().to_string();

        // If there's input, use it; otherwise use the selected suggestion
        let pid_to_analyze = if !input.is_empty() {
            input
        } else if self.analysis_suggestion_selected < self.analysis_process_suggestions.len() {
            self.analysis_process_suggestions[self.analysis_suggestion_selected]
                .pid
                .clone()
        } else {
            String::new()
        };

        if !pid_to_analyze.is_empty() {
            self.run_process_analysis(&pid_to_analyze);
        }

        self.state = AppState::Running;
        self.analysis_prompt_input.clear();
        self.analysis_suggestion_selected = 0;
    }

    pub fn cancel_process_analysis_prompt(&mut self) {
        self.state = AppState::Running;
        self.analysis_prompt_input.clear();
    }

    pub fn run_registered_analysis(&mut self) {
        use types::InfoOrIndex;
        let mut output = String::new();

        output.push_str("=== REGISTERED PROCESSES ===\n\n");
        let mut registered: Vec<_> = self
            .crash_dump
            .processes
            .iter()
            .filter_map(|entry| {
                if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                    if let Some(ref name) = proc_info.name {
                        if !name.is_empty() {
                            return Some((entry.key().clone(), name.clone()));
                        }
                    }
                }
                None
            })
            .collect();

        registered.sort_by(|a, b| a.1.cmp(&b.1));

        for (pid, name) in registered {
            output.push_str(&format!("{:20} {}\n", pid, name));
        }

        self.analysis_text = output;
        self.analysis_scroll = 0;
    }
}
