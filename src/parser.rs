use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::collections::HashMap;
use std::io::Read;


pub struct CDParser {
    //file: File,
    // mmap: Mmap,
    filepath: PathBuf,
    filename: String,
    crash_dump_sections: CrashDumpSections,
    index: Vec<String>
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

    pub fn parse(&self) -> Result<Vec<String>, io::Error> {
        let mut contents = Vec::new();
        let realpath = self.filepath.join(&self.filename);
        let mut file = File::open(realpath)?;
        file.read_to_end(&mut contents)?;
        let lines: Vec<&str> = contents.split(|c| *c == b'\n').map(|s| std::str::from_utf8(s).unwrap()).collect();
    
        Ok(lines.par_iter().enumerate().filter_map(|(index, line)| {
            if line.starts_with('=') {
                //Some((index, line.to_string()))
                Some(format!("{} : {}", index, line.to_string()))
            } else {
                None
            }
        }).collect())
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
