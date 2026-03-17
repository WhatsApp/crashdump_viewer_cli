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

//! Advanced term parsing with two-pass approach and pointer dereferencing.
//!
//! This module implements sophisticated heap term parsing similar to OTP Observer's
//! crashdump_viewer.erl. It uses a two-pass approach to handle forward references
//! and complex term structures like maps and binaries.
//!
//! ## Two-Pass Parsing Strategy
//!
//! **Pass 1: Collect Lines**
//! - Read all heap lines without parsing
//! - Build a HashMap (line map) of Address -> Line
//! - Separate refc binaries (Yc) to process them first
//!
//! **Pass 2: Parse with Context**
//! - Parse each line in order (refc binaries first)
//! - Build a BTreeMap (term dictionary) of Address -> ParsedTerm
//! - Support lazy parsing via deref_ptr for forward references
//!
//! ## Key Components
//!
//! - `HeapParser`: Main parser struct with line map and term dictionary
//! - `ParsedTerm`: Enum representing all possible Erlang term types
//! - `deref_ptr()`: Pointer dereferencing with forward reference support
//! - Binary index table for fast binary lookups

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use num_bigint::{BigInt, BigUint};
use std::str::FromStr;

/// A parsed Erlang term that can be stored in the term dictionary.
#[derive(Debug, Clone)]
pub enum ParsedTerm {
    // Simple types
    Integer(i64),
    BigNum(BigInt), // Arbitrary precision integer
    Float(f64),
    Atom(String),
    Nil,
    String(String),

    // Compound types
    List(Vec<ParsedTerm>),
    ImproperList(Vec<ParsedTerm>, Box<ParsedTerm>), // [H1, H2, ... | Tail]
    Tuple(Vec<ParsedTerm>),

    // Process identifiers
    Pid(String),
    Port(String),
    ExternalPid(String),
    ExternalPort(String),

    // Binaries
    HeapBinary(Vec<u8>),
    RefcBinary {
        address: u64,
        offset: usize,
        size: usize,
    },
    SubBinary {
        parent_addr: u64,
        offset: usize,
        size: usize,
    },

    // Maps
    FlatMap(HashMap<String, ParsedTerm>), // Using String keys for now
    HashMap(HashMap<String, ParsedTerm>), // Could be improved with better key type

    // Functions and external
    FunRef {
        module: String,
        function: String,
        arity: usize,
    },
    ExternalTerm(Vec<u8>), // Encoded in external format

    // References
    HeapPointer(u64), // Pointer to another heap term
    IncompleteHeap,   // Marker for incomplete/truncated heap

    // Distribution
    DistExternal(Vec<u8>), // Distribution external format with atom cache
}

impl fmt::Display for ParsedTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParsedTerm::Integer(i) => write!(f, "{}", i),
            ParsedTerm::BigNum(s) => write!(f, "{}", s),
            ParsedTerm::Float(fl) => write!(f, "{}", fl),
            ParsedTerm::Atom(a) => write!(f, "{}", a),
            ParsedTerm::Nil => write!(f, "[]"),
            ParsedTerm::String(s) => write!(f, "\"{}\"", s),
            ParsedTerm::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            ParsedTerm::ImproperList(head, tail) => {
                write!(f, "[")?;
                for (i, item) in head.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, " | {}]", tail)
            }
            ParsedTerm::Tuple(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "}}")
            }
            ParsedTerm::Pid(p) => write!(f, "{}", p),
            ParsedTerm::Port(p) => write!(f, "{}", p),
            ParsedTerm::ExternalPid(p) => write!(f, "external_pid({})", p),
            ParsedTerm::ExternalPort(p) => write!(f, "external_port({})", p),
            ParsedTerm::HeapBinary(bytes) => write!(f, "<<{}>>", bytes.len()),
            ParsedTerm::RefcBinary { address, offset, size } => {
                write!(f, "<<refc: @0x{:X}+{}:{} bytes>>", address, offset, size)
            }
            ParsedTerm::SubBinary { parent_addr, offset, size } => {
                write!(f, "<<sub: @0x{:X}+{}:{} bytes>>", parent_addr, offset, size)
            }
            ParsedTerm::FlatMap(m) => write!(f, "#{{flatmap: {} keys}}", m.len()),
            ParsedTerm::HashMap(m) => write!(f, "#{{hashmap: {} keys}}", m.len()),
            ParsedTerm::FunRef { module, function, arity } => {
                write!(f, "fun {}:{}/{}", module, function, arity)
            }
            ParsedTerm::ExternalTerm(bytes) => write!(f, "external({} bytes)", bytes.len()),
            ParsedTerm::HeapPointer(addr) => write!(f, "@0x{:X}", addr),
            ParsedTerm::IncompleteHeap => write!(f, "#<incomplete_heap>"),
            ParsedTerm::DistExternal(bytes) => write!(f, "dist_external({} bytes)", bytes.len()),
        }
    }
}

