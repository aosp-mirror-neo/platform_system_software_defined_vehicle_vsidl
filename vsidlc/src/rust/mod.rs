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

mod android_bp;
mod generator;
mod generator_chain_builder;
mod generator_step;
mod generator_tool_state;
mod main_android_bp;
mod rpc_service;
mod service_bundle;
mod steps;
pub use generator::generate_middleware;
pub use generator::APEX_DIR;
pub use generator::CONFIGS_DIR;
pub use generator::GENERATED_RS_SOURCES_DIR;
pub use generator::SERVICES_DIR;

mod diagnostics;
