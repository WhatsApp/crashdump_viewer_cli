use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::collections::HashMap;
use std::io::Read;
use grep::{
        searcher::{BinaryDetection, SearcherBuilder, Searcher, Sink, SinkMatch},
        regex::RegexMatcher,
};

use crate::parser::Tag;

pub const TAG_PREAMBLE: &[u8] = b"erl_crash_dump";
pub const TAG_ABORT: &[u8] = b"abort";
pub const TAG_ALLOCATED_AREAS: &[u8] = b"allocated_areas";
pub const TAG_ALLOCATOR: &[u8] = b"allocator";
pub const TAG_ATOMS: &[u8] = b"atoms";
pub const TAG_BINARY: &[u8] = b"binary";
pub const TAG_DIRTY_CPU_SCHEDULER: &[u8] = b"dirty_cpu_scheduler";
pub const TAG_DIRTY_CPU_RUN_QUEUE: &[u8] = b"dirty_cpu_run_queue";
pub const TAG_DIRTY_IO_SCHEDULER: &[u8] = b"dirty_io_scheduler";
pub const TAG_DIRTY_IO_RUN_QUEUE: &[u8] = b"dirty_io_run_queue";
pub const TAG_ENDE: &[u8] = b"ende";
pub const TAG_ERL_CRASH_DUMP: &[u8] = b"erl_crash_dump";
pub const TAG_ETS: &[u8] = b"ets";
pub const TAG_FUN: &[u8] = b"fun";
pub const TAG_HASH_TABLE: &[u8] = b"hash_table";
pub const TAG_HIDDEN_NODE: &[u8] = b"hidden_node";
pub const TAG_INDEX_TABLE: &[u8] = b"index_table";
pub const TAG_INSTR_DATA: &[u8] = b"instr_data";
pub const TAG_INTERNAL_ETS: &[u8] = b"internal_ets";
pub const TAG_LITERALS: &[u8] = b"literals";
pub const TAG_LOADED_MODULES: &[u8] = b"loaded_modules";
pub const TAG_MEMORY: &[u8] = b"memory";
pub const TAG_MEMORY_MAP: &[u8] = b"memory_map";
pub const TAG_MEMORY_STATUS: &[u8] = b"memory_status";
pub const TAG_MOD: &[u8] = b"mod";
pub const TAG_NO_DISTRIBUTION: &[u8] = b"no_distribution";
pub const TAG_NODE: &[u8] = b"node";
pub const TAG_NOT_CONNECTED: &[u8] = b"not_connected";
pub const TAG_OLD_INSTR_DATA: &[u8] = b"old_instr_data";
pub const TAG_PERSISTENT_TERMS: &[u8] = b"persistent_terms";
pub const TAG_PORT: &[u8] = b"port";
pub const TAG_PROC: &[u8] = b"proc";
pub const TAG_PROC_DICTIONARY: &[u8] = b"proc_dictionary";
pub const TAG_PROC_HEAP: &[u8] = b"proc_heap";
pub const TAG_PROC_MESSAGES: &[u8] = b"proc_messages";
pub const TAG_PROC_STACK: &[u8] = b"proc_stack";
pub const TAG_SCHEDULER: &[u8] = b"scheduler";
pub const TAG_TIMER: &[u8] = b"timer";
pub const TAG_VISIBLE_NODE: &[u8] = b"visible_node";
pub const TAG_END: &[u8] = b"end";


pub struct CDParser {
    //file: File,
    // mmap: Mmap,
    filepath: PathBuf,
    filename: String,
    crash_dump_sections: CrashDumpSections,
    index: Vec<String>
}


struct IndexSink {
    matches: Vec<(Tag, u64)>,
}

