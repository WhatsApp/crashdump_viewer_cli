use std::collections::HashMap;
use std::time::SystemTime;
use serde::de::{self, Deserializer, Visitor};
use std::fmt;
use serde::Deserialize;

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


// #[derive(Debug, Deserialize)]
pub struct CrashDump {
    pub preamble: Preamble,
    pub memory: MemoryInfo,
    pub allocators: Vec<AllocatorInfo>,
    pub nodes: Vec<NodeInfo>,
    pub processes: HashMap<String, ProcInfo>,
    pub processes_heap: HashMap<String, ProcHeapInfo>,
    pub processes_stack: HashMap<String, ProcStackInfo>,
    pub ports: HashMap<String, PortInfo>,
    pub schedulers: Vec<SchedulerInfo>,
    pub ets: Vec<EtsInfo>,
    pub timers: Vec<TimerInfo>,
    pub atoms: Vec<String>,
    pub loaded_modules: Vec<LoadedModules>,
    pub persistent_terms: Vec<PersistentTermInfo>,
    pub raw_sections: HashMap<String, Vec<u8>>,
}
// #[derive(Debug, Deserialize)]
pub struct Preamble {
    pub version: String,
    pub time: SystemTime,
    pub slogan: String,
    pub otp_release: String,
    pub erts: ERTS,
    pub taints: Vec<String>,
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
