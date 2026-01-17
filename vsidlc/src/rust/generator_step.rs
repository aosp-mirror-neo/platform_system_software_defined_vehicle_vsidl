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

use crate::rust::generator_tool_state::GeneratorToolState;
use anyhow::Result;

/// Defines the contract for a single step in the compilation chain.
pub trait GeneratorStepExecution {
    /// Determines if this step should be executed.
    fn should_run(&self, _state: &GeneratorToolState) -> bool {
        true // Default to always run
    }

    fn action(&self, state: &mut GeneratorToolState) -> Result<()>;
}

/// A node in the chain, holding a step's implementation and a link to the next step.
pub struct GeneratorStep {
    action_impl: Box<dyn GeneratorStepExecution>,
    pub next_step: Option<Box<GeneratorStep>>,
}

impl GeneratorStep {
    /// Creates a new GeneratorStep.
    pub fn new(action_impl: Box<dyn GeneratorStepExecution>) -> Self {
        Self { action_impl, next_step: None }
    }

    /// Sets the next step in the chain.
    pub fn set_next(&mut self, next_step: Box<GeneratorStep>) {
        self.next_step = Some(next_step);
    }

    /// Executes its own action and then passes control to the next step.
    pub fn execute(&self, state: &mut GeneratorToolState) -> Result<()> {
        if self.action_impl.should_run(state) {
            self.action_impl.action(state)?;
        }
        if let Some(next_step) = &self.next_step {
            next_step.execute(state)?;
        }
        Ok(())
    }
}
