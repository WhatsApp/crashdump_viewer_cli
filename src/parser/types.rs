use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;

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

// Section tags - lifted from https://github.com/erlang/otp/blob/master/lib/observer/src/crashdump_viewer.erl#L121
#[derive(Debug)]
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
    // Proc(ProcInfo),
    // ProcHeap(ProcHeapInfo),
    // ProcStack(ProcStackInfo),
    // Scheduler(SchedulerInfo),
    // Ets(EtsInfo),
    // Timer(TimerInfo),
    // Port(PortInfo),
    // Memory(MemoryInfo),
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
            let parts: Vec<&str> = line.splitn(2, ":").collect();
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

fn parse_section(s: &str) -> Result<DumpSection, String> {
    let section = GenericSection::from_str(s)?;
    let id = section.id.clone().unwrap_or_else(|| "".to_string());
    let raw_lines = &section.raw_lines;
    let data = &section.data;

    let section = match section.tag.as_str() {
        "preamble" => {
            let preamble = Preamble {
                version: id,
                time: raw_lines[0].clone(),
                slogan: data["Slogan"].parse().unwrap(),
                erts: data["System version"].parse().unwrap(),
                taints: data["Taints"].parse().unwrap(),
                atom_count: data["Atom count"].parse::<i64>().unwrap(),
            };
            DumpSection::Preamble(preamble)
        }

        _ => DumpSection::Generic(section),
    };
    Ok(section)
}

#[derive(Debug, PartialEq)] // Added PartialEq for comparison in tests if needed
pub struct IndexRow {
    r#type: String, // Use r#type to avoid keyword conflict
    id: Option<String>,
    start: String,
    length: String,
}

#[derive(Debug)]
pub enum InfoOrIndex<T> {
    Index(IndexRow), // Now Index holds IndexRow
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
        }
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
}
// #[derive(Debug, Deserialize)]
pub struct ERTS {
    pub version: String,
    pub thread: String,
    pub word_size: i32,
    pub async_status: String,
}
// #[derive(Debug, Deserialize)]
pub struct MemoryInfo {
    pub total: i64,
    pub processes: Processes,
    pub system: i64,
    pub atom: Atom,
    pub binary: i64,
    pub code: i64,
    pub ets: i64,
}
// #[derive(Debug, Deserialize)]
pub struct Processes {
    pub total: i64,
    pub used: i64,
}
// #[derive(Debug, Deserialize)]
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
// #[derive(Debug, Deserialize)]
pub struct ProcInfo {
    pub pid: String,
    pub state: String,
    pub name: String,
    pub spawned_as: String,
    pub spawned_by: String,
    pub message_queue_length: i64,
    pub heap_fragments: HeapFragments,
    pub reductions: i64,
    pub memory: Memory,
    pub program_counter: ProgramCounter,
    pub internal_state: Vec<String>,
}
// #[derive(Debug, Deserialize)]
pub struct HeapFragments {
    pub count: i64,
    pub data: i64,
}
// #[derive(Debug, Deserialize)]
pub struct Memory {
    pub stack_heap: i64,
    pub old_heap: i64,
    pub heap_unused: i64,
    pub old_heap_unused: i64,
    pub bin_vheap: i64,
    pub old_bin_vheap: i64,
    pub bin_vheap_unused: i64,
    pub old_bin_vheap_unused: i64,
    pub total: i64,
}
// #[derive(Debug, Deserialize)]
pub struct ProgramCounter {
    pub address: String,
    pub function: String,
    pub offset: i64,
    pub arity: i32,
}
// #[derive(Debug, Deserialize)]
pub struct ProcHeapInfo {
    pub pid: String,
    pub entries: HashMap<String, HeapEntry>,
}
// #[derive(Debug, Deserialize)]
pub struct HeapEntry {
    pub address: String,
    pub type_: String,
    pub contents: Vec<Value>,
    pub raw: String,
}
// #[derive(Debug, Deserialize)]
pub struct Value {
    pub type_: ValueType,
    pub raw: String,
    pub integer: Option<i64>,
    pub atom: Option<String>,
    pub heap_ref: Option<String>,
}
// #[derive(Debug, Deserialize)]
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
