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

use crate::Formatter;
use chrono::Datelike;

pub struct PrettyPlease {}

impl Formatter for PrettyPlease {
    fn format(&mut self, input: &str) -> anyhow::Result<String> {
        let syntax_tree = syn::parse_str(input).unwrap();
        // the syntax tree has no comments, therefore here we prepend to the formatted code
        // the first lines of the input which are the Android copyright statement
        Ok(format!(
            r##"// Copyright (C) {} The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
{}"##,
            get_current_year(),
            prettyplease::unparse(&syntax_tree)
        ))
    }
}

fn get_current_year() -> i32 {
    chrono::Utc::now().year()
}
