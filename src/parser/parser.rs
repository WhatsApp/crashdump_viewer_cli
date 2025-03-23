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

//! This module provides the `CDParser` struct, which is responsible for parsing crash dump files.
//! The parser uses a combination of regex matching and byte offset indexing to efficiently extract
//! information from the crash dump.

use crate::parser::*;
use grep::{
    regex::RegexMatcher,
    searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch},
};
// use rayon::prelude::*;
use dashmap::DashMap;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

struct IndexSink {
    matches: Vec<(Tag, Option<String>, u64)>,
}

impl IndexSink {
    fn new() -> Self {
        Self {
            matches: Vec::new(),
        }
    }
}

impl Sink for IndexSink {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, match_: &SinkMatch) -> Result<bool, Self::Error> {
        let byte_offset = match_.absolute_byte_offset();
        let match_bytes = match_.bytes();
        if match_bytes.starts_with(b"=") {
            let tag_end = match_bytes
                .iter()
                .position(|&x| x == b':')
                .unwrap_or(match_bytes.len() - 1);

            let tag = &match_bytes[1..tag_end];

            // println!("tag: {:?}", std::str::from_utf8(tag).unwrap());
            let tag_enum = types::string_tag_to_enum(std::str::from_utf8(tag).unwrap());

            let tag_id_string = if match_bytes.len() > tag_end + 1 {
                let tag_id_cow = String::from_utf8_lossy(&match_bytes[tag_end + 1..]);
                Some(tag_id_cow.trim().to_string())
            } else {
                None
            };

            self.matches.push((tag_enum, tag_id_string, byte_offset));
        }
        Ok(true)
    }
}

#[derive(Default, Debug)]
pub struct CDParser {
    //file: File,
    // mmap: Mmap,
    filepath: PathBuf,
    filename: String,
    index: Vec<String>,
}

impl CDParser {
    /// Creates a new `CDParser` instance.
    ///
    /// # Arguments
    ///
    /// * `filepath` - The path to the crash dump file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or the path is invalid.
    pub fn new(filepath: &str) -> Result<Self, io::Error> {
        let (filepath, filename) = Self::split_path_and_filename(filepath)?;
        let realpath = filepath.join(&filename);

        // TODO: add mmap support

        Ok(CDParser {
            //file,
            // mmap,
            filepath: realpath,
            filename,
            index: Vec::new(),
        })
    }

    /// Builds an index of the crash dump file.
    /// The index maps section tags (e.g., `=proc:<0.1.0>`, `=HEAP_INFO:<0.1.0>`) to their corresponding byte offsets and lengths
    /// within the file. This allows for efficient retrieval of specific sections later.
    ///
    /// The index is built by searching the file for lines starting with "=" using a regex.
    /// Each match is parsed to extract the tag and optional ID, which are then used to populate the `IndexMap`.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the `IndexMap` if successful, or an `io::Error` if an error occurred during file processing.
    pub fn build_index(&self) -> Result<IndexMap, io::Error> {
        let now = Instant::now();

        let matcher = RegexMatcher::new(r"^=.*").unwrap();
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(false)
            .build();
        let mut sink = IndexSink::new();
        searcher.search_path(&matcher, &self.filepath, &mut sink)?;
        let file_size = std::fs::metadata(&self.filepath)?.len();
        let mut index_map: IndexMap = HashMap::new();
        for window in sink.matches.windows(2) {
            let (tag1, tag_id, offset1) = &window[0];
            let (_, _, offset2) = window[1];
            let index_row = IndexRow {
                r#type: format!("{:?}", tag1),
                id: tag_id.clone(),
                start: offset1.to_string(),
                length: (offset2 - offset1).to_string(),
            };
            match tag_id {
                Some(id) => {
                    index_map
                        .entry(*tag1)
                        .or_insert_with(|| IndexValue::Map(HashMap::new()))
                        .as_map_mut()
                        .unwrap()
                        .insert(id.clone(), index_row);
                }
                None => {
                    index_map
                        .entry(*tag1)
                        .or_insert_with(|| IndexValue::List(Vec::new()))
                        .as_list_mut()
                        .unwrap()
                        .push(index_row);
                }
            }
        }
        if let Some(last_match) = sink.matches.last() {
            let (last_tag, last_id, last_offset) = last_match;
            let index_row = IndexRow {
                r#type: format!("{:?}", last_tag),
                id: last_id.clone(),
                start: last_offset.to_string(),
                length: (file_size - last_offset).to_string(),
            };
            match last_id {
                Some(id) => {
                    index_map
                        .entry(*last_tag)
                        .or_insert_with(|| IndexValue::Map(HashMap::new()))
                        .as_map_mut()
                        .unwrap()
                        .insert(id.clone(), index_row);
                }
                None => {
                    index_map
                        .entry(*last_tag)
                        .or_insert_with(|| IndexValue::List(Vec::new()))
                        .as_list_mut()
                        .unwrap()
                        .push(index_row);
                }
            }
        }

        let elapsed = now.elapsed();
        println!("Building index took: {:.2?}", elapsed);

        Ok(index_map)
    }

