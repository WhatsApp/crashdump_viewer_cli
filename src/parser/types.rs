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

///////////////////////////////////////////////////////////////////////////////////////////////////
//
// This file contains the type definitions and parsing logic for the crash dumps.
//
// The general idea is that we have a two-phase parsing approach:
//
// 1. Index Creation:
//    - We scan the crash dump file for section headers (e.g., `=proc:1`).
//    - We record the byte offset and length of each section in an `IndexMap`.
//
// 2. Detailed Crash Dump Creation:
//    - We use the `IndexMap` to locate and parse individual sections.
//    - We use the `InfoOrIndex<T>` type to represent either the parsed information (`Info`) or
//      just the index (`Index`) for a section.
//    - The `from_index_map` function controls whether the parsing is eager or lazy.
//      - If eager, we parse the section immediately and store the `Info`.
//      - If lazy, we store the `Index` and defer parsing until later.
//
// The `parse_section` function defines how a particular section is parsed.
//
// The `from_index_map` function determines when a section is parsed.
//
// This approach allows us to:
// - Avoid loading the entire crash dump into memory at once.
// - Defer parsing of sections until they are actually needed.
// - Support both eager and lazy parsing strategies.
//
///////////////////////////////////////////////////////////////////////////////////////////////////

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::prelude::FileExt;
use std::path::PathBuf;
use crossbeam::channel;
use std::sync::Arc;
use std::thread;
use std::str::FromStr; // Import rayon traits

pub const MAX_DEPTH_PARSE_DATATYPE: usize = 5;
const CHUNK_SIZE: usize = 100; // You can adjust this value as needed

pub const TAG_PREAMBLE: &str = "erl_crash_dump";
pub const TAG_ABORT: &str = "abort";
pub const TAG_ALLOCATED_AREAS: &str = "allocated_areas";
pub const TAG_ALLOCATOR: &str = "allocator";
pub const TAG_ATOMS: &str = "atoms";
pub const TAG_BINARY: &str = "binary";
pub const TAG_DIRTY_CPU_SCHEDULER: &str = "dirty_cpu_scheduler";
pub const TAG_DIRTY_CPU_RUN_QUEUE: &str = "dirty_cpu_run_queue";
pub const TAG_DIRTY_IO_SCHEDULER: &str = "dirty_io_scheduler";
pub const TAG_DIRTY_IO_RUN_QUEUE: &str = "dirty_io_run_queue";
pub const TAG_ENDE: &str = "ende";
pub const TAG_ERL_CRASH_DUMP: &str = "erl_crash_dump";
pub const TAG_ETS: &str = "ets";
pub const TAG_FUN: &str = "fun";
pub const TAG_HASH_TABLE: &str = "hash_table";
pub const TAG_HIDDEN_NODE: &str = "hidden_node";
pub const TAG_INDEX_TABLE: &str = "index_table";
pub const TAG_INSTR_DATA: &str = "instr_data";
pub const TAG_INTERNAL_ETS: &str = "internal_ets";
pub const TAG_LITERALS: &str = "literals";
pub const TAG_LOADED_MODULES: &str = "loaded_modules";
pub const TAG_MEMORY: &str = "memory";
pub const TAG_MEMORY_MAP: &str = "memory_map";
pub const TAG_MEMORY_STATUS: &str = "memory_status";
pub const TAG_MOD: &str = "mod";
pub const TAG_NO_DISTRIBUTION: &str = "no_distribution";
pub const TAG_NODE: &str = "node";
pub const TAG_NOT_CONNECTED: &str = "not_connected";
pub const TAG_OLD_INSTR_DATA: &str = "old_instr_data";
pub const TAG_PERSISTENT_TERMS: &str = "persistent_terms";
pub const TAG_PORT: &str = "port";
pub const TAG_PROC: &str = "proc";
pub const TAG_PROC_DICTIONARY: &str = "proc_dictionary";
pub const TAG_PROC_HEAP: &str = "proc_heap";
pub const TAG_PROC_MESSAGES: &str = "proc_messages";
pub const TAG_PROC_STACK: &str = "proc_stack";
pub const TAG_SCHEDULER: &str = "scheduler";
pub const TAG_TIMER: &str = "timer";
pub const TAG_VISIBLE_NODE: &str = "visible_node";
pub const TAG_END: &str = "end";

// Section tags - lifted from https://github.com/erlang/otp/blob/master/lib/observer/src/crashdump_viewer.erl#L121
#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub enum Tag {
    // Preamble of the crash dump.
    Preamble,
    // Abort information.
    Abort,
    // Allocated memory areas.
    AllocatedAreas,
    // Memory allocator information.
    Allocator,
    // Atom table.
    Atoms,
    // Binary data.
    Binary,
    // Dirty CPU scheduler information.
    DirtyCpuScheduler,
    // Dirty CPU run queue information.
    DirtyCpuRunQueue,
    // Dirty IO scheduler information.
    DirtyIoScheduler,
    // Dirty IO run queue information.
    DirtyIoRunQueue,
    // End of a section.
    Ende,
    // Erlang crash dump header.
    ErlCrashDump,
    // ETS table information.
    Ets,
    // Fun information.
    Fun,
    // Hash table information.
    HashTable,
    // Hidden node information.
    HiddenNode,
    // Index table information.
    IndexTable,
    // Instrumentation data.
    InstrData,
    // Internal ETS table information.
    InternalEts,
    // Literal values.
    Literals,
    // Loaded modules information.
    LoadedModules,
    // Memory usage information.
    Memory,
    // Memory map information.
    MemoryMap,
    // Memory status information.
    MemoryStatus,
    // Module information.
    Mod,
    // No distribution information.
    NoDistribution,
    // Node information.
    Node,
    // Not connected information.
    NotConnected,
    // Old instrumentation data.
    OldInstrData,
    // Persistent terms information.
    PersistentTerms,
    // Port information.
    Port,
    // Process information.
    Proc,
    // Process dictionary information.
    ProcDictionary,
    // Process heap information.
    ProcHeap,
    // Process messages information.
    ProcMessages,
    // Process stack information.
    ProcStack,
    // Scheduler information.
    Scheduler,
    // Timer information.
    Timer,
    // Visible node information.
    VisibleNode,
    // End of a section.
    End,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum DumpSection {
    Preamble(Preamble),
    // Allocator(AllocatorInfo),
    // Node(NodeInfo),
    Proc(ProcInfo),
    // ProcHeap(ProcHeapInfo),
    ProcStack(ProcStackInfo),
    ProcMessages(ProcMessagesInfo),
    // Scheduler(SchedulerInfo),
    // Ets(EtsInfo),
    // Timer(TimerInfo),
    // Port(PortInfo),
    Memory(MemoryInfo),
    // Atoms(Vec<String>),
    // PersistentTerms(PersistentTermInfo),
    // LoadedModules(LoadedModules),
    // Modules(ModuleInfo),
    Generic(GenericSection),
}

/// Generic section that can represent any section in the crash dump.
///
/// This is used as an intermediate representation (IR) before we parse the section into a
/// more specific type.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GenericSection {
    /// Tag of the section (e.g., `proc`, `proc_stack`, etc.).
    tag: String,
    /// Optional identifier for the section (e.g., process ID, binary ID, etc.).
    id: Option<String>,
    data: HashMap<String, String>, // For key-value pairs
    raw_lines: Vec<String>,        // For raw lines without key-value pairs
}

// TODO: once the format is stablized we can implement this trait
// pub trait FromGenericSection: Sized {
//     fn from_generic_section(section: &GenericSection) -> Result<Self, String>;
// }

impl FromStr for GenericSection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut lines = s.lines();

        // Parse the header line
        let header_line = lines.next().ok_or("Missing header line".to_string())?;
        if !header_line.starts_with("=") {
            return Err("Invalid header format".to_string());
        }

        let header_parts: Vec<&str> = header_line[1..].split(":").collect();
        let tag = header_parts.get(0).unwrap().trim().to_string();
        let id = header_parts.get(1).map(|s| s.trim().to_string());

        let mut data = HashMap::new();

        let mut raw_lines = Vec::new();

        if tag == TAG_PROC_STACK {
            // because the stack has repeating keys, in the y registers, we need to parse it different
            for line in lines {
                raw_lines.push(line.to_string());
            }
        } else {
            for line in lines {
                let parts: Vec<&str> = line.splitn(2, ": ").collect();
                if parts.len() == 2 {
                    // key-value pair
                    let key = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();

                    data.insert(key, value);
                } else {
                    // raw line
                    raw_lines.push(line.to_string());
                }
            }
        }

        Ok(GenericSection {
            tag,
            id,
            data,
            raw_lines,
        })
    }
}

