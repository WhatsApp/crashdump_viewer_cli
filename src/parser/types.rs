use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::rc::Rc;
use std::str::FromStr;
use std::time::SystemTime;

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
    Preamble,
    Abort,
    AllocatedAreas,
    Allocator,
    Atoms,
    Binary,
    DirtyCpuScheduler,
    DirtyCpuRunQueue,
    DirtyIoScheduler,
    DirtyIoRunQueue,
    Ende,
    ErlCrashDump,
    Ets,
    Fun,
    HashTable,
    HiddenNode,
    IndexTable,
    InstrData,
    InternalEts,
    Literals,
    LoadedModules,
    Memory,
    MemoryMap,
    MemoryStatus,
    Mod,
    NoDistribution,
    Node,
    NotConnected,
    OldInstrData,
    PersistentTerms,
    Port,
    Proc,
    ProcDictionary,
    ProcHeap,
    ProcMessages,
    ProcStack,
    Scheduler,
    Timer,
    VisibleNode,
    End,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum DumpSection {
    Preamble(Preamble),
    // Allocator(AllocatorInfo),
    // Node(NodeInfo),
    Proc(ProcInfo),
    // ProcHeap(ProcHeapInfo),
    // ProcStack(ProcStackInfo),
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GenericSection {
    tag: String,
    id: Option<String>,
    data: HashMap<String, String>, // For key-value pairs
    //   items: Vec<HashMap<String, String>>, // For lists of items
    raw_lines: Vec<String>, // For raw lines without key-value pairs
}

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
        //    let mut items = Vec::new();
        let mut raw_lines = Vec::new();

        for line in lines {
            let parts: Vec<&str> = line.splitn(2, ": ").collect();
            if parts.len() == 2 {
                // key-value pair
                let key = parts[0].trim().to_string();
                let value = parts[1].trim().to_string();

                data.insert(key, value);
            } else if line.starts_with("0x") {
                // Special handling for lines starting with "0x"
                // (e.g., in the Stack Trace section)
                raw_lines.push(line.to_string());
            } else {
                // raw line
                raw_lines.push(line.to_string());
            }
        }

        Ok(GenericSection {
            tag,
            id,
            data,
            //       items,
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

        Tag::Proc => {
            // link_list might be optional or nonexistent
            // any of these fields might be optional or nonexistent

            let link_list: Vec<String> = data
                .get("Link list")
                .map(|s| {
                    s.trim_matches(|c| c == '[' || c == ']')
                        .split(", ")
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            let internal_state: Vec<String> = data
                .get("Internal State")
                .map(|s| s.split(" | ").map(|s| s.to_string()).collect())
                .unwrap_or_default();

            let program_counter: Option<ProgramCounter> = data
                .get("Program counter")
                .map(|s| ProgramCounter::from_string(s))
                .unwrap_or_default();

            let mut proc = ProcInfo {
                pid: id,
                state: data["State"].clone(),
                name: Some(
                    data.get("Name")
                        .map(|s| s.clone())
                        .unwrap_or("".to_string()),
                ),

                spawned_as: data.get("Spawned as").cloned().filter(|s| !s.is_empty()),
                spawned_by: data.get("Spawned by").cloned().filter(|s| !s.is_empty()),

                message_queue_length: data["Message queue length"].parse::<i64>().unwrap_or(0),
                number_of_heap_fragments: data["Number of heap fragments"].parse().unwrap_or(0),
                heap_fragment_data: data["Heap fragment data"].parse().unwrap_or(0),
                link_list: link_list,
                program_counter: program_counter.unwrap_or_default(),
                reductions: data["Reductions"].parse::<i64>().unwrap_or(0),
                stack_heap: data["Stack+heap"].parse::<i64>().unwrap_or(0),
                old_heap: data["OldHeap"].parse::<i64>().unwrap_or(0),
                heap_unused: data["Heap unused"].parse::<i64>().unwrap_or(0),
                old_heap_unused: data["OldHeap unused"].parse::<i64>().unwrap_or(0),
                bin_vheap: data["BinVHeap"].parse::<i64>().unwrap_or(0),
                old_bin_vheap: data["OldBinVHeap"].parse::<i64>().unwrap_or(0),
                memory: data["Memory"].parse::<i64>().unwrap_or(0),
                bin_vheap_unused: data["BinVHeap unused"].parse::<i64>().unwrap_or(0),
                old_bin_vheap_unused: data["OldBinVHeap unused"].parse::<i64>().unwrap_or(0),
                total_bin_vheap: 0,
                //arity: raw_lines[0].split("=").last().unwrap().parse::<i64>().unwrap(),
                arity: 0,
                internal_state: internal_state,
            };

            proc.total_bin_vheap = proc.bin_vheap + proc.old_bin_vheap;

            DumpSection::Proc(proc)
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

// #[derive(Debug)]
pub struct CrashDump {
    pub preamble: Preamble,
    pub memory: MemoryInfo,
    pub allocators: Vec<InfoOrIndex<AllocatorInfo>>,
    pub nodes: Vec<InfoOrIndex<NodeInfo>>,
    pub processes: HashMap<String, InfoOrIndex<ProcInfo>>,
    pub processes_heap: HashMap<String, InfoOrIndex<ProcHeapInfo>>,
    pub processes_stack: HashMap<String, InfoOrIndex<ProcStackInfo>>,
    pub ports: HashMap<String, InfoOrIndex<PortInfo>>,
    pub schedulers: Vec<InfoOrIndex<SchedulerInfo>>,
    pub ets: Vec<InfoOrIndex<EtsInfo>>,
    pub timers: Vec<InfoOrIndex<TimerInfo>>,
    pub atoms: Vec<InfoOrIndex<String>>,
    pub loaded_modules: Vec<InfoOrIndex<LoadedModules>>,
    pub persistent_terms: Vec<InfoOrIndex<PersistentTermInfo>>,
    pub raw_sections: HashMap<String, Vec<u8>>,
    pub group_info_map: HashMap<String, GroupInfo>,
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
            ports: HashMap::new(),
            schedulers: vec![],
            ets: vec![],
            timers: vec![],
            atoms: vec![],
            loaded_modules: vec![],
            persistent_terms: vec![],
            raw_sections: HashMap::new(),
            group_info_map: HashMap::new(),
        }
    }

    pub fn load_section(index_row: &IndexRow, file: &mut File) -> io::Result<String> {
        let start_offset: u64 = index_row.start.parse().unwrap_or(0);
        let length: u64 = index_row.length.parse().unwrap_or(0);

        file.seek(SeekFrom::Start(start_offset))?;

        let mut buffer = vec![0; length as usize];
        file.read_exact(&mut buffer)?;

        let contents = String::from_utf8_lossy(&buffer);
        Ok(contents.to_string())
    }

    pub fn from_index_map(index_map: &IndexMap, file_path: &PathBuf) -> io::Result<Self> {
        let mut crash_dump = CrashDump::new();
        let mut file = File::open(file_path)?;
        // let mut child_map: HashMap<String, Vec<String>> = HashMap::new();

        for (tag, index_value) in index_map {
            match index_value {
                IndexValue::Map(inner_map) => {
                    for (id, index_row) in inner_map {
                        match tag {
                            Tag::Preamble => {
                                let contents = Self::load_section(&index_row, &mut file)?;
                                if let Ok(DumpSection::Preamble(preamble)) =
                                    parse_section(&contents, Some(&id))
                                {
                                    crash_dump.preamble = preamble;
                                }
                            }

                            Tag::Proc => {
                                let contents = Self::load_section(&index_row, &mut file)?;
                                if let Ok(DumpSection::Proc(proc)) =
                                    parse_section(&contents, Some(&id))
                                {
                                    crash_dump
                                        .processes
                                        .insert(id.clone(), InfoOrIndex::Info(proc));
                                }
                            }

                            Tag::ProcHeap => {
                                crash_dump
                                    .processes_heap
                                    .insert(id.clone(), InfoOrIndex::Index(index_row.clone()));
                            }

                            _ => {}
                        }
                    }
                }
                IndexValue::List(index_rows) => {
                    for index_row in index_rows {
                        match tag {
                            Tag::Memory => {
                                let contents = Self::load_section(&index_row, &mut file)?;

                                if let Ok(DumpSection::Memory(memory)) =
                                    parse_section(&contents, None)
                                {
                                    crash_dump.memory = memory;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(crash_dump)
    }
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
// #[derive(Debug, Deserialize)]
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
// #[derive(Debug, Deserialize)]
pub struct MBCSCarriers {
    pub count: i64,
    pub mseg_count: i64,
    pub sys_alloc_count: i64,
    pub size: [i64; 3],
    pub mseg_size: i64,
    pub sys_alloc_size: i64,
}
// #[derive(Debug, Deserialize)]
pub struct SBCSCarriers {
    pub count: i64,
    pub mseg_count: i64,
    pub sys_alloc_count: i64,
    pub size: [i64; 3],
    pub mseg_size: i64,
    pub sys_alloc_size: i64,
}
// #[derive(Debug, Deserialize)]
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
// #[derive(Debug, Deserialize)]
pub struct BlockInfo {
    pub count: [i64; 3],
    pub size: [i64; 3],
}
// #[derive(Debug, Deserialize)]
pub struct NodeInfo {
    pub name: String,
    pub type_: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub struct ProcInfo {
    pub pid: String,
    pub state: String,
    pub name: Option<String>,
    pub spawned_as: Option<String>,
    pub spawned_by: Option<String>,
    pub message_queue_length: i64,
    pub number_of_heap_fragments: i64,
    pub heap_fragment_data: i64,
    pub link_list: Vec<String>,
    pub reductions: i64,
    pub stack_heap: i64,
    pub old_heap: i64,
    pub heap_unused: i64,
    pub old_heap_unused: i64,
    pub bin_vheap: i64,
    pub old_bin_vheap: i64,
    pub bin_vheap_unused: i64,
    pub old_bin_vheap_unused: i64,
    pub total_bin_vheap: i64,
    pub memory: i64,
    pub arity: i64,
    pub program_counter: ProgramCounter,
    pub internal_state: Vec<String>,
}

impl ProcInfo {
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
#[derive(Debug)]
pub enum ValueType {
    IntegerValue,
    AtomValue,
    HeapRefValue,
    PidValue,
}
// #[derive(Debug, Deserialize)]
pub struct ProcStackInfo {
    pub pid: String,
    pub variables: HashMap<String, Value>,
    pub frames: Vec<StackFrame>,
}
// #[derive(Debug, Deserialize)]
pub struct StackFrame {
    pub address: String,
    pub return_addr: String,
    pub function: String,
    pub offset: i64,
    pub variables: HashMap<String, Value>,
}
// #[derive(Debug, Deserialize)]
pub struct SchedulerInfo {
    pub id: i32,
    pub sleep_info: SleepInfo,
    pub current_port: String,
    pub run_queue: RunQueue,
    pub current_process: CurrentProcess,
}
// #[derive(Debug, Deserialize)]
pub struct SleepInfo {
    pub flags: Vec<String>,
    pub aux_work: Vec<String>,
}
// #[derive(Debug, Deserialize)]
pub struct RunQueue {
    pub max_length: i32,
    pub high_length: i32,
    pub normal_length: i32,
    pub low_length: i32,
    pub port_length: i32,
    pub flags: Vec<String>,
}
// #[derive(Debug, Deserialize)]
pub struct CurrentProcess {
    pub pid: String,
    pub state: String,
    pub internal_state: Vec<String>,
    pub program_counter: ProgramCounter,
    pub stack_trace: Vec<StackFrame>,
}
// #[derive(Debug, Deserialize)]
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
// #[derive(Debug, Deserialize)]
pub struct ChainLength {
    pub avg: f64,
    pub max: i32,
    pub min: i32,
    pub std_dev: f64,
    pub expected_std_dev: f64,
}
// #[derive(Debug, Deserialize)]
pub struct IndexTableInfo {
    pub name: String,
    pub size: i64,
    pub limit: i64,
    pub entries: i64,
}
// #[derive(Debug, Deserialize)]
pub struct HashTableInfo {
    pub name: String,
    pub size: i64,
    pub used: i64,
    pub objects: i64,
    pub depth: i32,
}
// #[derive(Debug, Deserialize)]
pub struct TimerInfo {
    pub pid: String,
    pub message: serde_json::Value,
    pub time_left: i64,
}
// #[derive(Debug, Deserialize)]
pub struct LoadedModules {
    pub current_code: i64,
    pub old_code: i64,
    pub modules: Vec<ModuleInfo>,
}
// #[derive(Debug, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub current_size: i64,
}
// #[derive(Debug, Deserialize)]
pub struct PersistentTermInfo {
    pub terms: Vec<PersistentTerm>,
}
// #[derive(Debug, Deserialize)]
pub struct PersistentTerm {
    pub address: String,
    pub value: Value,
}
// #[derive(Debug, Deserialize)]
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
