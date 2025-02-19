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
use std::path::Path;
use std::path::PathBuf;

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
                Some(tag_id_cow.into_owned())
            } else {
                None
            };

            self.matches.push((tag_enum, tag_id_string, byte_offset));
        }
        Ok(true)
    }
}

pub struct CDParser {
    //file: File,
    // mmap: Mmap,
    filepath: PathBuf,
    filename: String,
    index: Vec<String>,
}

impl CDParser {
    pub fn new(filepath: &str) -> Result<Self, io::Error> {
        let (filepath, filename) = Self::split_path_and_filename(filepath)?;
        let realpath = filepath.join(&filename);
        // need to figure out mmap later
        // let mmap = unsafe { Mmap::map(&file)? };

        Ok(CDParser {
            //file,
            // mmap,
            filepath: realpath,
            filename,
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

    // TODO: fix this to accomodate a vector
    pub fn build_index(&self) -> Result<IndexMap, io::Error> {
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
    pub fn parse(&mut self) -> io::Result<CrashDump> {
        let index_map = self.build_index()?;
        let crash_dump = CrashDump::from_index_map(&index_map, &self.filepath);

        crash_dump
    }

    fn split_path_and_filename(filepath: &str) -> Result<(PathBuf, String), io::Error> {
        let path = Path::new(filepath);
        let filepath = path.parent().unwrap_or(Path::new("."));
        let filename = path.file_name().unwrap_or(OsStr::new("")).to_string_lossy();
        Ok((filepath.to_path_buf(), filename.into_owned()))
    }
}