pub fn string_tag_to_enum(tag: &str) -> Tag {
    let tag_enum = match tag {
        t if t == TAG_PREAMBLE => Tag::Preamble,
        t if t == TAG_ABORT => Tag::Abort,
        t if t == TAG_ALLOCATED_AREAS => Tag::AllocatedAreas,
        t if t == TAG_ALLOCATOR => Tag::Allocator,
        t if t == TAG_ATOMS => Tag::Atoms,
        t if t == TAG_BINARY => Tag::Binary,
        t if t == TAG_DIRTY_CPU_SCHEDULER => Tag::DirtyCpuScheduler,
        t if t == TAG_DIRTY_CPU_RUN_QUEUE => Tag::DirtyCpuRunQueue,
        t if t == TAG_DIRTY_IO_SCHEDULER => Tag::DirtyIoScheduler,
        t if t == TAG_DIRTY_IO_RUN_QUEUE => Tag::DirtyIoRunQueue,
        t if t == TAG_ENDE => Tag::Ende,
        t if t == TAG_ERL_CRASH_DUMP => Tag::ErlCrashDump,
        t if t == TAG_ETS => Tag::Ets,
        t if t == TAG_FUN => Tag::Fun,
        t if t == TAG_HASH_TABLE => Tag::HashTable,
        t if t == TAG_HIDDEN_NODE => Tag::HiddenNode,
        t if t == TAG_INDEX_TABLE => Tag::IndexTable,
        t if t == TAG_INSTR_DATA => Tag::InstrData,
        t if t == TAG_INTERNAL_ETS => Tag::InternalEts,
        t if t == TAG_LITERALS => Tag::Literals,
        t if t == TAG_LOADED_MODULES => Tag::LoadedModules,
        t if t == TAG_MEMORY => Tag::Memory,
        t if t == TAG_MEMORY_MAP => Tag::MemoryMap,
        t if t == TAG_MEMORY_STATUS => Tag::MemoryStatus,
        t if t == TAG_MOD => Tag::Mod,
        t if t == TAG_NO_DISTRIBUTION => Tag::NoDistribution,
        t if t == TAG_NODE => Tag::Node,
        t if t == TAG_NOT_CONNECTED => Tag::NotConnected,
        t if t == TAG_OLD_INSTR_DATA => Tag::OldInstrData,
        t if t == TAG_PERSISTENT_TERMS => Tag::PersistentTerms,
        t if t == TAG_PORT => Tag::Port,
        t if t == TAG_PROC => Tag::Proc,
        t if t == TAG_PROC_DICTIONARY => Tag::ProcDictionary,
        t if t == TAG_PROC_HEAP => Tag::ProcHeap,
        t if t == TAG_PROC_MESSAGES => Tag::ProcMessages,
        t if t == TAG_PROC_STACK => Tag::ProcStack,
        t if t == TAG_SCHEDULER => Tag::Scheduler,
        t if t == TAG_TIMER => Tag::Timer,
        t if t == TAG_VISIBLE_NODE => Tag::VisibleNode,
        t if t == TAG_END => Tag::End,
        _ => unreachable!(),
    };
    tag_enum
}

fn parse_section(s: &str, id: Option<&str>) -> Result<DumpSection, String> {
    let section = GenericSection::from_str(s)?;
    let id = section.id.clone().unwrap_or_else(|| "".to_string());
    let raw_lines = &section.raw_lines;
    let data = &section.data;

    let section = match string_tag_to_enum(section.tag.as_str()) {
        Tag::Preamble => {
            let preamble = Preamble {
                version: id,
                time: raw_lines[0].clone(),
                slogan: data["Slogan"].clone(),
                erts: data["System version"].clone(),
                taints: data["Taints"].clone(),
                atom_count: data["Atoms"].parse::<i64>().unwrap(),
                calling_thread: data["Calling Thread"].clone(),
            };
            DumpSection::Preamble(preamble)
        }

        Tag::Memory => DumpSection::Memory(MemoryInfo::from_generic_section(&data)),

        Tag::Proc => DumpSection::Proc(ProcInfo::from_generic_section(&section)),

        Tag::ProcStack => {
            DumpSection::ProcStack(ProcStackInfo::from_generic_section(&section).unwrap())
        }

        Tag::ProcMessages => {
            DumpSection::ProcMessages(ProcMessagesInfo::from_generic_section(&section).unwrap())
        }

        _ => DumpSection::Generic(section),
    };
    Ok(section)
}

#[derive(Debug, PartialEq, Clone)] // Added PartialEq for comparison in tests if needed
pub struct IndexRow {
    pub r#type: String, // Use r#type to avoid keyword conflict
    pub id: Option<String>,
    pub start: String,
    pub length: String,
}

// pub type IndexMap = HashMap<Tag, HashMap<Option<String>, IndexRow>>;
#[derive(Debug, Clone)]
pub enum IndexValue {
    Map(HashMap<String, IndexRow>),
    List(Vec<IndexRow>),
}

impl IndexValue {
    pub fn as_map_mut(&mut self) -> Option<&mut HashMap<String, IndexRow>> {
        if let IndexValue::Map(ref mut map) = self {
            Some(map)
        } else {
            None
        }
    }
    pub fn as_list_mut(&mut self) -> Option<&mut Vec<IndexRow>> {
        if let IndexValue::List(ref mut list) = self {
            Some(list)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        match self {
            IndexValue::Map(map) => map.len(),
            IndexValue::List(list) => list.len(),
        }
    }
}

pub type IndexMap = HashMap<Tag, IndexValue>;

#[derive(Debug)]
pub enum InfoOrIndex<T> {
    Index(IndexRow),
    Info(T),
}

#[derive(Debug)]
pub struct CrashDump {
    // physical crash dump sections
    pub preamble: Preamble,
    pub memory: MemoryInfo,
    pub allocators: Vec<InfoOrIndex<AllocatorInfo>>,
    pub nodes: Vec<InfoOrIndex<NodeInfo>>,
    pub processes: HashMap<String, InfoOrIndex<ProcInfo>>,
    pub processes_heap: HashMap<String, InfoOrIndex<ProcHeapInfo>>,
    pub processes_stack: HashMap<String, InfoOrIndex<ProcStackInfo>>,
    pub processes_messages: HashMap<String, InfoOrIndex<ProcMessagesInfo>>,
    pub ports: HashMap<String, InfoOrIndex<PortInfo>>,
    pub schedulers: Vec<InfoOrIndex<SchedulerInfo>>,
    pub ets: Vec<InfoOrIndex<EtsInfo>>,
    pub timers: Vec<InfoOrIndex<TimerInfo>>,
    pub atoms: Vec<InfoOrIndex<String>>,
    pub loaded_modules: Vec<InfoOrIndex<LoadedModules>>,
    pub persistent_terms: Vec<InfoOrIndex<PersistentTermInfo>>,
    /// Raw sections that have not yet been parsed.
    ///
    /// This is used to store sections that we don't know how to parse yet, or sections that
    /// we want to defer parsing until later.
    pub raw_sections: HashMap<String, Vec<u8>>,
    pub group_info_map: HashMap<String, GroupInfo>,

    // derived data
    pub all_heap_addresses: HashMap<String, String>,
    pub all_visited_heap_addresses: HashSet<String>,
    pub visited_binaries: HashMap<String, usize>,
    pub visited_binaries_found: HashMap<String, usize>,
    pub visited_binaries_not_found: HashMap<String, String>,
    pub all_off_heap_binaries: HashMap<String, (usize, usize)>,
}

impl CrashDump {
    pub fn new() -> CrashDump {
        CrashDump {
            preamble: Preamble {
                version: "".to_string(),
                time: "".to_string(),
                slogan: "".to_string(),
                erts: "".to_string(),
                taints: "".to_string(),
                atom_count: 0,
                calling_thread: "".to_string(),
            },
            memory: MemoryInfo {
                total: 0,
                processes: Processes { total: 0, used: 0 },
                system: 0,
                atom: Atom { total: 0, used: 0 },
                binary: 0,
                code: 0,
                ets: 0,
            },
            allocators: vec![],
            nodes: vec![],
            processes: HashMap::new(),
            processes_heap: HashMap::new(),
            processes_stack: HashMap::new(),
            processes_messages: HashMap::new(),
            ports: HashMap::new(),
            schedulers: vec![],
            ets: vec![],
            timers: vec![],
            atoms: vec![],
            loaded_modules: vec![],
            persistent_terms: vec![],
            raw_sections: HashMap::new(),
            group_info_map: HashMap::new(),
            all_heap_addresses: HashMap::new(),
            all_visited_heap_addresses: HashSet::new(),
            visited_binaries: HashMap::new(),
            visited_binaries_found: HashMap::new(),
            visited_binaries_not_found: HashMap::new(),
            all_off_heap_binaries: HashMap::new(),
        }
    }

    pub fn load_section(index_row: &IndexRow, file: &File) -> io::Result<String> {
        let start_offset: u64 = index_row.start.parse().unwrap_or(0);
        let length: u64 = index_row.length.parse().unwrap_or(0);

        let mut buffer = vec![0; length as usize];
        
        // file.seek(SeekFrom::Start(start_offset))?;

        
        // file.read_exact(&mut buffer)?;

        file.read_exact_at(&mut buffer, start_offset)?;

        let contents = String::from_utf8_lossy(&buffer);
        Ok(contents.to_string())
    }

    // pub fn from_index_map_par(index_map: &IndexMap, file_path: &PathBuf) -> io::Result<Self> {
    //     let mut file = File::open(file_path)?;

    //     // 1. Serial Sections
    //     let preamble = CrashDump::load_and_parse_preamble(index_map, &mut file)?;
    //     let memory = CrashDump::load_and_parse_memory(index_map, &mut file)?;

    //     // 2. Parallel Map Processing
    //     let all_heap_addresses = CrashDump::process_all_heap_addresses(index_map, &mut file);
    //     let processes_stack = CrashDump::process_processes_stack(index_map, &mut file)?;
    //     let visited_binaries = CrashDump::process_visited_binaries(index_map, &mut file)?;
    //     let processes_heap = CrashDump::process_processes_heap(index_map, &mut file)?;
    //     let processes = CrashDump::process_processes(index_map, &mut file)?;

    //     // Construct the final CrashDump
    //     Ok(CrashDump {
    //         preamble,
    //         memory,
    //         allocators: vec![], // Initialize other fields as needed
    //         nodes: vec![],
    //         processes,
    //         processes_heap,
    //         processes_stack,
    //         ports: HashMap::new(),
    //         schedulers: vec![],
    //         ets: vec![],
    //         timers: vec![],
    //         atoms: vec![],
    //         loaded_modules: vec![],
    //         persistent_terms: vec![],
    //         raw_sections: HashMap::new(),
    //         group_info_map: HashMap::new(),
    //         all_heap_addresses,
    //         all_visited_heap_addresses: HashSet::new(),
    //         visited_binaries,
    //         visited_binaries_found: HashMap::new(),
    //         visited_binaries_not_found: HashMap::new(),
    //         all_off_heap_binaries: HashMap::new(),
    //     })
    // }

