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
//! This crate provides visitors for traversing the VSIDL catalog

pub mod convert_channel_to_topic;
pub mod detect_input_version;
pub mod hide_channel_syntax;
pub mod hide_pdk29_syntax;
pub mod keyword_converter;
pub mod remove_non_google_options;
pub mod visitor;
pub mod visitor_mut;