/// Main heap parser with two-pass parsing support.
pub struct HeapParser {
    /// Line map: Address -> Unparsed line (for forward references)
    line_map: HashMap<u64, String>,

    /// Term dictionary: Address -> Parsed term
    term_dict: BTreeMap<u64, ParsedTerm>,

    /// Binary index: Binary address -> File position (for lazy binary loading)
    binary_index: HashMap<u64, u64>,

    /// Binary address adjustment for old dump versions (<0.3)
    bin_addr_adj: u64,

    /// Whether to use base64 decoding (dump version >= 0.5)
    use_base64: bool,

    /// Track if we encountered incomplete heap
    incomplete_heap: bool,
}

impl HeapParser {
    /// Create a new heap parser.
    ///
    /// # Arguments
    /// * `bin_addr_adj` - Binary address adjustment for old dumps (0 for modern dumps)
    /// * `use_base64` - Whether to use base64 decoding (true for dump version >= 0.5)
    pub fn new(bin_addr_adj: u64, use_base64: bool) -> Self {
        Self {
            line_map: HashMap::new(),
            term_dict: BTreeMap::new(),
            binary_index: HashMap::new(),
            bin_addr_adj,
            use_base64,
            incomplete_heap: false,
        }
    }

    /// Pass 1: Collect heap lines without parsing.
    ///
    /// This builds the line map that will be used for forward references.
    ///
    /// # Arguments
    /// * `lines` - Iterator of (address, line_content) pairs
    pub fn collect_lines<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = (u64, String)>,
    {
        for (addr, line) in lines {
            self.line_map.insert(addr, line);
        }
    }

    /// Pass 2: Parse all collected lines.
    ///
    /// This parses heap terms in the correct order:
    /// 1. Refc binaries (Yc) first
    /// 2. All other terms in reverse order (minimizes forward references)
    ///
    /// Returns true if heap parsing completed successfully, false if incomplete.
    pub fn parse_all(&mut self) -> bool {
        // Separate refc binaries from other lines
        let mut refc_lines = Vec::new();
        let mut other_lines = Vec::new();

        for (addr, line) in &self.line_map {
            if line.starts_with("Yc") {
                refc_lines.push((*addr, line.clone()));
            } else {
                other_lines.push((*addr, line.clone()));
            }
        }

        // Process refc binaries first
        for (addr, line) in refc_lines {
            if !self.term_dict.contains_key(&addr) {
                let _ = self.parse_line(addr, &line);
            }
        }

        // Process other lines in reverse order
        other_lines.reverse();
        for (addr, line) in other_lines {
            if !self.term_dict.contains_key(&addr) {
                let _ = self.parse_line(addr, &line);
            }
        }

        !self.incomplete_heap
    }

    /// Parse a single heap line.
    ///
    /// This is called during pass 2, or recursively via deref_ptr for forward references.
    fn parse_line(&mut self, addr: u64, line: &str) -> Result<ParsedTerm, String> {
        let term = self.parse_heap_term(addr, line)?;
        self.term_dict.insert(addr, term.clone());
        Ok(term)
    }

    /// Parse a heap term from a line.
    ///
    /// This dispatches to specific parsers based on the first character(s).
    fn parse_heap_term(&mut self, addr: u64, line: &str) -> Result<ParsedTerm, String> {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return Err("Empty heap line".to_string());
        }

