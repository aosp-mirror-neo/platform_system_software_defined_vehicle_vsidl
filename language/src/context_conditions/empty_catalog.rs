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

use super::{FindingCollector, FindingId};
use crate::impl_context_condition;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::Catalog;

#[derive(Debug, Default)]
pub struct EmptyCatalog {
    finding_collector: FindingCollector,
}

impl_context_condition!(EmptyCatalog);

impl VsidlVisitor for EmptyCatalog {
    fn start_visit_catalog(&mut self, catalog: &Catalog) {
        if catalog.catalog_vsidl_files.is_empty() {
            if let Some(path) = &catalog.catalog_path {
                self.finding_collector.warn(
                    FindingId::EmptyCatalog,
                    format!(
                        "No .vsidl files detected in catalog path '{}'. Code generation will be skipped.",
                        path.display()
                    ),
                    None, vec![],
                );
            }
            return;
        }

        let mut is_effectively_empty = true;
        for (_, entry) in &catalog.catalog_vsidl_files {
            if !entry.service_bundle.is_empty()
                || !entry.some_ip_mapping.is_empty()
                || !entry.vhal_mapping.is_empty()
            {
                is_effectively_empty = false;
                break;
            }
        }

        if is_effectively_empty {
            if let Some(path) = &catalog.catalog_path {
                self.finding_collector.warn(
                    FindingId::EmptyCatalog,
                    format!(
                        "All .vsidl files in catalog path '{}' are empty. Code generation will be skipped.",
                        path.display()
                    ),
                    None, vec![],
                );
            }
        }
    }
}
