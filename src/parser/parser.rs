use crate::parser::*;
use grep::{
    regex::RegexMatcher,
    searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch},
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

pub struct CDParser {
    //file: File,
    // mmap: Mmap,
    filepath: PathBuf,
    filename: String,
    crash_dump: CrashDump,
    index: Vec<String>,
}

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
            let tag_id_string = if match_bytes.len() > tag_end + 1 {
                let tag_id_cow = String::from_utf8_lossy(&match_bytes[tag_end + 1..]);
                Some(tag_id_cow.into_owned())
            } else {
                None
            };
            self.matches.push((tag_enum, tag_id_string, byte_offset));
        }
        Ok(true)
    }
}

impl CDParser {
    pub fn new(filepath: &str) -> Result<Self, io::Error> {
        let (filepath, filename) = Self::split_path_and_filename(filepath)?;

        // need to figure out mmap later
        // let mmap = unsafe { Mmap::map(&file)? };

        Ok(CDParser {
            //file,
            // mmap,
            filepath,
            filename,
            crash_dump: CrashDump::new(),
            index: Vec::new(),
        })
    }

    // really the parse needs to call the grep crate, which then does a simple regex match
    // the implementation is that we just need to search for the offsets
    // create the regex that searches for =<section>:<label>
    // then get all the byte offsets, from the length to each one
    // using that, map into chunks, and deserialize into the types.rs structs
    // at the end there will be a big struct that contains all the sections
    // enrich as needed

    pub fn build_index(&self) -> Result<IndexMap, io::Error> {
        let matcher = RegexMatcher::new(r"^=.*").unwrap();
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(false)
            .build();
        let mut sink = IndexSink::new();
        let realpath = self.filepath.join(&self.filename);
        searcher.search_path(&matcher, &realpath, &mut sink)?;
        let file_size = std::fs::metadata(&realpath)?.len();
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
            index_map
                .entry(*tag1)
                .or_insert_with(HashMap::new)
                .insert(tag_id.clone(), index_row);
        }
        if let Some(last_match) = sink.matches.last() {
            let (last_tag, last_id, last_offset) = last_match;
            let index_row = IndexRow {
                r#type: format!("{:?}", last_tag),
                id: last_id.clone(),
                start: last_offset.to_string(),
                length: (file_size - last_offset).to_string(),
            };
            index_map
                .entry(*last_tag)
                .or_insert_with(HashMap::new)
                .insert(last_id.clone(), index_row);
        }
        Ok(index_map)
    }

    pub fn format_index(index_map: &IndexMap) -> Vec<String> {
        let mut formatted_index = Vec::new();
        for (tag, inner_map) in index_map {
            for (id, index_row) in inner_map {
                formatted_index.push(format!(
                    "{:?}:{} {} {}",
                    tag,
                    id.as_deref().unwrap_or_default(),
                    index_row.start,
                    index_row.length
                ));
            }
        }
        formatted_index
    }

    // after building the IndexMap, we can iterate through it and deserialize into the CrashDump struct
    pub fn parse(&mut self) -> Result<(), io::Error> {
        let index_map = self.build_index()?;
        for (tag, inner_map) in index_map {
            for (id, index_row) in inner_map {
                
            }
        }
        Ok(())
    }

    // returns the slogan, crash time, and other general information
    pub fn get_premable(&self) -> Preamble {
        self.crash_dump.preamble.clone()
    }

    fn split_path_and_filename(filepath: &str) -> Result<(PathBuf, String), io::Error> {
        let path = Path::new(filepath);
        let filepath = path.parent().unwrap_or(Path::new("."));
        let filename = path.file_name().unwrap_or(OsStr::new("")).to_string_lossy();
        Ok((filepath.to_path_buf(), filename.into_owned()))
    }
}