        match chars[0] {
            'l' => self.parse_cons_cell(addr, &line[1..]),      // Cons cell (list)
            't' => self.parse_tuple(addr, &line[1..]),            // Tuple
            'F' => self.parse_float_term(addr, &line[1..]),       // Float
            'B' => self.parse_bignum_term(addr, line),            // Big number
            'P' => self.parse_external_pid(addr, &line[1..]),     // External PID
            'p' => self.parse_external_port(addr, &line[1..]),    // External Port
            'E' => self.parse_external_format(addr, &line[1..]),  // External format
            'Y' => self.parse_binary_term(addr, line),            // Binary (Yh, Yc, Ys)
            'M' => self.parse_map_term(addr, line),               // Map (Mf, Mh, Mn)
            'R' => self.parse_fun_ref(addr, &line[1..]),          // Fun reference
            'H' => self.parse_heap_pointer(&line[1..]),           // Heap pointer (for deref)
            'D' => self.parse_dist_external(addr, &line[1..]),    // Distribution external
            _ => Err(format!("Unknown heap term type: {}", chars[0])),
        }
    }

    /// Parse a cons cell (list element).
    ///
    /// Format: `l<head>|<tail>`
    fn parse_cons_cell(&mut self, addr: u64, line: &str) -> Result<ParsedTerm, String> {
        // Find the '|' separator
        if let Some(sep_pos) = line.find('|') {
            let head_str = &line[..sep_pos];
            let tail_str = &line[sep_pos + 1..];

            let head = self.parse_term(head_str)?;
            let tail = self.parse_term(tail_str)?;

            // Check if tail is Nil (proper list) or another cons/term (improper list)
            match tail {
                ParsedTerm::Nil => Ok(ParsedTerm::List(vec![head])),
                ParsedTerm::List(mut items) => {
                    let mut result = vec![head];
                    result.append(&mut items);
                    Ok(ParsedTerm::List(result))
                }
                other => Ok(ParsedTerm::ImproperList(vec![head], Box::new(other))),
            }
        } else {
            Err(format!("Invalid cons cell format: {}", line))
        }
    }

    /// Parse a tuple.
    ///
    /// Format: `t<size>:<elem1>,<elem2>,...`
    fn parse_tuple(&mut self, addr: u64, line: &str) -> Result<ParsedTerm, String> {
        if let Some(colon_pos) = line.find(':') {
            let size_str = &line[..colon_pos];
            let size = usize::from_str_radix(size_str, 16)
                .map_err(|e| format!("Invalid tuple size: {}", e))?;

            let elements_str = &line[colon_pos + 1..];
            let elements = self.parse_tuple_elements(elements_str, size)?;

            Ok(ParsedTerm::Tuple(elements))
        } else {
            Err(format!("Invalid tuple format: {}", line))
        }
    }

    /// Parse tuple elements separated by commas.
    fn parse_tuple_elements(&mut self, line: &str, expected_count: usize) -> Result<Vec<ParsedTerm>, String> {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() != expected_count {
            return Err(format!(
                "Tuple element count mismatch: expected {}, got {}",
                expected_count,
                parts.len()
            ));
        }

        parts.iter().map(|s| self.parse_term(s)).collect()
    }

    /// Parse a generic term (used for sub-elements).
    ///
    /// This handles the term parsing dispatch for non-heap-specific terms.
    fn parse_term(&mut self, data: &str) -> Result<ParsedTerm, String> {
        if data.is_empty() {
            return Err("Empty term data".to_string());
        }

        let chars: Vec<char> = data.chars().collect();
        match chars[0] {
            'I' => self.parse_integer(&data[1..]),
            'A' => self.parse_atom(&data[1..]),
            'N' => Ok(ParsedTerm::Nil),
            'H' => self.deref_ptr(&data[1..]),
            'P' => Ok(ParsedTerm::ExternalPid(data[1..].to_string())),
            'p' => Ok(ParsedTerm::ExternalPort(data[1..].to_string())),
            'S' => Ok(ParsedTerm::String(data[1..].to_string())),
            _ => Err(format!("Unknown term type in parse_term: {}", chars[0])),
        }
    }

    /// Dereference a heap pointer.
    ///
    /// This implements the lazy parsing strategy: if the term is already in the
    /// dictionary, return it. Otherwise, check the line map and parse it now.
    fn deref_ptr(&mut self, ptr_str: &str) -> Result<ParsedTerm, String> {
        let addr = u64::from_str_radix(ptr_str, 16)
            .map_err(|e| format!("Invalid hex address: {}", e))?;

        // Check if already parsed
        if let Some(term) = self.term_dict.get(&addr) {
            return Ok(term.clone());
        }

        // Check line map for forward reference
        if let Some(line) = self.line_map.get(&addr).cloned() {
            // Parse the referenced line now
            return self.parse_line(addr, &line);
        }

        // Not found - mark as incomplete
        self.incomplete_heap = true;
        Ok(ParsedTerm::IncompleteHeap)
    }

    /// Parse a heap pointer term (for display purposes).
    fn parse_heap_pointer(&self, ptr_str: &str) -> Result<ParsedTerm, String> {
        let addr = u64::from_str_radix(ptr_str, 16)
            .map_err(|e| format!("Invalid hex address: {}", e))?;
        Ok(ParsedTerm::HeapPointer(addr))
    }

    /// Parse an integer.
    ///
    /// Format: `I<decimal>`
    fn parse_integer(&self, data: &str) -> Result<ParsedTerm, String> {
        let val = data.parse::<i64>()
            .map_err(|e| format!("Invalid integer: {}", e))?;
        Ok(ParsedTerm::Integer(val))
    }

    /// Parse an atom.
    ///
    /// Format: `A<size>:<chars>`
    fn parse_atom(&self, data: &str) -> Result<ParsedTerm, String> {
        if let Some(colon_pos) = data.find(':') {
            let atom_str = &data[colon_pos + 1..];
            Ok(ParsedTerm::Atom(atom_str.to_string()))
        } else {
            Err(format!("Invalid atom format: {}", data))
        }
    }

    /// Parse a float term.
    ///
    /// Format: `F<len>:<float_string>`
    fn parse_float_term(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        if let Some(colon_pos) = data.find(':') {
            let float_str = &data[colon_pos + 1..];
            let val = float_str.parse::<f64>()
                .map_err(|e| format!("Invalid float: {}", e))?;
            Ok(ParsedTerm::Float(val))
        } else {
            Err(format!("Invalid float format: {}", data))
        }
    }

    /// Parse a big number term.
    ///
    /// Formats: `B16#<hex>`, `B-16#<hex>`, `B<decimal>`
    fn parse_bignum_term(&self, _addr: u64, data: &str) -> Result<ParsedTerm, String> {
        if data.starts_with("B16#") {
            // Positive hexadecimal bignum
            let hex_str = &data[4..];
            BigInt::parse_bytes(hex_str.as_bytes(), 16)
                .ok_or_else(|| format!("Invalid hex bignum: {}", data))
                .map(ParsedTerm::BigNum)
        } else if data.starts_with("B-16#") {
            // Negative hexadecimal bignum
            let hex_str = &data[5..];
            BigInt::parse_bytes(hex_str.as_bytes(), 16)
                .map(|n| ParsedTerm::BigNum(-n))
                .ok_or_else(|| format!("Invalid negative hex bignum: {}", data))
        } else if data.starts_with("B") {
            // Decimal bignum
            let dec_str = &data[1..];
            BigInt::from_str(dec_str)
                .map(ParsedTerm::BigNum)
                .map_err(|e| format!("Invalid decimal bignum: {} - {}", data, e))
        } else {
            Err(format!("Invalid bignum format: {}", data))
        }
    }

    /// Parse an external PID.
    fn parse_external_pid(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        Ok(ParsedTerm::ExternalPid(data.to_string()))
    }

    /// Parse an external port.
    fn parse_external_port(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        Ok(ParsedTerm::ExternalPort(data.to_string()))
    }

    /// Parse external format term.
    ///
    /// Format: `E<len>:<binary_data>`
    fn parse_external_format(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // For now, just store the raw bytes
        // TODO: Implement proper external term format decoding
        Ok(ParsedTerm::ExternalTerm(data.as_bytes().to_vec()))
    }

    /// Parse distribution external format.
    fn parse_dist_external(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // TODO: Implement atom cache and proper dist external parsing
        Ok(ParsedTerm::DistExternal(data.as_bytes().to_vec()))
    }

    /// Parse a fun reference.
    ///
    /// Format: `Rf<address>` (simplified for now)
    fn parse_fun_ref(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // TODO: Parse actual fun reference fields
        Ok(ParsedTerm::FunRef {
            module: "unknown".to_string(),
            function: "unknown".to_string(),
            arity: 0,
        })
    }

    /// Parse binary terms (Yh, Yc, Ys).
    fn parse_binary_term(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        let chars: Vec<char> = data.chars().collect();
        if chars.len() < 2 {
            return Err(format!("Invalid binary format: {}", data));
        }

        match chars[1] {
            'h' => self.parse_heap_binary(addr, &data[2..]),
            'c' => self.parse_refc_binary(addr, &data[2..]),
            's' => self.parse_sub_binary(addr, &data[2..]),
            _ => Err(format!("Unknown binary type: Y{}", chars[1])),
        }
    }

    /// Parse a heap binary (Yh).
    ///
    /// Format: `Yh<size>:<bytes>` or `Yh<size>:<base64_bytes>` (if use_base64)
    fn parse_heap_binary(&self, _addr: u64, data: &str) -> Result<ParsedTerm, String> {
        if let Some(colon_pos) = data.find(':') {
            let size_str = &data[..colon_pos];
            let _size = usize::from_str_radix(size_str, 16)
                .map_err(|e| format!("Invalid binary size: {}", e))?;
            let content = &data[colon_pos + 1..];

            // Decode binary content
            let bytes = if self.use_base64 {
                // Decode from base64
                BASE64.decode(content)
                    .map_err(|e| format!("Failed to decode base64 binary: {}", e))?
            } else {
                // Direct byte representation (each pair of hex digits is a byte)
                // For simplicity, just store as bytes for now
                content.as_bytes().to_vec()
            };

            Ok(ParsedTerm::HeapBinary(bytes))
        } else {
            Err(format!("Invalid heap binary format: {}", data))
        }
    }

    /// Parse a reference-counted binary (Yc).
    ///
    /// Format: `Yc<binp>:<offset>:<size>`
    fn parse_refc_binary(&self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid refc binary format: {}", data));
        }

        let binp = u64::from_str_radix(parts[0], 16)
            .map_err(|e| format!("Invalid binp: {}", e))?;
        let offset = usize::from_str_radix(parts[1], 16)
            .map_err(|e| format!("Invalid offset: {}", e))?;
        let size = usize::from_str_radix(parts[2], 16)
            .map_err(|e| format!("Invalid size: {}", e))?;

        let adjusted_binp = binp | self.bin_addr_adj;

        Ok(ParsedTerm::RefcBinary {
            address: adjusted_binp,
            offset,
            size,
        })
    }

    /// Parse a sub binary (Ys).
    ///
    /// Format: `Ys<parent_addr>:<offset>:<size>`
    fn parse_sub_binary(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.len() < 3 {
            return Err(format!("Invalid sub binary format: {}", data));
        }

        let parent_addr = u64::from_str_radix(parts[0], 16)
            .map_err(|e| format!("Invalid parent addr: {}", e))?;
        let offset = usize::from_str_radix(parts[1], 16)
            .map_err(|e| format!("Invalid offset: {}", e))?;
        let size = usize::from_str_radix(parts[2], 16)
            .map_err(|e| format!("Invalid size: {}", e))?;

        Ok(ParsedTerm::SubBinary {
            parent_addr,
            offset,
            size,
        })
    }

    /// Parse map terms (Mf, Mh, Mn).
    fn parse_map_term(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        let chars: Vec<char> = data.chars().collect();
        if chars.len() < 2 {
            return Err(format!("Invalid map format: {}", data));
        }

        match chars[1] {
            'f' => self.parse_flatmap(addr, &data[2..]),
            'h' => self.parse_hashmap_head(addr, &data[2..]),
            'n' => self.parse_hashmap_node(addr, &data[2..]),
            _ => Err(format!("Unknown map type: M{}", chars[1])),
        }
    }

    /// Parse a flatmap (Mf).
    ///
    /// Format: `Mf<size>:<keys_tuple>:<values_tuple>`
    fn parse_flatmap(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // Parse the size
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(format!("Invalid flatmap format: {}", data));
        }

        let size = usize::from_str_radix(parts[0], 16)
            .map_err(|e| format!("Invalid flatmap size: {}", e))?;

        // Parse keys tuple (should be a tuple)
        let keys_str = parts[1];
        let keys_term = self.parse_term(keys_str)?;

        // Parse values tuple
        let values_str = parts[2];
        let values_tuple = self.parse_tuple(addr, values_str)?;

        // Extract the actual values from the tuple
        let values = match values_tuple {
            ParsedTerm::Tuple(vals) => vals,
            _ => return Err("Flatmap values not a tuple".to_string()),
        };

        // Extract the keys
        let keys = match keys_term {
            ParsedTerm::Tuple(ks) => ks,
            _ => return Err("Flatmap keys not a tuple".to_string()),
        };

        // Verify size matches
        if keys.len() != size || values.len() != size {
            return Err(format!(
                "Flatmap size mismatch: expected {}, got keys={}, values={}",
                size,
                keys.len(),
                values.len()
            ));
        }

        // Zip keys and values into a HashMap
        // For now, using string representation of keys
        let mut map = HashMap::new();
        for (key, value) in keys.into_iter().zip(values.into_iter()) {
            map.insert(format!("{}", key), value);
        }

        Ok(ParsedTerm::FlatMap(map))
    }

    /// Parse a hashmap head node (Mh).
    ///
    /// Format: `Mh<map_size>:<node_count>:<nodes_tuple>`
    fn parse_hashmap_head(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // Parse the format
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(format!("Invalid hashmap format: {}", data));
        }

        let map_size = usize::from_str_radix(parts[0], 16)
            .map_err(|e| format!("Invalid hashmap size: {}", e))?;
        let _node_count = usize::from_str_radix(parts[1], 16)
            .map_err(|e| format!("Invalid node count: {}", e))?;

        // Parse the nodes tuple
        let nodes_str = parts[2];
        let nodes_term = self.parse_tuple(addr, nodes_str)?;

        let nodes = match nodes_term {
            ParsedTerm::Tuple(ns) => ns,
            _ => return Err("Hashmap nodes not a tuple".to_string()),
        };

        // Flatten the hashmap nodes into key-value pairs
        let pairs = self.flatten_hashmap_nodes(&nodes)?;

        // Verify size
        if pairs.len() != map_size {
            return Err(format!(
                "Hashmap size mismatch: expected {}, got {}",
                map_size,
                pairs.len()
            ));
        }

        // Convert to HashMap
        let mut map = HashMap::new();
        for (key, value) in pairs {
            map.insert(format!("{}", key), value);
        }

        Ok(ParsedTerm::HashMap(map))
    }

    /// Flatten hashmap nodes into key-value pairs.
    ///
    /// Hashmap nodes can be:
    /// - Direct key-value pairs (2-tuples)
    /// - Interior nodes (Mn) containing more nodes
    fn flatten_hashmap_nodes(&mut self, nodes: &[ParsedTerm]) -> Result<Vec<(ParsedTerm, ParsedTerm)>, String> {
        let mut pairs = Vec::new();

        for node in nodes {
            match node {
                // A 2-tuple is a key-value pair
                ParsedTerm::Tuple(elems) if elems.len() == 2 => {
                    pairs.push((elems[0].clone(), elems[1].clone()));
                }
                // A tuple with more elements might contain nested nodes
                ParsedTerm::Tuple(elems) => {
                    // Recursively flatten
                    let nested = self.flatten_hashmap_nodes(elems)?;
                    pairs.extend(nested);
                }
                // Other terms are ignored (shouldn't happen in valid hashmap)
                _ => {}
            }
        }

        Ok(pairs)
    }

    /// Parse a hashmap interior node (Mn).
    ///
    /// Format: `Mn<count>:<elements_tuple>`
    fn parse_hashmap_node(&mut self, addr: u64, data: &str) -> Result<ParsedTerm, String> {
        // Interior nodes are intermediate - they get processed as part of Mh
        self.parse_tuple(addr, data)
    }

    /// Get a parsed term by address.
    pub fn get_term(&self, addr: u64) -> Option<&ParsedTerm> {
        self.term_dict.get(&addr)
    }

    /// Check if heap parsing was incomplete.
    pub fn is_incomplete(&self) -> bool {
        self.incomplete_heap
    }

    /// Get all parsed terms.
    pub fn get_all_terms(&self) -> &BTreeMap<u64, ParsedTerm> {
        &self.term_dict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer() {
        let parser = HeapParser::new(0, false);
        let result = parser.parse_integer("42");
        assert!(result.is_ok());
        if let Ok(ParsedTerm::Integer(val)) = result {
            assert_eq!(val, 42);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_parse_atom() {
        let parser = HeapParser::new(0, false);
        let result = parser.parse_atom("4:test");
        assert!(result.is_ok());
        if let Ok(ParsedTerm::Atom(s)) = result {
            assert_eq!(s, "test");
        } else {
            panic!("Expected atom");
        }
    }

    #[test]
    fn test_parse_nil() {
        let mut parser = HeapParser::new(0, false);
        let result = parser.parse_term("N");
        assert!(result.is_ok());
        matches!(result.unwrap(), ParsedTerm::Nil);
    }
}
