// Copyright 2024 Google LLC

use crate::vsidl_provider::VsidlProviderErrorV1;
use protobuf::Message;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

pub fn directory_files(directory_name: &str) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(directory_name)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry: Result<DirEntry, _>| entry.ok())
        .filter(|entry: &DirEntry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
}

pub fn read_config_file<T: protobuf::Message, P: AsRef<Path>>(
    file_name: P,
) -> Result<T, VsidlProviderErrorV1> {
    let config = File::open(&file_name)?;
    let mut reader = BufReader::new(config);
    if let Ok(message) = Message::parse_from_reader(&mut reader) {
        Ok(message)
    } else {
        Err(VsidlProviderErrorV1::ConfigParseError(file_name.as_ref().display().to_string()))
    }
}
