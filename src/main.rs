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

use std::io;

use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    app::{App, AppResult, AppState},
    event::{Event, EventHandler},
    handler::handle_key_events,
    tui::Tui,
    config::CommonColors,
    parser::parser::CDParser,
};
use ratatui::style::Color;


pub mod app;
pub mod config;
pub mod event;
pub mod handler;
mod parser;
pub mod tui;
pub mod ui;

use clap::Parser;

// Analysis helper functions
fn analyze_memory(
    crash_dump: &parser::types::CrashDump,
    group_info_map: &std::collections::HashMap<String, parser::types::GroupInfo>,
    num: usize,
) {
    use parser::types::InfoOrIndex;

    println!("=== MEMORY ANALYSIS ===");
    println!();

    // Print memory information
    println!("{}", crash_dump.memory.lock().unwrap().format());
    println!();

    // Get processes and sort by different memory metrics
    let mut processes: Vec<_> = crash_dump.processes.iter()
        .filter_map(|entry| {
            if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                Some((entry.key().clone(), proc_info.clone()))
            } else {
                None
            }
        })
        .collect();

    // Top processes by total memory
    println!("=== TOP {} PROCESSES BY MEMORY ===", num);
    processes.sort_by(|a, b| b.1.memory.cmp(&a.1.memory));
    for (pid, proc_info) in processes.iter().take(num) {
        println!("{:20} {:30} Memory: {} bytes",
            pid,
            proc_info.name.as_deref().unwrap_or("unnamed"),
            proc_info.memory);
    }
    println!();

    // Top processes by BinVHeap
    println!("=== TOP {} PROCESSES BY BINVHEAP ===", num);
    processes.sort_by(|a, b| b.1.bin_vheap.cmp(&a.1.bin_vheap));
    for (pid, proc_info) in processes.iter().take(num) {
        println!("{:20} {:30} BinVHeap: {} bytes",
            pid,
            proc_info.name.as_deref().unwrap_or("unnamed"),
            proc_info.bin_vheap);
    }
    println!();

    // Top processes by Message Queue Length
    println!("=== TOP {} PROCESSES BY MESSAGE QUEUE LENGTH ===", num);
    processes.sort_by(|a, b| b.1.message_queue_length.cmp(&a.1.message_queue_length));
    for (pid, proc_info) in processes.iter().take(num) {
        println!("{:20} {:30} MsgQ Length: {}",
            pid,
            proc_info.name.as_deref().unwrap_or("unnamed"),
            proc_info.message_queue_length);
    }
    println!();

    // Top process groups by total memory
    println!("=== TOP {} PROCESS GROUPS BY TOTAL MEMORY ===", num);
    let mut groups: Vec<_> = group_info_map.iter().collect();
    groups.sort_by(|a, b| b.1.total_memory_size.cmp(&a.1.total_memory_size));
    for (pid, group_info) in groups.iter().take(num) {
        println!("{:20} {:30} Total Memory: {} bytes (Children: {})",
            pid,
            &group_info.name,
            group_info.total_memory_size,
            group_info.children.len());
    }
}

fn analyze_process(
    crash_dump: &parser::types::CrashDump,
    parser: &CDParser,
    filepath: &str,
    process_id: &str,
) {
    use parser::types::InfoOrIndex;

    // Try to find process by PID first, then by name
    let proc_data = crash_dump.processes.get(process_id)
        .or_else(|| {
            crash_dump.processes.iter().find(|entry| {
                if let InfoOrIndex::Info(ref proc_info) = *entry.value() {
                    proc_info.name.as_deref() == Some(process_id)
                } else {
                    false
                }
            }).map(|entry| entry.pair().0.clone())
                .and_then(|pid| crash_dump.processes.get(&pid))
        });

    match proc_data {
        Some(proc_ref) => {
            if let InfoOrIndex::Info(ref proc_info) = *proc_ref.value() {
                let pid = proc_ref.key();
                println!("=== PROCESS INFORMATION ===");
                println!("{}", proc_info.format());
                println!();

                println!("=== STACK ===");
                if let Ok(stack_text) = parser.get_stack_info(crash_dump, &filepath.to_string(), pid, &config::CommonColors::default()) {
                    for line in stack_text.lines {
                        for span in line.spans {
                            print!("{}", span.content);
                        }
                        println!();
                    }
                }
                println!();

                println!("=== HEAP ===");
                if let Ok(heap_text) = parser.get_heap_info(crash_dump, &filepath.to_string(), pid, &config::CommonColors::default()) {
                    for line in heap_text.lines {
                        for span in line.spans {
                            print!("{}", span.content);
                        }
                        println!();
                    }
                }
                println!();

                println!("=== MESSAGE QUEUE ===");
                if let Ok(msgq_text) = parser.get_message_queue_info(crash_dump, &filepath.to_string(), pid, &config::CommonColors::default()) {
                    for line in msgq_text.lines {
                        for span in line.spans {
                            print!("{}", span.content);
                        }
                        println!();
                    }
                }
            }
        }
        None => {
            println!("Process not found: {}", process_id);
        }
    }
}

