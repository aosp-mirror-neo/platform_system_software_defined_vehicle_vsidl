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
//! This crate provides parser logic for VSIDL catalog

pub mod parser;
pub use parser::*;
// Reexport the dependent libraries so that 'vsidl_v1_parser'
// can stay a common crate name despite being internally split into
// multiple libraries for faster compile times
pub use vsidl_v1_common::*;
pub use vsidl_v1_context_conditions::get_explanation_doc;
pub use vsidl_v1_context_conditions::AndroidBpDanglingFiles;
pub use vsidl_v1_context_conditions::ContextCondition;
pub use vsidl_v1_context_conditions::ContextConditionChecker;
pub use vsidl_v1_context_conditions::Finding;
pub use vsidl_v1_context_conditions::FindingCollector;
pub use vsidl_v1_context_conditions::FindingId;
pub use vsidl_v1_context_conditions::FindingSeverity;
pub use vsidl_v1_pest_parser::*;
pub use vsidl_v1_symbols::*;