impl IndexSink {
    fn new() -> Self{
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
            let tag_end = match_bytes.iter().position(|&x| x == b':').unwrap_or(match_bytes.len() - 1);

            let tag = &match_bytes[1..tag_end];
            println!("tag: {:?}", std::str::from_utf8(tag).unwrap());
            let tag_enum = match tag {
                TAG_PREAMBLE => Tag::Preamble,
                TAG_ABORT => Tag::Abort,
                TAG_ALLOCATED_AREAS => Tag::AllocatedAreas,
                TAG_ALLOCATOR => Tag::Allocator,
                TAG_ATOMS => Tag::Atoms,
                TAG_BINARY => Tag::Binary,
                TAG_DIRTY_CPU_SCHEDULER => Tag::DirtyCpuScheduler,
                TAG_DIRTY_CPU_RUN_QUEUE => Tag::DirtyCpuRunQueue,
                TAG_DIRTY_IO_SCHEDULER => Tag::DirtyIoScheduler,
                TAG_DIRTY_IO_RUN_QUEUE => Tag::DirtyIoRunQueue,
                TAG_ENDE => Tag::Ende,
                TAG_ERL_CRASH_DUMP => Tag::ErlCrashDump,
                TAG_ETS => Tag::Ets,
                TAG_FUN => Tag::Fun,
                TAG_HASH_TABLE => Tag::HashTable,
                TAG_HIDDEN_NODE => Tag::HiddenNode,
                TAG_INDEX_TABLE => Tag::IndexTable,
                TAG_INSTR_DATA => Tag::InstrData,
                TAG_INTERNAL_ETS => Tag::InternalEts,
                TAG_LITERALS => Tag::Literals,
                TAG_LOADED_MODULES => Tag::LoadedModules,
                TAG_MEMORY => Tag::Memory,
                TAG_MEMORY_MAP => Tag::MemoryMap,
                TAG_MEMORY_STATUS => Tag::MemoryStatus,
                TAG_MOD => Tag::Mod,
                TAG_NO_DISTRIBUTION => Tag::NoDistribution,
                TAG_NODE => Tag::Node,
                TAG_NOT_CONNECTED => Tag::NotConnected,
                TAG_OLD_INSTR_DATA => Tag::OldInstrData,
                TAG_PERSISTENT_TERMS => Tag::PersistentTerms,
                TAG_PORT => Tag::Port,
                TAG_PROC => Tag::Proc,
                TAG_PROC_DICTIONARY => Tag::ProcDictionary,
                TAG_PROC_HEAP => Tag::ProcHeap,
                TAG_PROC_MESSAGES => Tag::ProcMessages,
                TAG_PROC_STACK => Tag::ProcStack,
                TAG_SCHEDULER => Tag::Scheduler,
                TAG_TIMER => Tag::Timer,
                TAG_VISIBLE_NODE => Tag::VisibleNode,
                TAG_END => Tag::End,
                _ => 
                    unreachable!(),
            };
            self.matches.push((tag_enum, byte_offset));
        }
        Ok(true)
    }
}


// sections for an erlang crash dump
// each major section starts with a header of "^=.*(:.*)[0-1]\n"
// Example
// =erl_crash_dump:0.5
// Sat Jan  4 19:32:02 2025
// Slogan: forced_dump
// System version: Erlang/OTP 27 [erts-15.2] [source] [64-bit] [smp:8:8] [ds:8:8:10] [async-threads:1] [jit]
// Taints: 
// Atoms: 10568
// Calling Thread: scheduler:4

pub struct CrashDumpSections {
    pub premable: HashMap<String, String>,
    pub processes: HashMap<String, String>,
    pub binaries: HashMap<String, String>,
    pub heap: HashMap<String, String>,
    
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
            index: Vec::new(),
            crash_dump_sections: CrashDumpSections {
                premable: HashMap::new(),
                processes: HashMap::new(),
                binaries: HashMap::new(),
                heap: HashMap::new(),

            },

        })
    }

    // really the parse needs to call the grep crate, which then does a simple regex match
    // the implementation is that we just need to search for the offsets 
    // create the regex that searches for =<section>:<label>
    // then get all the byte offsets, from the length to each one
    // using that, map into chunks, and deserialize into the types.rs structs
    // at the end there will be a big struct that contains all the sections
    // enrich as needed

    

    pub fn get_index(&self) -> Result<Vec<String>, io::Error> {
        let matcher = RegexMatcher::new(r"^=.*").unwrap();

        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(false)
            .build();

        let mut sink = IndexSink::new();

        //let mut contents = Vec::new();
        let realpath = self.filepath.join(&self.filename);

        searcher.search_path(&matcher, &realpath, &mut sink);
        let vec_of_strings: Vec<String> = sink.matches
            .iter()
            .map(|tuple| format!("{:?}", tuple))
            .collect();

        Ok(vec_of_strings)
        //println!("{:?}", sink.matches);

        // let mut file = File::open(realpath)?;
        // file.read_to_end(&mut contents)?;
        // let lines: Vec<&str> = contents.split(|c| *c == b'\n').map(|s| std::str::from_utf8(s).unwrap()).collect();
    
        // Ok(lines.par_iter().enumerate().filter_map(|(index, line)| {
        //     if line.starts_with('=') {
        //         //Some((index, line.to_string()))
        //         Some(format!("{} : {}", index, line.to_string()))
        //     } else {
        //         None
        //     }
        // }).collect())
    }

    // returns the slogan, crash time, and other general information
    pub fn get_premable(&self) -> HashMap<String, String> {
        self.crash_dump_sections.premable.clone()
    }

    // need to list all the sections and their various counts, like unique counts for binaries, processes, etc
    // pub fn get_crash_dump_sections(&self) -> HashMap<String, String> {
        
    // }
    
    fn split_path_and_filename(filepath: &str) -> Result<(PathBuf, String), io::Error> {
        let path = Path::new(filepath);
        let filepath = path.parent().unwrap_or(Path::new("."));
        let filename = path.file_name().unwrap_or(OsStr::new("")).to_string_lossy();
        Ok((filepath.to_path_buf(), filename.into_owned()))
    }

    // fn get_json() -> String {

    // }
}