fn print_registered_processes(crash_dump: &parser::types::CrashDump) {
    use parser::types::InfoOrIndex;

    println!("=== REGISTERED PROCESSES ===");
    let mut registered: Vec<_> = crash_dump.processes.iter()
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
        println!("{:20} {}", pid, name);
    }
}

fn analyze_context(
    crash_dump: &parser::types::CrashDump,
    parser: &CDParser,
    filepath: &str,
) {
    // Analyze context-specific processes like logging, metrics, monitoring
    let context_processes = vec![
        "wa_system_monitor",
        "wa_logger",
        "wa_metrics",
        "error_logger",
    ];

    println!("=== CONTEXT ANALYSIS ===");
    for process_name in context_processes {
        println!("\n{}", "=".repeat(80));
        analyze_process(crash_dump, parser, filepath, process_name);
    }
}

fn analyze_all(
    crash_dump: &parser::types::CrashDump,
    parser: &CDParser,
    filepath: &str,
    _num: usize,
) {
    use parser::types::InfoOrIndex;

    println!("=== DEEP DIVE ANALYSIS OF ALL PROCESSES ===");
    println!();

    let mut pids: Vec<_> = crash_dump.processes.iter()
        .filter_map(|entry| {
            if let InfoOrIndex::Info(_) = *entry.value() {
                Some(entry.key().clone())
            } else {
                None
            }
        })
        .collect();

    pids.sort();

    for pid in pids {
        println!("\n{}", "#".repeat(100));
        analyze_process(crash_dump, parser, filepath, &pid);
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Action to perform. Should be one of "tui", "analyze"
    /// Default is "tui", which launches the TUI
    /// "analyze" will run CLI analysis modes similar to the Python crash_dump_analyzer
    #[arg(short, long, default_value_t = String::from("tui"))]
    action: String,

    /// Path to the crash dump
    #[arg(required = true)]
    filepath: String,

    /// Analysis mode (for --action=analyze): memory, process, decode, registered, context, all
    #[arg(long)]
    mode: Option<String>,

    /// Argument for the analysis mode (e.g., process name/PID, address to decode, or number)
    #[arg(long)]
    arg: Option<String>,

    /// Turns on light mode
    #[clap(long, short, action)]
    light_mode: bool,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Args::parse();

    if args.action == "tui" {
        // Create an application.

        let colors = if !args.light_mode {
            CommonColors::default()
        } else {
            CommonColors {
                default_text: Color::Black,
                highlight_text: Color::Gray,
                header_text: Color::Black,
                header_background: Color::Yellow,
                highlight_background: Color::DarkGray,
                info_preamble: Color::Blue,
                info_text: Color::Cyan,
                border_color: Color::Gray,
                title_color: Color::Black,
                alt_color_1: Color::Blue,
                alt_color_2: Color::Magenta,
                alt_color_3: Color::Red,
                background_color: Color::White,
            }
        };

        let mut app = App::new(args.filepath, Some(colors))?;

        // Initialize the terminal user interface.
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        let events = EventHandler::new(250);
        let mut tui = Tui::new(terminal, events);
        tui.init()?;

        // Start the main loop.
        while app.state != AppState::Quitting {
            // Render the user interface.
            tui.draw(&mut app)?;
            // Handle events.
            match tui.events.next().await? {
                Event::Tick => app.tick(),
                Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
                Event::Mouse(_) => {}
                Event::Resize(_, _) => {}
            }
        }

        // Exit the user interface.
        tui.exit()?;
    } else if args.action == "analyze" {
        // CLI analysis mode
        let mode = args.mode.as_ref().ok_or("--mode is required for analyze action")?;

        // Create parser and load crash dump
        let parser = CDParser::new(&args.filepath)?;
        let index_map = parser.build_index()?;
        let crash_dump = parser.parse(&index_map)?;
        let ancestor_map = CDParser::create_descendants_table(&crash_dump.processes);
        let group_info_map = CDParser::calculate_group_info(&ancestor_map, &crash_dump.processes);

        match mode.as_str() {
            "memory" => {
                let num = args.arg.as_ref().and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
                analyze_memory(&crash_dump, &group_info_map, num);
            }
            "process" => {
                let process_id = args.arg.as_ref().ok_or("--arg is required for process mode (PID or name)")?;
                analyze_process(&crash_dump, &parser, &args.filepath, process_id);
            }
            "registered" => {
                print_registered_processes(&crash_dump);
            }
            "context" => {
                analyze_context(&crash_dump, &parser, &args.filepath);
            }
            "all" => {
                let num = args.arg.as_ref().and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
                analyze_all(&crash_dump, &parser, &args.filepath, num);
            }
            _ => {
                println!("Unknown analysis mode: {}", mode);
                println!("Available modes: memory, process, registered, context, all");
            }
        }
    } else if args.action == "json" {
        todo!()
    } else {
        println!("Invalid action: {}", args.action);
    }

    Ok(())
}