    // fn load_and_parse_preamble(index_map: &IndexMap, file: &mut File) -> io::Result<Preamble> {
    //     if let Some(IndexValue::Map(preamble_map)) = index_map.get(&Tag::Preamble) {
    //         if let Some(index_row) = preamble_map.values().next() {
    //             let contents = CrashDump::load_section(index_row, file)?;
    //             if let Ok(DumpSection::Preamble(preamble)) = parse_section(&contents, None) {
    //                 return Ok(preamble);
    //             }
    //         }
    //     }
    //     Err(io::Error::new(io::ErrorKind::Other, "Preamble not found or invalid"))
    // }

    // fn load_and_parse_memory(index_map: &IndexMap, file: &mut File) -> io::Result<MemoryInfo> {
    //     if let Some(IndexValue::List(memory_list)) = index_map.get(&Tag::Memory) {
    //         if let Some(index_row) = memory_list.first() {
    //             let contents = CrashDump::load_section(index_row, file)?;
    //             if let Ok(DumpSection::Memory(memory)) = parse_section(&contents, None) {
    //                 return Ok(memory);
    //             }
    //         }
    //     }
    //     Err(io::Error::new(io::ErrorKind::Other, "Memory section not found or invalid"))
    // }

    // fn process_all_heap_addresses(index_map: &IndexMap, file: &mut File) -> HashMap<String, String> {
    //     let keys: Vec<_> = index_map
    //         .par_iter()
    //         .filter(|(tag, _)| matches!(tag, Tag::ProcHeap | Tag::PersistentTerms | Tag::Literals))
    //         .flat_map(|(_, index_value)| {
    //             match index_value {
    //                 IndexValue::Map(inner_map) => inner_map.keys().cloned().collect::<Vec<_>>(),
    //                 IndexValue::List(_) => vec![],
    //             }
    //         })
    //         .collect();

    //     keys
    //         .par_chunks(CHUNK_SIZE)
    //         .fold(
    //             || HashMap::new(),
    //             |mut local_map, chunk| {
    //                 for key in chunk {
    //                     // 1. ProcHeap
    //                     if let Some(index_row) = index_map.get(&Tag::ProcHeap).and_then(|v| v.as_map_mut()).and_then(|map| map.get(key)) {
    //                         let contents = CrashDump::load_section(index_row, file).unwrap();
    //                         if let Ok(DumpSection::Generic(generic_section)) = parse_section(&contents, Some(key)) {
    //                             generic_section.raw_lines.into_iter().for_each(|line| {
    //                                 let parts: Vec<&str> = line.splitn(2, ':').collect();
    //                                 if parts.len() == 2 {
    //                                     local_map.insert(parts[0].to_string(), parts[1].to_string());
    //                                 } else {
    //                                     eprintln!("Line does not contain expected delimiter: {}", line);
    //                                 }
    //                             });
    //                         }
    //                     }

    //                     // 2. PersistentTerms
    //                     if let Some(index_row) = index_map.get(&Tag::PersistentTerms).and_then(|v| v.as_list_mut()).and_then(|list| list.first()) {
    //                         let contents = CrashDump::load_section(index_row, file).unwrap();
    //                         if let Ok(DumpSection::Generic(generic_section)) = parse_section(&contents, Some(key)) {
    //                             generic_section.raw_lines.into_iter().for_each(|line| {
    //                                 let parts: Vec<&str> = line.splitn(2, '|').collect();
    //                                 if parts.len() == 2 {
    //                                     local_map.insert(parts[0].to_string(), parts[1].to_string());
    //                                 } else {
    //                                     eprintln!("Line does not contain expected delimiter: {}", line);
    //                                 }
    //                             });
    //                         }
    //                     }

    //                     // 3. Literals
    //                     if let Some(index_row) = index_map.get(&Tag::Literals).and_then(|v| v.as_list_mut()).and_then(|list| list.first()) {
    //                         let contents = CrashDump::load_section(index_row, file).unwrap();
    //                         if let Ok(DumpSection::Generic(generic_section)) = parse_section(&contents, Some(key)) {
    //                             generic_section.raw_lines.into_iter().for_each(|line| {
    //                                 let parts: Vec<&str> = line.splitn(2, ':').collect();
    //                                 if parts.len() == 2 {
    //                                     local_map.insert(parts[0].to_string(), parts[1].to_string());
    //                                 } else {
    //                                     eprintln!("Line does not contain expected delimiter: {}", line);
    //                                 }
    //                             });
    //                         }
    //                     }
    //                 }
    //                 local_map
    //             },
    //         )
    //         .reduce(
    //             || HashMap::new(),
    //             |mut acc, local_map| {
    //                 acc.extend(local_map);
    //                 acc
    //             },
    //         )
    // }

    // fn process_processes_stack(index_map: &IndexMap, file: &mut File) -> io::Result<HashMap<String, InfoOrIndex<ProcStackInfo>>> {
    //     index_map
    //         .get(&Tag::ProcStack)
    //         .and_then(|v| v.as_map_mut())
    //         .map(|inner_map| {
    //             let keys: Vec<_> = inner_map.keys().cloned().collect(); // Collect keys

    //             keys
    //                 .par_chunks(CHUNK_SIZE)
    //                 .fold(
    //                     || HashMap::new(),
    //                     |mut local_map, chunk| {
    //                         for key in chunk {
    //                             if let Some(index_row) = inner_map.get(key) {
    //                                 // Load the ProcStack section
    //                                 let contents = CrashDump::load_section(index_row, file).unwrap();

    //                                 // Parse and process the section
    //                                 if let Ok(DumpSection::ProcStack(proc_stack)) = parse_section(&contents, Some(key)) {
    //                                     local_map.insert(key.clone(), InfoOrIndex::Info(proc_stack));
    //                                 }
    //                             }
    //                         }
    //                         local_map
    //                     },
    //                 )
    //                 .reduce(
    //                     || HashMap::new(),
    //                     |mut acc, local_map| {
    //                         acc.extend(local_map);
    //                         acc
    //                     },
    //                 )
    //         })
    //         .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "ProcStack section not found"))
    // }

    // fn process_visited_binaries(index_map: &IndexMap, file: &mut File) -> io::Result<HashMap<String, usize>> {
    //     index_map
    //         .get(&Tag::Binary)
    //         .and_then(|v| v.as_map_mut())
    //         .map(|inner_map| {
    //             let keys: Vec<_> = inner_map.keys().cloned().collect(); // Collect keys

    //             keys
    //                 .par_chunks(CHUNK_SIZE)
    //                 .fold(
    //                     || HashMap::new(),
    //                     |mut local_map, chunk| {
    //                         for key in chunk {
    //                             if let Some(index_row) = inner_map.get(key) {
    //                                 // No need to load the section, just process the index row
    //                                 if let Some(binary_id) = &index_row.id {
    //                                     let len = index_row.length.parse::<usize>().unwrap_or(0);
    //                                     local_map.insert(binary_id.clone(), len);
    //                                 }
    //                             }
    //                         }
    //                         local_map
    //                     },
    //                 )
    //                 .reduce(
    //                     || HashMap::new(),
    //                     |mut acc, local_map| {
    //                         acc.extend(local_map);
    //                         acc
    //                     },
    //                 )
    //         })
    //         .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Binary section not found"))
    // }

    // fn process_processes_heap(index_map: &IndexMap, file: &mut File) -> io::Result<HashMap<String, InfoOrIndex<ProcHeapInfo>>> {
    //     let chunk_size = 10; // Adjust as needed

    //     index_map
    //         .get(&Tag::ProcHeap)
    //         .and_then(|v| v.as_map_mut())
    //         .map(|inner_map| {
    //             let keys: Vec<_> = inner_map.keys().cloned().collect(); // Collect keys

    //             keys
    //                 .par_chunks(CHUNK_SIZE)
    //                 .fold(
    //                     || HashMap::new(),
    //                     |mut local_map, chunk| {
    //                         for key in chunk {
    //                             if let Some(index_row) = inner_map.get(key) {
    //                                 // Load the ProcHeap section
    //                                 let contents = CrashDump::load_section(index_row, file).unwrap();

    //                                 // Parse and process the section (adapt parsing as needed)
    //                                 if let Ok(DumpSection::Generic(generic_section)) = parse_section(&contents, Some(key)) {
    //                                     // Assuming you have a function to convert GenericSection to ProcHeapInfo
    //                                     let proc_heap_info = ProcHeapInfo::from_generic_section(&generic_section);
    //                                     local_map.insert(key.clone(), InfoOrIndex::Info(proc_heap_info));
    //                                 }
    //                             }
    //                         }
    //                         local_map
    //                     },
    //                 )
    //                 .reduce(
    //                     || HashMap::new(),
    //                     |mut acc, local_map| {
    //                         acc.extend(local_map);
    //                         acc
    //                     },
    //                 )
    //         })
    //         .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "ProcHeap section not found"))
    // }

    // fn process_processes(index_map: &IndexMap, file: &mut File) -> io::Result<HashMap<String, InfoOrIndex<ProcInfo>>> {
    //     index_map
    //         .get(&Tag::Proc)
    //         .and_then(|v| v.as_map_mut())
    //         .map(|inner_map| {
    //             let keys: Vec<_> = inner_map.keys().cloned().collect(); // Collect keys

    //             keys
    //                 .par_chunks(CHUNK_SIZE)
    //                 .fold(
    //                     || HashMap::new(),
    //                     |mut local_map, chunk| {
    //                         for key in chunk {
    //                             if let Some(index_row) = inner_map.get(key) {
    //                                 // Load the Proc section
    //                                 let contents = CrashDump::load_section(index_row, file).unwrap();

