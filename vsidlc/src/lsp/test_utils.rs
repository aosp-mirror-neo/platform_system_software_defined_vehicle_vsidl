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

use crate::Cli;
use ::formatter::Format;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

pub(crate) fn setup_test_catalog() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a dummy VSIDL file
    let vsidl_path = temp_dir.path().join("test.vsidl");
    let mut vsidl_file = File::create(&vsidl_path).expect("Failed to create vsidl file");
    let file_content = r##"package: "com.sdv.tm"
service_bundle {
  name: "Manager"

  publisher {
    message: "com.sdv.tm.TirePressure"
    channel: "FRONT_RIGHT"
    topic: "front-right"
    capacity: 8
  }

  subscriber {
    message: "TirePressure"
    channel: "FRONT_RIGHT"
    topic: "front-right"
  }
}"##;
    vsidl_file.write_all(file_content.as_bytes()).expect("Failed to write to vsidl file");

    // Create a dummy proto file
    let proto_dir = temp_dir.path().join("protos");
    std::fs::create_dir(&proto_dir).expect("Failed to create protos dir");
    let proto_path = proto_dir.join("test.proto");
    let mut proto_file = File::create(&proto_path).expect("Failed to create proto file");
    let file_content = r##"syntax = "proto3";
package com.sdv.tm;

import "google/protobuf/empty.proto";
import "sdv/vsidl/v1/annotations.proto";

enum Location {
  FRONT_LEFT = 0;
  FRONT_RIGHT = 1;
}

/// Here's a multi-line
/// Doc String for TirePressure
message TirePressure {
  option (.sdv.vsidl.v1.publication) = {
    instances_enum: "Location"
    message_count: 8
    model: SINGLE_PUB
  };

  uint32 pressure = 1;
}

service TestService { rpc Do(google.protobuf.Empty) returns (google.protobuf.Empty); }"##;
    proto_file.write_all(file_content.as_bytes()).expect("Failed to write to proto file");

    // Create a dummy Android.bp file
    let android_bp_path = temp_dir.path().join("Android.bp");
    let mut android_bp_file = File::create(&android_bp_path).expect("Failed to create Android.bp");
    let file_content = r##"rust_protobuf {
name: "test_proto"
protos: ["protos/*.proto"],
crate_name: "test_proto",
}
    "##;
    android_bp_file.write_all(file_content.as_bytes()).expect("Failed to write to Android.bp");

    temp_dir
}

#[allow(dead_code)]
pub(crate) fn setup_mock_cli(temp_dir: &TempDir) -> Cli {
    Cli {
        catalog_path: Some(PathBuf::from(temp_dir.path())),
        output_path: None,
        purge: false,
        ignore: Vec::new(),
        no_pest: false,
        dependency_catalog_path: Vec::new(),
        rust_formatter: Format::None,
        explain: None,
        genrule: false,
        apex: false,
        services: false,
        target_api: "pdk29".to_string(),
        no_warn: true,
        use_acls: false,
        lsp: true,
    }
}
