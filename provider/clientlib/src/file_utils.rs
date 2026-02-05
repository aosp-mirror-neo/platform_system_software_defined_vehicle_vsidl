/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use protobuf::Message;
use sdv::status::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
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

pub fn read_config_file<T: protobuf::Message, P: AsRef<Path>>(file_name: P) -> SdvResult<T> {
    let config =
        File::open(&file_name).map_err(|e| SdvStatus::with_cause(SdvStatusCode::Internal, e))?;
    let mut reader = BufReader::new(config);
    if let Ok(message) = Message::parse_from_reader(&mut reader) {
        Ok(message)
    } else {
        Err(SdvStatus::with_message(
            SdvStatusCode::Internal,
            "Config Parse Error of file: {file_name:?}",
        ))
    }
}
