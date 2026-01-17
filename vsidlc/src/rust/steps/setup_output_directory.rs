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
use anyhow::Result;
use std::fs;

pub struct SetupOutputDirectory;

impl GeneratorStepExecution for SetupOutputDirectory {
    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let root_dir =
            state.cli.output_path.clone().unwrap_or_default().join(GENERATED_RS_SOURCES_DIR);
        fs::create_dir_all(&root_dir)?;
        Ok(())
    }
}