    pub fn format_index(index_map: &IndexMap) -> Vec<String> {
        let mut formatted_index = Vec::new();
        for (tag, index_value) in index_map {
            match index_value {
                IndexValue::Map(inner_map) => {
                    for (id, index_row) in inner_map {
                        formatted_index.push(format!(
                            "{:?}:{} {} {}",
                            tag, id, index_row.start, index_row.length
                        ));
                    }
                }
                IndexValue::List(inner_list) => {
                    for index_row in inner_list {
                        formatted_index.push(format!(
                            "{:?} {} {}",
                            tag, index_row.start, index_row.length
                        ));
                    }
                }
            }
        }
        formatted_index
    }

    // after building the IndexMap, we can iterate through it and deserialize into the CrashDump struct
    pub fn parse(&self, index_map: &IndexMap) -> io::Result<CrashDump> {
        let crash_dump = CrashDump::from_index_map(&index_map, &self.filepath);

        crash_dump
    }

    pub fn get_heap_info<'a>(
        &self,
        crash_dump: &'a CrashDump,
        filepath: &String,
        id: &str,
    ) -> io::Result<Text<'a>> {
        // seeks to the file using the byteoffsets in the dict and just retrives the raw data
        //println!("{:?}", filepath);
        // println!("{:?}, {:#?}", id, crash_dump.processes_heap.get(id));
        if let Some(process_heap_ref) = crash_dump.processes_heap.get(id) {
            if let InfoOrIndex::Index(ref heap_index) = *process_heap_ref.value() {
                let file = OpenOptions::new().read(true).open(filepath)?;
                return crash_dump.load_proc_heap(heap_index, &file);
            }
        }
        Ok(Text::from(""))
    }

    pub fn get_stack_info<'a>(
        &self,
        crash_dump: &'a CrashDump,
        filepath: &String,
        id: &str,
    ) -> io::Result<Text<'a>> {
        if let Some(stack_info_ref) = crash_dump.processes_stack.get(id) {
            if let InfoOrIndex::Index(ref stack_index) = *stack_info_ref.value() {
                let file = OpenOptions::new().read(true).open(filepath)?;

                return crash_dump.load_proc_stack(stack_index, &file);
            }
        }
        Ok(Text::from(""))
    }

    pub fn get_message_queue_info<'a>(
        &self,
        crash_dump: &'a CrashDump,
        filepath: &String,
        id: &str,
    ) -> io::Result<Text<'a>> {
        if let Some(mq_index_ref) = crash_dump.processes_messages.get(id) {
            if let InfoOrIndex::Index(ref mq_index) = *mq_index_ref.value() {
                let file = OpenOptions::new().read(true).open(filepath)?;

                return crash_dump.load_proc_message_queue(mq_index, &file);
            }
        }

        Ok(Text::from(""))
    }

    /// Calculates the group information for each process in the ancestor map.
    ///
    /// The group information includes the total heap size, binary size, and memory size for each process and its children.
    ///
    /// # Arguments
    ///
    /// * `ancestor_map` - A map of process IDs to their children.
    /// * `processes` - A map of process IDs to their `ProcInfo`.
    ///
    /// # Returns
    ///
    /// Returns a map of process IDs to their `GroupInfo`.
    pub fn calculate_group_info(
        ancestor_map: &HashMap<String, Vec<String>>,
        processes: &DashMap<String, InfoOrIndex<ProcInfo>>,
    ) -> HashMap<String, GroupInfo> {
        // for each child pid, look it up in the processes section. If it exists, then add its sizes
        let mut group_info_map = HashMap::new();

        for (pid, children) in ancestor_map {
            let mut group_info = GroupInfo {
                total_heap_size: 0,
                total_binary_size: 0,
                total_memory_size: 0,
                pid: pid.clone(),
                name: "".to_string(),
                children: children.clone(),
            };
            for child in children {
                if let Some(proc_ref) = processes.get(child) {
                    if let InfoOrIndex::Info(ref proc) = *proc_ref.value() {
                        group_info.total_heap_size += proc.old_bin_vheap + proc.bin_vheap;
                        group_info.total_heap_size += proc.stack_heap + proc.old_heap;
                        group_info.total_memory_size += proc.memory;
                    }
                }
            }

            if let Some(proc_ref) = processes.get(pid) {
                if let InfoOrIndex::Info(ref proc) = *proc_ref.value() {
                    group_info.total_heap_size += proc.old_bin_vheap + proc.bin_vheap;
                    group_info.total_heap_size += proc.stack_heap + proc.old_heap;
                    group_info.total_memory_size += proc.memory;
                    if let Some(pid_name) = &proc.name {
                        group_info.name = pid_name.clone();
                    }
                }
            }

            group_info_map.insert(pid.clone(), group_info);
        }
        group_info_map
    }

    /// Creates a table of descendants for each process in the crash dump.
    ///
    /// The table maps each process ID to a vector of its descendants.
    ///
    /// # Arguments
    ///
    /// * `all_processes_info` - A map of all process IDs to their `ProcInfo`.
    ///
    /// # Returns
    ///
    /// Returns a map of process IDs to their descendants.
    pub fn create_descendants_table(
        all_processes_info: &DashMap<String, InfoOrIndex<ProcInfo>>,
    ) -> HashMap<String, Vec<String>> {
        let mut descendants: HashMap<String, Vec<String>> = HashMap::new();

        for ref_multi in all_processes_info.iter() {
            let pid = ref_multi.key();
            let proc_info_or_index = ref_multi.value();

            if let InfoOrIndex::Info(proc_info) = proc_info_or_index {
                let mut ancestor = proc_info.spawned_by.clone();

                // Navigate upwards to find the first ancestor with a name
                while let Some(ref ancestor_pid) = ancestor {
                    if let Some(ancestor_info_ref) = all_processes_info.get(ancestor_pid.as_str()) {
                        if let InfoOrIndex::Info(ancestor_proc) = ancestor_info_ref.value() {
                            if ancestor_proc.name.is_some() {
                                break;
                            }
                            ancestor = ancestor_proc.spawned_by.clone();
                        }
                    } else {
                        break;
                    }
                }

                if let Some(ancestor_pid) = ancestor {
                    descendants
                        .entry(ancestor_pid)
                        .or_insert_with(Vec::new)
                        .push(pid.clone());
                }
            }
        }

        descendants
    }

    fn split_path_and_filename(filepath: &str) -> Result<(PathBuf, String), io::Error> {
        let path = Path::new(filepath);
        let filepath = path.parent().unwrap_or(Path::new("."));
        let filename = path.file_name().unwrap_or(OsStr::new("")).to_string_lossy();
        Ok((filepath.to_path_buf(), filename.into_owned()))
    }
}
