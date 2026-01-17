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

use crate::rust::generator_step::{GeneratorStep, GeneratorStepExecution};

/// A fluent builder to construct the chain of steps.
pub struct GeneratorChainBuilder {
    head: Option<Box<GeneratorStep>>,
}

impl GeneratorChainBuilder {
    /// Creates a new, empty GeneratorChainBuilder.
    pub fn new() -> Self {
        Self { head: None }
    }

    /// Adds a new step to the end of the chain.
    pub fn add_step(mut self, step_action: Box<dyn GeneratorStepExecution>) -> Self {
        let new_step = Box::new(GeneratorStep::new(step_action));

        match self.head.as_mut() {
            Some(head) => {
                let mut current = head;
                // Traverse to the last node
                while current.next_step.is_some() {
                    current = current.next_step.as_mut().unwrap();
                }
                // Set the next step of the last node
                current.set_next(new_step);
            }
            None => {
                self.head = Some(new_step);
            }
        }
        self
    }

    /// Builds the chain and returns the head of the chain.
    pub fn build(self) -> Option<Box<GeneratorStep>> {
        self.head
    }
}

impl Default for GeneratorChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}
