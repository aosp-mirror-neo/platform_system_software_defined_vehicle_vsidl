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

use crate::local_provider_from_apexes::LocalProviderFromApexes;
use crate::local_provider_from_files::LocalProviderFromFiles;
use crate::provider_from_agent::ProviderFromAgent;
use crate::types::DataType;
// use crate::types::DataSource;
use crate::vsidl_provider_api::VsidlConfigProvider;
use log::warn;
use mockall::automock;
use sdv_comms::ContextRef;
use std::collections::HashSet;

/// Abstract factory for VsidlProvider sub-implementations
#[automock]
pub(crate) trait Factory {
    /// Implementation fetching data from local VM system files.
    fn provider_from_files(
        &self,
        data_types: HashSet<DataType>,
        source_directory_path: &str,
    ) -> Option<Box<dyn VsidlConfigProvider>>;

    /// Implementation fetching data from local VM APEX files.
    fn provider_from_apexes(
        &self,
        data_types: HashSet<DataType>,
    ) -> Option<Box<dyn VsidlConfigProvider>>;

    /// Implementation fetching data from VSIDL provider Agent on the VM corresponding to the FQIN in the request.
    ///
    /// This implementation will provide data from local VM Agent, in case FQIN is from local VM.
    fn provider_from_agent(
        &self,
        data_types: HashSet<DataType>,
        ctx: ContextRef,
    ) -> Option<Box<dyn VsidlConfigProvider>>;

    /// Implementation fetching data from VSIDL provider Agent on the remote VM corresponding to the FQIN in the request.
    ///
    /// This implementation will *not* provide data from local VM Agent, allowing to fallback for in-process implementations of VsidlProvider (i.e. provider from APEXes or files).
    fn provider_from_remote_agent(
        &self,
        data_types: HashSet<DataType>,
        ctx: ContextRef,
    ) -> Option<Box<dyn VsidlConfigProvider>>;
}

pub(crate) struct FactoryImpl;

impl Factory for FactoryImpl {
    fn provider_from_agent(
        &self,
        data_types: HashSet<DataType>,
        ctx: ContextRef,
    ) -> Option<Box<dyn VsidlConfigProvider>> {
        Some(Box::new(ProviderFromAgent::new_from_agent(ctx, data_types)))
    }

    fn provider_from_remote_agent(
        &self,
        data_types: HashSet<DataType>,
        ctx: ContextRef,
    ) -> Option<Box<dyn VsidlConfigProvider>> {
        Some(Box::new(ProviderFromAgent::new_from_remote_agent(ctx, data_types)))
    }

    fn provider_from_apexes(
        &self,
        data_types: HashSet<DataType>,
    ) -> Option<Box<dyn VsidlConfigProvider>> {
        match LocalProviderFromApexes::new(data_types) {
            Ok(local_provider_from_apexes) => Some(Box::new(local_provider_from_apexes)),
            Err(e) => {
                warn!("Failed to create local provider from apexes: {e:?}");
                None
            }
        }
    }

    fn provider_from_files(
        &self,
        data_types: HashSet<DataType>,
        source_directory_path: &str,
    ) -> Option<Box<dyn VsidlConfigProvider>> {
        Some(Box::new(LocalProviderFromFiles::new(source_directory_path, data_types)))
    }
}
