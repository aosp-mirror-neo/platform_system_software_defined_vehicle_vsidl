// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::rust::generator::GENERATED_RS_SOURCES_DIR;
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::rust::main_android_bp;
use crate::util::write_bytes_to_file;
use crate::AndroidBpFileGroups;
use anyhow::Result;

fn get_main_catalog_name(filegroups: &AndroidBpFileGroups) -> String {
    if filegroups.catalog.is_empty() {
        panic!("--genrule requires a filegroup defining the full catalog");
    }

    // 1. Search for "catalog"
    let catalog_result = filegroups.catalog.clone().into_iter().find(|s| s.contains("catalog"));
    if let Some(result) = catalog_result {
        return result;
    }

    // 2. Search for "vsidl"
    let vsidl_result = filegroups.catalog.clone().into_iter().find(|s| s.contains("vsidl"));
    if let Some(result) = vsidl_result {
        return result;
    }

    // 3. Return the first element of the list
    filegroups.catalog[0].clone()
}

pub struct GenerateMainAndroidBp;

impl GeneratorStepExecution for GenerateMainAndroidBp {
    fn should_run(&self, state: &GeneratorToolState) -> bool {
        state.cli.genrule
    }

    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let main_genrule_name = get_main_catalog_name(&state.filegroups);
        let bp_content = main_android_bp::generate_main_android_bp(
            main_genrule_name.clone(),
            state.filegroups.clone(),
            &state.cli,
            state.handlebars.as_ref().unwrap(),
        )?;
        let bp_path = state
            .cli
            .output_path
            .clone()
            .unwrap_or_default()
            .join(GENERATED_RS_SOURCES_DIR)
            .join("Android.bp");
        write_bytes_to_file(&bp_content, &bp_path)?;
        state.main_genrule_name = Some(main_genrule_name);
        Ok(())
    }
}