    //                                 // Parse and process the section
    //                                 if let Ok(DumpSection::Proc(proc_info)) = parse_section(&contents, Some(key)) {
    //                                     local_map.insert(key.clone(), InfoOrIndex::Info(proc_info));
    //                                 }
    //                             }
    //                         }
    //                         local_map
    //                     },
    //                 )
    //                 .reduce(
    //                     || HashMap::new(),
    //                     |mut acc, local_map| {
    //                         acc.extend(local_map);
    //                         acc
    //                     },
    //                 )
    //         })
    //         .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Proc section not found"))
    // }

    /// Creates a new `CrashDump` from an `IndexMap`.
    ///
    /// This function controls whether the parsing is eager or lazy.
    ///
    /// If eager, we parse the section immediately and store the `Info`.
    ///
    /// If lazy, we store the `Index` and defer parsing until later.
    pub fn from_index_map(index_map: &IndexMap, file_path: &PathBuf) -> io::Result<Self> {
        let mut crash_dump = CrashDump::new();
        let file = File::open(file_path)?;
        let file = Arc::new(file);

        // let mut child_map: HashMap<String, Vec<String>> = HashMap::new();

        let (tx, rx) = channel::unbounded();
        let num_consumers = 20;
        let mut handles = Vec::new();

        // TODO: fixthis with sharded dashmap
        for _ in 0..num_consumers {
            let rx = rx.clone();
            let file = Arc::clone(&file);
            let handle = thread::spawn(move || {
                while let Ok((tag, id, index_row)) = rx.recv() {
                    match tag {
                        Tag::Proc => {
                            let contents = Self::load_section(&index_row, &file).unwrap();
                            if let Ok(DumpSection::Proc(proc)) = parse_section(&contents, Some(&id)) {
                                // dashmap insert
                            }
                        }
                        Tag::ProcHeap => {
                            let contents = Self::load_section(&index_row, &file).unwrap();
                            if let Ok(DumpSection::Generic(proc_heap)) = parse_section(&contents, Some(&id)) {
                                // dashmap insert
                                proc_heap.raw_lines.into_iter().for_each(|line| {
                                    let parts: Vec<&str> = line.splitn(2, ':').collect();
                                    if parts.len() == 2 {
                                        addrs.insert(parts[0].to_string(), parts[1].to_string());
                                    } else {
                                        eprintln!("Line does not contain expected delimiter: {}", line);
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
            handles.push(handle);
        }
        
        // proc, proc heap, literals, persistent terms are probably the slowest to be read
        // proc stack and proc messages we're just cloning some ids, not huge
        for (tag, index_value) in index_map {
            match index_value {
                IndexValue::Map(inner_map) => {
                    for (id, index_row) in inner_map {
                        match tag {
                            Tag::Preamble => {
                                let contents = Self::load_section(&index_row, &file)?;
                                if let Ok(DumpSection::Preamble(preamble)) =
                                    parse_section(&contents, Some(&id))
                                {
                                    crash_dump.preamble = preamble;
                                }
                            }

                            Tag::Proc => {
                                // send the index row and file ref to the processor channel, and then parse it
                                // when the parse is okay, insert all of it at the end

                                tx.send((tag, id.clone(), index_row.clone())).unwrap();
                            }

                            Tag::ProcHeap => {
                                // add only the idx's since we don't need to load them yet
                                crash_dump
                                    .processes_heap
                                    .insert(id.clone(), InfoOrIndex::Index(index_row.clone()));
                                tx.send((tag, id.clone(), index_row.clone())).unwrap();
                            }

                            Tag::ProcStack => {
                                crash_dump
                                    .processes_stack
                                    .insert(id.clone(), InfoOrIndex::Index(index_row.clone()));
                            }

                            Tag::ProcMessages => {
                                crash_dump
                                    .processes_messages
                                    .insert(id.clone(), InfoOrIndex::Index(index_row.clone()));
                            }

                            Tag::Binary => {
                                // binaries are structured like `=binary:FFFF4D7B8C88`, we only need to know the size
                                if let Some(binary_id) = &index_row.id {
                                    let len = index_row.length.parse::<usize>().unwrap_or(0);
                                    crash_dump.visited_binaries.insert(binary_id.clone(), len);
                                }
                            }

                            _ => {}
                        }
                    }
                }
                IndexValue::List(index_rows) => {
                    for index_row in index_rows {
                        match tag {
                            Tag::Memory => {
                                let contents = Self::load_section(&index_row, &file)?;

                                if let Ok(DumpSection::Memory(memory)) =
                                    parse_section(&contents, None)
                                {
                                    crash_dump.memory = memory;
                                }
                            }
                            Tag::PersistentTerms => {
                                // persistent terms are structured like `HFFFF555F6DB0|I6`
                                let contents = Self::load_section(&index_row, &file)?;

                                if let Ok(DumpSection::Generic(persistent_terms)) =
                                    parse_section(&contents, None)
                                {
                                    persistent_terms.raw_lines.into_iter().for_each(|line| {
                                        // Split the line on | and add the addr
                                        let parts: Vec<&str> = line.splitn(2, '|').collect();
                                        if parts.len() == 2 {
                                            crash_dump
                                                .all_heap_addresses
                                                .insert(parts[0].to_string(), parts[1].to_string());
                                        } else {
                                            // Handle the case where the line does not split into two parts
                                            eprintln!(
                                                "Line does not contain expected delimiter: {}",
                                                line
                                            );
                                        }
                                    });
                                }
                            }

                            Tag::Literals => {
                                // Literals are structured like `FFFF55210230:t3:I6,I10,I14`
                                let contents = Self::load_section(&index_row, &file)?;

                                if let Ok(DumpSection::Generic(literals)) =
                                    parse_section(&contents, None)
                                {
                                    literals.raw_lines.into_iter().for_each(|line| {
                                        let parts: Vec<&str> = line.splitn(2, ':').collect();
                                        if parts.len() == 2 {
                                            crash_dump
                                                .all_heap_addresses
                                                .insert(parts[0].to_string(), parts[1].to_string());
                                        } else {
                                            // Handle the case where the line does not split into two parts
                                            eprintln!(
                                                "Line does not contain expected delimiter: {}",
                                                line
                                            );
                                        }
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        drop(tx);
        // Wait for all consumer threads to finish
        for handle in handles {
            handle.join().unwrap();
        }

        println!("handle {:?}", handles);
        Ok(crash_dump)
    }

    // lines will look like `lA1E:jose_xchacha20_poly1305_crypto|HFFFF4541B8B0`
    // lines that have | denote a continuation of another heap addr
    // l is list, A is atom, H is heap, I is integer, Y is binary, E is heap binary
    // if we find a heap addr, increment the depth and continue parsing into the main structure
    // if it's a offheap binary, simply just print it out the length
    // something with multiple

    pub fn load_proc_heap(&self, index_row: &IndexRow, file: &mut File) -> io::Result<Text> {
        let contents = Self::load_section(index_row, file)?;
        let mut text = Text::default();

        match parse_section(&contents, index_row.id.as_deref()) {
            Ok(DumpSection::Generic(proc_heap)) => {
                proc_heap.raw_lines.into_iter().for_each(|line| {
                    let parts: Vec<&str> = line.splitn(2, ':').collect();

                    if parts.len() == 2 {
                        let addr = parts[0];
                        match self.parse_datatype(parts[1], 0) {
                            Ok(parsed_res) => {
                                text.lines.push(Line::from(vec![
                                    Span::styled(
                                        format!("{}", addr),
                                        Style::default().fg(Color::Yellow),
                                    ),
                                    Span::raw(" - "),
                                    Span::styled(
                                        format!("{}", parsed_res),
                                        Style::default().fg(Color::Cyan),
                                    ),
                                ]));
                            }
                            Err(err) => {
                                text.lines.push(Line::from(vec![
                                    Span::styled(
                                        format!("{}", addr),
                                        Style::default().fg(Color::Yellow),
                                    ),
                                    Span::raw(" - "),
                                    Span::styled(
                                        format!("{}", err),
                                        Style::default().fg(Color::Red),
                                    ),
                                ]));
                            }
                        }
                    } else {
                        text.lines.push(Line::from(vec![Span::raw(line)]));
                    }
                });
            }
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Parse error: {}", err),
                ));
            }
            _ => {}
        }
        Ok(text)
    }

    pub fn load_proc_stack(&self, index_row: &IndexRow, file: &mut File) -> io::Result<Text> {
        let contents = Self::load_section(index_row, file)?;
        let mut text = Text::default();
        let mut addr = String::new();

        if let Ok(DumpSection::ProcStack(proc_stack)) =
            parse_section(&contents, index_row.id.as_deref())
        {
            proc_stack.frames.into_iter().for_each(|frame| {
                let mut current_line_variables = Vec::new();
                frame.variables.into_iter().for_each(|variable| {
                    current_line_variables.push(self.parse_datatype(&variable, 0).unwrap());
                });
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}", frame.address),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" - M: "),
                    Span::styled(
                        format!("{}", frame.module),
                        Style::default().fg(Color::Green),
                    ),
                    Span::raw(" F: "),
                    Span::styled(
                        format!("{}", frame.function),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw(" A: ("),
                    Span::styled(
                        current_line_variables.join(","),
                        Style::default().fg(Color::Magenta),
                    ),
                    Span::raw(")"),
                ]);
                // deduplication, if it's the same addr don't add it. Sometimes the frames have weird duplicates
                if addr != frame.address {
                    text.lines.push(line);
                    addr = frame.address.clone();
                }
            });
        }
        Ok(text)
    }

    pub fn load_proc_message_queue(
        &self,
        index_row: &IndexRow,
        file: &mut File,
    ) -> io::Result<Text> {
        let contents = Self::load_section(index_row, file)?;
        let mut text = Text::default();
        if let Ok(DumpSection::ProcMessages(proc_messages)) =
            parse_section(&contents, index_row.id.as_deref())
        {
            proc_messages
                .messages
                .into_iter()
                .for_each(|(message_addr, message_val)| {
                    // set the ADDR to be Yellow and the Value to be Cyan
                    // and try to parse each data type
                    let message_addr = self.parse_datatype(&message_addr, 0).unwrap();
                    let message_val = self.parse_datatype(&message_val, 0).unwrap();
                    let line = Line::from(vec![
                        Span::styled(message_addr, Style::default().fg(Color::Yellow)),
                        Span::raw(" - "),
                        Span::styled(message_val, Style::default().fg(Color::Cyan)),
                    ]);
                    text.lines.push(line);
                });
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Parse error: {}", contents),
            ));
        }
        Ok(text)
    }

    // Parses a data type string from a crash dump.
    //
    // This function recursively parses the input string `data` to interpret the encoded data type.
    // It uses the first character of the string to determine the type and then dispatches to a
    // specific helper function.
    //
    // The `depth` parameter is used to track the recursion depth and prevent infinite loops.
    //
    // # Arguments
    //
    // * `data` - The input string containing the encoded data type.
    // * `depth` - The current recursion depth.
    //
    // # Returns
    //
    // A `Result` containing either a `String` representation of the parsed data type or a `String`
    // describing the error encountered.
    //
    // # Examples
    //
    // ```
    // // Parse an integer:
    // let result = parse_datatype("I123", 0);
    // assert_eq!(result, Ok("123".to_string()));
    //
    // // Parse a tuple:
    // let result = parse_datatype("t2:I1,I2", 0);
    // assert_eq!(result, Ok("{1, 2}".to_string()));
    // ```
    fn parse_datatype(&self, data: &str, depth: usize) -> Result<String, String> {
        if depth > MAX_DEPTH_PARSE_DATATYPE {
            return Ok(format!("(*{})", data));
        }

        let depth = depth + 1;
        // Match the first character of the data string to determine the data type.
        match data.chars().next() {
            Some('t') => self.parse_tuple(data, depth), // Tuple
            Some('A') => Ok(self.parse_atom(data)),     // Atom
            Some('I') => self.parse_int(data).map(|i| i.to_string()), // Integer
            Some('N') => Ok("".to_string()),            // Empty list
            Some('l') => self.parse_list(data, depth),  // List
            Some('H') => self.parse_heap(data, depth),  // Heap reference
            Some('E') => self.parse_encoded_term(data), // Encoded term
            Some('B') => self.parse_bignum(data),       // Bignum
            Some('F') => self.parse_float(data),        // Float
            Some('P') | Some('p') => self.parse_pid(data), // Pid or port
            Some('Y') => self.parse_binary(data),       // Binary
            Some('M') => Ok(format!("M: {}", data)),    // Map (currently returns raw data)
            Some('R') => self.parse_funref(data),       // Fun reference
            Some('S') => Ok(self.parse_string(data)),   // String
            _ => Ok(format!(
                "---don't know how to parse {} at depth {}---",
                data, depth
            )), // Unknown data type
        }
    }

    // for now treat S as a string
    fn parse_string(&self, data: &str) -> String {
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        if parts.len() > 1 {
            parts[1].to_string()
        } else {
            "".to_string()
        }
    }

    fn parse_atom(&self, data: &str) -> String {
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        if parts.len() > 1 {
            parts[1].to_string()
        } else {
            "".to_string()
        }
    }

    // Parses a tuple data type string.
    //
    // This function extracts the tuple size from the input string and then recursively parses the
    // individual elements of the tuple.
    //
    // # Arguments
    //
    // * `data` - The input string containing the encoded tuple.
    // * `depth` - The current recursion depth.
    //
    // # Returns
    //
    // A `Result` containing either a `String` representation of the parsed tuple or a `String`
    // describing the error encountered.
    //
    // # Examples
    //
    // ```
    // let result = parse_tuple("t2:I1,I2", 1);
    // assert_eq!(result, Ok("{1, 2}".to_string()));
    // ```
    fn parse_tuple(&self, data: &str, depth: usize) -> Result<String, String> {
        let mut chars = data.chars();
        chars.next(); // Consume 't'

        let mut size_str = String::new();
        while let Some(c) = chars.next() {
            // hex for the sizes
            if c.is_digit(16) {
                size_str.push(c);
            } else {
                if c == ':' {
                    break;
                } else {
                    return Err(format!("Invalid tuple size format: {}", data));
                }
            }
        }

        let remaining_data = chars.as_str();
        let parts: Vec<&str> = remaining_data.split(',').collect();

        let parsed: Result<Vec<String>, String> = parts
            .iter()
            .map(|x| self.parse_datatype(x, depth))
            .collect();

        let parsed = parsed?;
        Ok(format!("{{{}}}", parsed.join(", ")))
    }

    fn parse_int(&self, data: &str) -> Result<i64, String> {
        let int_str = &data[1..];
        int_str.parse::<i64>().map_err(|e| e.to_string())
    }

    // Parses a list data type string.
    //
    // This function recursively parses the individual elements of the list.
    //
    // # Arguments
    //
    // * `data` - The input string containing the encoded list.
    // * `depth` - The current recursion depth.
    //
    // # Returns
    //
    // A `Result` containing either a `String` representation of the parsed list or a `String`
    // describing the error encountered.
    //
    // # Examples
    //
    // ```
    // let result = parse_list("lHFFFF454383C8|HFFFF45438460|N", 1);
    // assert_eq!(result, Ok("[{1, 2}, {3, 4}]".to_string()));
    // ```
    fn parse_list(&self, data: &str, depth: usize) -> Result<String, String> {
        let parts = data[1..].split('|'); // Remove 'l' and split by '|'
        let parsed: Result<Vec<String>, String> =
            parts.map(|x| self.parse_datatype(x, depth)).collect();

        let parsed = parsed?;
        Ok(format!("[{}]", parsed.join(", ")))
    }

    // Parses a heap reference data type string.
    //
    // This function looks up the heap address in the `all_heap_addresses` map and recursively parses
    // the corresponding data. If the address is not found, it returns a string indicating an
    // unresolved heap reference.
    //
    // # Arguments
    //
    // * `data` - The input string containing the encoded heap reference.
    // * `depth` - The current recursion depth.
    //
    // # Returns
    //
    // A `Result` containing either a `String` representation of the parsed heap data or a `String`
    // describing the error encountered.
    //
    // # Examples
    //
    // ```
    // let result = parse_heap("HFFFF454383C8", 1);
    // assert_eq!(result, Ok("{1, 2}".to_string()));
    // ```
    fn parse_heap(&self, data: &str, depth: usize) -> Result<String, String> {
        let addr = &data[1..]; // Remove 'H'
        if self.all_heap_addresses.contains_key(addr) {
            let heap_data = self.all_heap_addresses.get(addr).unwrap();
            self.parse_datatype(heap_data, depth)
        } else {
            Ok(format!("*U - {}", addr))
        }
    }

    fn parse_bignum(&self, data: &str) -> Result<String, String> {
        // let sign = if data.starts_with("B-") { "-" } else { "" };
        let number_str = if data.starts_with("B16#") || data.starts_with("B-16#") {
            &data[4..] // Skip "B16#" or "B-16#"
        } else {
            &data[1..] // Skip "B"
        };
        Ok(format!("[bignum size: {}]", number_str.len()))
    }

    fn parse_float(&self, data: &str) -> Result<String, String> {
        let parts: Vec<&str> = data[1..].splitn(2, ':').collect(); // Skip 'F'
        if parts.len() != 2 {
            return Err(format!("Invalid float format: {}", data));
        }
        let len_str = parts[0];
        let float_str = parts[1];
        let len: usize = usize::from_str_radix(len_str, 16).map_err(|e| e.to_string())?;
        if len != float_str.len() {
            return Err(format!(
                "Float length mismatch: expected {}, got {}",
                len,
                float_str.len()
            ));
        }
        Ok(format!("[float: {}]", float_str))
    }

    fn parse_pid(&self, data: &str) -> Result<String, String> {
        let prefix = match data.chars().next() {
            Some('P') => "[external pid: ",
            Some('p') => "[external port: ",
            _ => return Err(format!("Invalid pid/port format: {}", data)),
        };
        Ok(format!("{}{}]", prefix, &data[1..]))
    }

    fn parse_binary(&self, data: &str) -> Result<String, String> {
        match &data[1..2] {
            "h" => {
                // Heap binary
                let binary_data = &data[3..]; // Skip "Yh"
                Ok(format!("[heap binary: {}]", binary_data))
            }
            "c" => {
                // Reference-counted binary
                let parts: Vec<&str> = data[2..].split(':').collect(); // Skip "Yc"
                if parts.len() != 3 {
                    return Err(format!("Invalid reference-counted binary format: {}", data));
                }
                let binp0_str = parts[0];
                let offset_str = parts[1];
                let sz_str = parts[2];

                // Parse the values as hexadecimal integers
                let binp0: usize =
                    usize::from_str_radix(binp0_str, 16).map_err(|e| e.to_string())?;
                let offset: usize =
                    usize::from_str_radix(offset_str, 16).map_err(|e| e.to_string())?;
                let sz: usize = usize::from_str_radix(sz_str, 16).map_err(|e| e.to_string())?;

                // Lookup in binary index (using self.visited_binaries)
                let binp_str = format!("{:X}", binp0); // Convert binp0 to hex string
                match self.visited_binaries.get(&binp_str) {
                    Some(len) => {
                        // Found in visited binaries
                        Ok(format!(
                            "[ref-counted binary: binp0=0x{:x}, offset={}, sz={}, len={}]",
                            binp0, offset, sz, len
                        ))
                    }
                    None => {
                        // Not found in visited binaries
                        Ok(format!(
                            "[ref-counted binary: binp0=0x{:x}, offset={}, sz={}, not found]",
                            binp0, offset, sz
                        ))
                    }
                }
            }
            "s" => {
                // Sub binary
                let parts: Vec<&str> = data[2..].split(':').collect(); // Skip "Ys"
                if parts.len() != 3 {
                    return Err(format!("Invalid sub binary format: {}", data));
                }
                let binp0_str = parts[0];
                let offset_str = parts[1];
                let sz_str = parts[2];

                // Parse the values as hexadecimal integers
                let binp0: usize =
                    usize::from_str_radix(binp0_str, 16).map_err(|e| e.to_string())?;
                let offset: usize =
                    usize::from_str_radix(offset_str, 16).map_err(|e| e.to_string())?;
                let sz: usize = usize::from_str_radix(sz_str, 16).map_err(|e| e.to_string())?;

                // Dereference the binary (using self.visited_binaries)
                let binp_str = format!("{:X}", binp0); // Convert binp0 to hex string
                match self.visited_binaries.get(&binp_str) {
                    Some(len) => {
                        // Found in visited binaries
                        let start = offset; // Assuming offset is the start position
                        let end = offset + sz; // Assuming sz is the size of the sub binary
                        if end > *len {
                            return Err(format!(
                                "Sub binary out of bounds: start={}, end={}, len={}",
                                start, end, len
                            ));
                        }
                        Ok(format!(
                            "[sub binary: binp0=0x{:x}, offset={}, sz={}, start={}, end={}]",
                            binp0, offset, sz, start, end
                        ))
                    }
                    None => {
                        // Not found in visited binaries
                        Ok(format!(
                            "[sub binary: binp0=0x{:x}, offset={}, sz={}, not found]",
                            binp0, offset, sz
                        ))
                    }
                }
            }
            _ => Err(format!("Invalid binary type: {}", data)),
        }
    }
    fn parse_map(&self, data: &str, depth: usize) -> Result<String, String> {
        match &data[1..2] {
            "f" => {
                // Flatmap
                let parts: Vec<&str> = data[2..].split(':').collect(); // Skip "Mf"
                if parts.len() < 2 {
                    // At least size and one key-value pair
                    return Err(format!("Invalid flatmap format: {}", data));
                }
                let size_str = parts[0];
                let size: usize = usize::from_str_radix(size_str, 16)
                    .map_err(|e| format!("{:?}, {}", parts, e.to_string()))?;

                // Recursively parse the key-value pairs
                let mut key_value_pairs = Vec::new();
                let mut current_data = parts[1..].join(":"); // Join the remaining parts with ":"
                for _ in 0..size {
                    let parts: Vec<&str> = current_data.splitn(2, ':').collect();
                    if parts.len() != 2 {
                        return Err(format!(
                            "Invalid flatmap key-value pair format: {}",
                            current_data
                        ));
                    }
                    let key_data = parts[0];
                    let value_data = parts[1];
                    let key = self.parse_datatype(key_data, depth + 1)?;
                    let value = self.parse_datatype(value_data, depth + 1)?;
                    key_value_pairs.push(format!("{}: {}", key, value));
                    current_data = value_data.to_string();
                }

                Ok(format!(
                    "[flatmap: size={}, {{{}}}]",
                    size,
                    key_value_pairs.join(", ")
                ))
            }
            "h" => {
                // Hashmap head node
                let parts: Vec<&str> = data[3..].split(':').collect(); // Skip "Mh"
                if parts.len() != 2 {
                    return Err(format!("Invalid hashmap head node format: {}", data));
                }
                let map_size_str = parts[0];
                let n_str = parts[1];
                let map_size: usize =
                    usize::from_str_radix(map_size_str, 16).map_err(|e| e.to_string())?;
                let n: usize = usize::from_str_radix(n_str, 16).map_err(|e| e.to_string())?;

                // Recursively parse the nodes
                let mut nodes = Vec::new();
                let mut current_data = n_str.to_owned();
                for _ in 0..n {
                    let parts: Vec<&str> = current_data.splitn(2, '|').collect();
                    if parts.len() != 2 {
                        return Err(format!("Invalid hashmap node format: {}", current_data));
                    }
                    let node_data = parts[0];
                    let next_data = parts[1];
                    let node = self.parse_datatype(node_data, depth + 1)?;
                    nodes.push(node);
                    current_data = next_data.to_string();
                }

                Ok(format!(
                    "[hashmap head: size={}, nodes=[{}]]",
                    map_size,
                    nodes.join(", ")
                ))
            }
            "n" => {
                // Hashmap interior node
                let parts: Vec<&str> = data[3..].split(':').collect(); // Skip "Mn"
                if parts.len() != 1 {
                    return Err(format!("Invalid hashmap interior node format: {}", data));
                }
                let n_str = parts[0];
                let n: usize = usize::from_str_radix(n_str, 16).map_err(|e| e.to_string())?;

                // Recursively parse the nodes
                let mut nodes = Vec::new();
                let mut current_data = n_str.to_owned();
                for _ in 0..n {
                    let parts: Vec<&str> = current_data.splitn(2, '|').collect();
                    if parts.len() != 2 {
                        return Err(format!("Invalid hashmap node format: {}", current_data));
                    }
                    let node_data = parts[0];
                    let next_data = parts[1];
                    let node = self.parse_datatype(node_data, depth + 1)?;
                    nodes.push(node);
                    current_data = next_data.to_string();
                }

                Ok(format!("[hashmap interior: nodes=[{}]]", nodes.join(", ")))
            }
            _ => Err(format!("Invalid map type: {}", data)),
        }
    }

    fn parse_funref(&self, data: &str) -> Result<String, String> {
        let ref_id = &data[2..]; // Skip "Rf"
        Ok(format!("[fun ref: {}]", ref_id))
    }

    fn parse_encoded_term(&self, data: &str) -> Result<String, String> {
        // Remove the 'E' prefix
        let data = &data[1..];

        // Split the data by ':'
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid heap binary format: {}", data));
        }

        // Extract the length and binary data
        let len_str = parts[0];
        // let binary_data = parts[1];

        // Parse the length as an integer
        let len: usize = usize::from_str_radix(len_str, 16).map_err(|e| e.to_string())?;

        // let decoded_data = match base64::decode(binary_data) {
        //     Ok(data) => data,
        //     Err(e) => return Err(format!("Base64 decode error: {}", e)),
        // };

        // let decoded_str = String::from_utf8_lossy(&decoded_data);

        Ok(format!("<<bin size {}>>", len))
    }

    // fn parse_list(data: &str, depth: usize) -> Result<String, String> {
    //     let mut acc = Vec::new();
    //     let mut truncated = false;
    //     let mut data = data.to_string();
    //     loop {
    //         let rem = &data[1..];
    //         let parts: Vec<&str> = rem.split('|').collect();
    //         let part1 = parts[0];
    //         let p1 = parse_datatype(filename, part1, depth)?;
    //         acc.push(p1);
    //         if parts.len() == 2 {
    //             let part2 = parts[1];
    //             if part2 == "N" {
    //                 break;
    //             }
    //             let address = &part2[1..];
    //             if all_heap_addresses.contains_key(address) {
    //                 visited_heap_addresses.insert(address.to_string());
    //                 data = all_heap_addresses[address].clone();
    //             } else {
    //                 truncated = true;
    //                 break;
    //             }
    //         } else {
    //             break;
    //         }
    //     }
    //     let result = try_string(&acc);
    //     let result = match result {
    //         Ok(s) => s,
    //         Err(list) => {
    //             let mut result = list.join(", ");
    //             if truncated {
    //                 result.push_str("...< heap truncated >");
    //             }
    //             format!("[{}]", result)
    //         }
    //     };
    //     Ok(result)
    // }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Preamble {
    pub version: String,
    pub time: String,
    pub slogan: String,
    pub erts: String,
    pub taints: String,
    pub atom_count: i64,
    pub calling_thread: String,
}

impl Preamble {
    pub fn format(&self) -> String {
        format!(
            "Version: {}\nCrash Dump Created On: {}\nSlogan: {}\nERTS: {}\nTaints: {}\nAtom Count: {}\nCalling Thread: {}",
            self.version, self.time, self.slogan, self.erts, self.taints, self.atom_count, self.calling_thread
        )
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MemoryInfo {
    pub total: i64,
    pub processes: Processes,
    pub system: i64,
    pub atom: Atom,
    pub binary: i64,
    pub code: i64,
    pub ets: i64,
}

impl MemoryInfo {
    pub fn format(&self) -> String {
        format!(
            "Total: {}\nProcesses: {:#?}\nSystem: {}\nAtom: {:#?}\nBinary: {}\nCode: {}\nETS: {}",
            self.total, self.processes, self.system, self.atom, self.binary, self.code, self.ets
        )
    }

    pub fn from_generic_section(data: &HashMap<String, String>) -> Self {
        MemoryInfo {
            total: data["total"].parse().unwrap(),
            processes: Processes {
                total: data["processes"].parse().unwrap(),
                used: data["processes_used"].parse().unwrap(),
            },
            system: data["system"].parse().unwrap(),
            atom: Atom {
                total: data["atom"].parse().unwrap(),
                used: data["atom_used"].parse().unwrap(),
            },
            binary: data["binary"].parse().unwrap(),
            code: data["code"].parse().unwrap(),
            ets: data["ets"].parse().unwrap(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Processes {
    pub total: i64,
    pub used: i64,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Atom {
    pub total: i64,
    pub used: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ProcInfo {
    // The process identifier (PID).
    pub pid: String,
    // The current state of the process (e.g., "running", "waiting").
    pub state: String,
    // The registered name of the process (if any).
    pub name: Option<String>,
    // The initial name of the process when it was spawned.
    pub spawned_as: Option<String>,
    // The PID of the parent process that spawned this process.
    pub spawned_by: Option<String>,
    // The number of messages in the process's message queue.
    pub message_queue_length: i64,
    // The number of heap fragments used by the process.
    pub number_of_heap_fragments: i64,
    // The total size of heap fragment data used by the process (in bytes).
    pub heap_fragment_data: i64,
    // A list of PIDs representing processes linked to this process.
    pub link_list: Vec<String>,
    // The number of reductions performed by the process.
    pub reductions: i64,
    // The combined size of the stack and heap memory used by the process (in bytes).
    pub stack_heap: i64,
    // The size of the old heap memory used by the process (in bytes).
    pub old_heap: i64,
    // The amount of unused heap memory allocated to the process (in bytes).
    pub heap_unused: i64,
    // The amount of unused old heap memory allocated to the process (in bytes).
    pub old_heap_unused: i64,
    // The size of the binary virtual heap used by the process (in bytes).
    pub bin_vheap: i64,
    // The size of the old binary virtual heap used by the process (in bytes).
    pub old_bin_vheap: i64,
    // The amount of unused binary virtual heap memory allocated to the process (in bytes).
    pub bin_vheap_unused: i64,
    // The amount of unused old binary virtual heap memory allocated to the process (in bytes).
    pub old_bin_vheap_unused: i64,
    // The total size of the binary virtual heap used by the process (in bytes), including both current and old.
    pub total_bin_vheap: i64,
    // The total memory used by the process (in bytes).
    pub memory: i64,
    // The arity of the current function being executed by the process.
    pub arity: i64,
    // Information about the current program counter of the process.
    pub program_counter: ProgramCounter,
    // A list of strings representing the internal state of the process.
    pub internal_state: Vec<String>,
}

impl ProcInfo {
    pub fn format_as_ratatui_text(&self) -> Text {
        // format as a ratatui text, composed of different lines. Each value should have a colorized key and values
        // key should be yellow, value should be cyan
        let mut text = Text::default();

        text.lines.push(Line::from(vec![
            Span::styled("Pid: ", Style::default().fg(Color::Yellow)),
            Span::styled(self.pid.clone(), Style::default().fg(Color::Cyan)),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("State: ", Style::default().fg(Color::Yellow)),
            Span::styled(self.state.clone(), Style::default().fg(Color::Cyan)),
        ]));

        if let Some(name) = &self.name {
            text.lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Yellow)),
                Span::styled(name.clone(), Style::default().fg(Color::Cyan)),
            ]));
        }

        if let Some(spawned_as) = &self.spawned_as {
            text.lines.push(Line::from(vec![
                Span::styled("Spawned As: ", Style::default().fg(Color::Yellow)),
                Span::styled(spawned_as.clone(), Style::default().fg(Color::Cyan)),
            ]));
        }

        if let Some(spawned_by) = &self.spawned_by {
            text.lines.push(Line::from(vec![
                Span::styled("Spawned By: ", Style::default().fg(Color::Yellow)),
                Span::styled(spawned_by.clone(), Style::default().fg(Color::Cyan)),
            ]));
        }

        text.lines.push(Line::from(vec![
            Span::styled("Message Queue Length: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.message_queue_length),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled(
                "Number of Heap Fragments: ",
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{}", self.number_of_heap_fragments),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Heap Fragment Data: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.heap_fragment_data),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Reductions: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.reductions),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Stack Heap: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.stack_heap),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Old Heap: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.old_heap),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Heap Unused: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.heap_unused),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Old Heap Unused: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.old_heap_unused),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Bin Vheap: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.bin_vheap),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Old Bin Vheap: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.old_bin_vheap),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Bin Vheap Unused: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.bin_vheap_unused),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Old Bin Vheap Unused: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", self.old_bin_vheap_unused),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Memory: ", Style::default().fg(Color::Yellow)),
            Span::styled(format!("{}", self.memory), Style::default().fg(Color::Cyan)),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Arity: ", Style::default().fg(Color::Yellow)),
            Span::styled(format!("{}", self.arity), Style::default().fg(Color::Cyan)),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Program Counter: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:?}", self.program_counter),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text.lines.push(Line::from(vec![
            Span::styled("Internal State: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:?}", self.internal_state),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        text
    }

    pub fn format(&self) -> String {
        format!(
            "Pid: {}\nState: {}\nName: {:#?}\nSpawned As: {:#?}\nSpawned By: {:#?}\nMessage Queue Length: {}\nNumber of Heap Fragments: {}\nHeap Fragment Data: {}\nLink List: {:#?}\nReductions: {}\nStack Heap: {}\nOld Heap: {}\nHeap Unused: {}\nOld Heap Unused: {}\nBin Vheap: {}\nOld Bin Vheap: {}\nBin Vheap
Unused: {}\nOld Bin Vheap Unused: {}\nMemory: {}\nArity: {}\n{:#?}\nInternal State: {:#?}",
            self.pid, self.state, self.name, self.spawned_as, self.spawned_by, self.message_queue_length, self.number_of_heap_fragments, self.heap_fragment_data, self.link_list, self.reductions, self.stack_heap, self.old_heap, self.heap_unused, self.old_heap_unused, self.bin_vheap, self.old_bin_vheap,
            self.bin_vheap_unused, self.old_bin_vheap_unused, self.memory, self.arity,
            self.program_counter, self.internal_state
        )
    }
    pub fn headers() -> [&'static str; 9] {
        [
            "OldBinVHeap",
            "Pid",
            "Name",
            "Memory",
            "TotalBinVHeap",
            "BinVHeap",
            "BinVHeap unused",
            "OldBinVHeap",
            "OldBinVHeap Unused",
        ]
    }

    pub fn ref_array(&self) -> [String; 9] {
        [
            format!("{}", self.old_bin_vheap),
            self.pid.clone(),
            self.name.clone().unwrap_or_default(),
            format!("{}", self.memory),
            format!("{}", self.bin_vheap + self.old_bin_vheap),
            format!("{}", self.bin_vheap),
            format!("{}", self.bin_vheap_unused),
            format!("{}", self.old_bin_vheap),
            format!("{}", self.old_bin_vheap_unused),
        ]
    }

    pub fn summary_ref_array(&self) -> [String; 5] {
        [
            self.pid.clone(),
            self.name.clone().unwrap_or_default(),
            format!("{}", self.memory),
            format!("{}", self.reductions),
            format!("{}", self.message_queue_length),
        ]
    }

    pub fn from_generic_section(section: &GenericSection) -> Self {
        let id = section.id.clone().unwrap_or_default();
        let raw_lines = &section.raw_lines;
        let data = &section.data;

        let link_list: Vec<String> = data
            .get("Link list")
            .map(|s| {
                s.trim_matches(|c| c == '[' || c == ']')
                    .split(", ")
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let internal_state: Vec<String> = data
            .get("Internal State")
            .map(|s| s.split(" | ").map(String::from).collect())
            .unwrap_or_default();

        let program_counter: Option<ProgramCounter> = data
            .get("Program counter")
            .and_then(|s| ProgramCounter::from_string(s));

        let state = data.get("State").cloned().unwrap_or_default();
        let name = data.get("Name").cloned().unwrap_or_default();
        let spawned_as = data.get("Spawned as").cloned().filter(|s| !s.is_empty());
        let spawned_by = data.get("Spawned by").cloned().filter(|s| !s.is_empty());

        let message_queue_length = data
            .get("Message queue length")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let number_of_heap_fragments = data
            .get("Number of heap fragments")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let heap_fragment_data = data
            .get("Heap fragment data")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let reductions = data
            .get("Reductions")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let stack_heap = data
            .get("Stack+heap")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let old_heap = data
            .get("OldHeap")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let heap_unused = data
            .get("Heap unused")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let old_heap_unused = data
            .get("OldHeap unused")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let bin_vheap = data
            .get("BinVHeap")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let old_bin_vheap = data
            .get("OldBinVHeap")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let memory = data
            .get("Memory")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let bin_vheap_unused = data
            .get("BinVHeap unused")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let old_bin_vheap_unused = data
            .get("OldBinVHeap unused")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let arity = raw_lines
            .get(0)
            .and_then(|line| line.split('=').last())
            .map(|s| s.trim())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let mut proc = ProcInfo {
            pid: id,
            state,
            name: Some(name),
            spawned_as,
            spawned_by,
            message_queue_length,
            number_of_heap_fragments,
            heap_fragment_data,
            link_list,
            program_counter: program_counter.unwrap_or_default(),
            reductions,
            stack_heap,
            old_heap,
            heap_unused,
            old_heap_unused,
            bin_vheap,
            old_bin_vheap,
            memory,
            bin_vheap_unused,
            old_bin_vheap_unused,
            total_bin_vheap: 0,
            arity,
            internal_state,
        };

        proc.total_bin_vheap = proc.bin_vheap + proc.old_bin_vheap;
        proc
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct GroupInfo {
    pub total_heap_size: i64,
    pub total_binary_size: i64,
    pub total_memory_size: i64,
    pub pid: String,
    pub name: String,
    pub children: Vec<String>,
}

impl GroupInfo {
    pub fn format(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.total_memory_size,
            self.pid,
            self.name,
            self.children.len()
        )
    }
    pub fn headers() -> [&'static str; 4] {
        ["Total Memory Size", "Pid", "Name", "Children Count"]
    }
    pub fn ref_array(&self) -> [String; 4] {
        [
            format!("{}", self.total_memory_size),
            self.pid.clone(),
            self.name.clone(),
            format!("{}", self.children.len()),
        ]
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct ProgramCounter {
    pub address: String,
    pub function: String,
    pub offset: i64,
    pub arity: i32,
}

impl ProgramCounter {
    pub fn from_string(s: &str) -> Option<Self> {
        let re =
            Regex::new(r"Program counter: (0x[0-9a-fA-F]+) \(([^:]+):([^/]+)/(\d+) \+ (\d+)\)")
                .unwrap();
        re.captures(s).map(|caps| ProgramCounter {
            address: caps[1].to_string(),
            function: caps[2].to_string(),
            offset: caps[5].parse().unwrap_or_default(),
            arity: caps[4].parse().unwrap_or_default(),
        })
    }
}

#[derive(Debug)]
pub struct ProcHeapInfo {
    pub pid: String,
    pub entries: HashMap<String, HeapEntry>,
}
#[derive(Debug)]
pub struct HeapEntry {
    pub address: String,
    pub type_: String,
    pub contents: Vec<Value>,
    pub raw: String,
}
#[derive(Debug)]
pub struct Value {
    pub type_: ValueType,
    pub raw: String,
    pub integer: Option<i64>,
    pub atom: Option<String>,
    pub heap_ref: Option<String>,
}
// Enum representing the type of a value in a heap entry.
#[derive(Debug)]
pub enum ValueType {
    // Integer value.
    IntegerValue,
    // Atom value.
    AtomValue,
    // Heap reference value.
    HeapRefValue,
    // Process identifier (PID) value.
    PidValue,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct ProcStackInfo {
    pub pid: String,
    pub frames: Vec<StackFrame>,
}

impl ProcStackInfo {
    pub fn from_generic_section(section: &GenericSection) -> Result<Self, String> {
        if section.tag != "proc_stack" {
            return Err("Not a proc_stack section".to_string());
        }
        let mut total_frames = Vec::new();
        let mut current_frame_args = Vec::new();

        let mut frame_address = String::new();
        let mut return_addr = String::new();
        let mut function = String::new();
        let mut module = String::new();
        let mut offset = 0;
        let mut arity = 0;

        // More generic regex to capture function info.
        let re_func_info = Regex::new(
            r"^(?P<address>0x[0-9A-Fa-f]+):S(?:Return addr|Catch)\s+(?P<retaddr>0x[0-9A-Fa-f]+)\s+\((?P<module>[^:]+):(?P<function>[^/]+)/(?P<arity>\d+)\s*\+\s*(?P<offset>\d+)\)",
        ).unwrap();
        // Regex to capture cases where there isn't a module e.g. <terminate process normally>
        let re_no_module = Regex::new(
            r"^(?P<address>0x[0-9A-Fa-f]+):S(?:Return addr|Catch)\s+(?P<retaddr>0x[0-9A-Fa-f]+)\s+\((?P<function><[^>]+>)\)",
        ).unwrap();

        let mut current_frame = StackFrame {
            address: frame_address.clone(),
            return_addr: return_addr.clone(),
            function: function.clone(),
            module: module.clone(),
            offset,
            arity,
            variables: current_frame_args.clone(),
        };

        for line in &section.raw_lines {
            // iterate through the lines, decoding and collecting the values as you go
            // once we hit a 0x line, we have a frame, so pop whatever we had before that into a string
            if line.starts_with("y") {
                let parts: Vec<&str> = line.splitn(2, ":").collect();
                if parts.len() == 2 {
                    let arg_value = parts[1].trim().to_string();
                    current_frame_args.push(arg_value);
                }
            } else if line.starts_with("0x") {
                // push the previous frame into the total_frames vector
                current_frame.variables = current_frame_args.clone(); // Add the arguments to the current frame
                total_frames.push(current_frame.clone()); // Push the current frame into the total_frames vector
                current_frame_args = Vec::new(); // Reset the current frame arguments

                // Try the no-module regex first
                if let Some(caps) = re_no_module.captures(line) {
                    frame_address = caps.name("address").unwrap().as_str().to_string();
                    return_addr = caps.name("retaddr").unwrap().as_str().to_string();
                    function = caps.name("function").unwrap().as_str().to_string();
                    // No module, arity, or offset in this case.
                } else if let Some(caps) = re_func_info.captures(line) {
                    frame_address = caps.name("address").unwrap().as_str().to_string();
                    return_addr = caps.name("retaddr").unwrap().as_str().to_string();
                    function = caps.name("function").unwrap().as_str().to_string();
                    module = caps.name("module").unwrap().as_str().to_string();
                    arity = caps
                        .name("arity")
                        .unwrap()
                        .as_str()
                        .parse::<usize>()
                        .unwrap();
                    offset = caps
                        .name("offset")
                        .unwrap()
                        .as_str()
                        .parse::<usize>()
                        .unwrap();
                }

                current_frame = StackFrame {
                    address: frame_address.clone(),
                    return_addr: return_addr.clone(),
                    function: function.clone(),
                    module: module.clone(),
                    offset,
                    arity,
                    variables: current_frame_args.clone(),
                };
            }
        }

        Ok(ProcStackInfo {
            pid: section.id.clone().unwrap(),
            frames: total_frames,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct ProcMessagesInfo {
    pub pid: String,
    pub messages: HashMap<String, String>,
}

// ProcMessages are arranged with <ADDR>:<VALUE> format, we can just parse .data
impl ProcMessagesInfo {
    fn from_generic_section(section: &GenericSection) -> Result<Self, String> {
        if section.tag != TAG_PROC_MESSAGES {
            return Err("Not a proc_messages section".to_string());
        }
        let mut messages = HashMap::new();
        section.raw_lines.iter().for_each(|line| {
            let parts: Vec<&str> = line.splitn(2, ":").collect();
            if parts.len() == 2 {
                messages.insert(parts[0].to_string(), parts[1].to_string());
            }
        });

        Ok(ProcMessagesInfo {
            pid: section.id.clone().unwrap(),
            messages,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct StackFrame {
    // The values of the variables in the current stack frame.
    pub variables: Vec<String>,
    // The address of the stack frame (the 0x... line).
    pub address: String,
    // The return address of the stack frame.
    pub return_addr: String,
    // The name of the function being executed in the stack frame.
    pub function: String,
    // The name of the module containing the function.
    pub module: String,
    // The offset within the function.
    pub offset: usize,
    // The arity of the function.
    pub arity: usize,
}

// the following structs are not strictly parsed yet, although support for them is ongoing

// Allocator information (currently unimplemented).
//
// This struct will eventually hold information about the memory allocator used by the Erlang VM.
#[derive(Debug)]
pub struct AllocatorInfo {
    pub name: String,
    pub version: String,
    pub options: HashMap<String, String>,
    pub mbcs_blocks: HashMap<String, BlockInfo>,
    pub mbcs_carriers: MBCSCarriers,
    pub sbcs_blocks: HashMap<String, BlockInfo>,
    pub sbcs_carriers: SBCSCarriers,
    pub calls: Calls,
}
#[derive(Debug)]
pub struct MBCSCarriers {
    pub count: i64,
    pub mseg_count: i64,
    pub sys_alloc_count: i64,
    pub size: [i64; 3],
    pub mseg_size: i64,
    pub sys_alloc_size: i64,
}
#[derive(Debug)]
pub struct SBCSCarriers {
    pub count: i64,
    pub mseg_count: i64,
    pub sys_alloc_count: i64,
    pub size: [i64; 3],
    pub mseg_size: i64,
    pub sys_alloc_size: i64,
}
#[derive(Debug)]
pub struct Calls {
    pub alloc: i64,
    pub free: i64,
    pub realloc: i64,
    pub mseg_alloc: i64,
    pub mseg_dealloc: i64,
    pub mseg_realloc: i64,
    pub sys_alloc: i64,
    pub sys_free: i64,
    pub sys_realloc: i64,
}
#[derive(Debug)]
pub struct BlockInfo {
    pub count: [i64; 3],
    pub size: [i64; 3],
}

// Node information (currently unimplemented).
//
// This struct will eventually hold information about the Erlang node involved in the crash.
#[derive(Debug)]
pub struct NodeInfo {
    pub name: String,
    pub type_: String,
    pub status: String,
}

#[derive(Debug)]
pub struct SchedulerInfo {
    pub id: i32,
    pub sleep_info: SleepInfo,
    pub current_port: String,
    pub run_queue: RunQueue,
    pub current_process: CurrentProcess,
}
#[derive(Debug)]
pub struct SleepInfo {
    pub flags: Vec<String>,
    pub aux_work: Vec<String>,
}
#[derive(Debug)]
pub struct RunQueue {
    pub max_length: i32,
    pub high_length: i32,
    pub normal_length: i32,
    pub low_length: i32,
    pub port_length: i32,
    pub flags: Vec<String>,
}
#[derive(Debug)]
pub struct CurrentProcess {
    pub pid: String,
    pub state: String,
    pub internal_state: Vec<String>,
    pub program_counter: ProgramCounter,
    pub stack_trace: Vec<StackFrame>,
}
#[derive(Debug)]
pub struct EtsInfo {
    pub pid: String,
    pub slot: i64,
    pub table: String,
    pub name: String,
    pub buckets: i32,
    pub chain_length: ChainLength,
    pub fixed: bool,
    pub objects: i64,
    pub words: i64,
    pub type_: String,
    pub protection: String,
    pub compressed: bool,
    pub write_concurrency: bool,
    pub read_concurrency: bool,
}
#[derive(Debug)]
pub struct ChainLength {
    pub avg: f64,
    pub max: i32,
    pub min: i32,
    pub std_dev: f64,
    pub expected_std_dev: f64,
}
#[derive(Debug)]
pub struct TimerInfo {
    pub pid: String,
    pub message: serde_json::Value,
    pub time_left: i64,
}
#[derive(Debug)]
pub struct LoadedModules {
    pub current_code: i64,
    pub old_code: i64,
    pub modules: Vec<ModuleInfo>,
}
#[derive(Debug)]
pub struct ModuleInfo {
    pub name: String,
    pub current_size: i64,
}
#[derive(Debug)]
pub struct PersistentTermInfo {
    pub terms: Vec<PersistentTerm>,
}
#[derive(Debug)]
pub struct PersistentTerm {
    pub address: String,
    pub value: Value,
}
#[derive(Debug)]
pub struct PortInfo {
    pub id: String,
    pub state: Vec<String>,
    pub slot: i64,
    pub connected: String,
    pub links: Vec<String>,
    pub registered_as: String,
    pub external_process: String,
    pub input: i64,
    pub output: i64,
    pub queue: i64,
}
