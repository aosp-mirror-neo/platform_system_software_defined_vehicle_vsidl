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
use google_sdv_service_bundles_registry::aidl::google::sdv::service_bundles_registry::ServiceBundleMetadata::ServiceBundleMetadata;
use sdv_comms::id::ServiceFqin;
use sdv_status_rs::SdvStatus;
use std::borrow::Cow;
use std::fmt;

#[derive(PartialEq, Eq, Clone, Default, Debug, Hash)]
pub(crate) struct ServiceFqbn<'string_lt> {
    package_name: Cow<'string_lt, str>,
    bundle_name: Cow<'string_lt, str>,
}

impl<'borrowed_lt> ServiceFqbn<'borrowed_lt> {
    pub(crate) fn from_fqin(fqin: &'borrowed_lt ServiceFqin) -> ServiceFqbn<'borrowed_lt> {
        ServiceFqbn {
            package_name: Cow::Borrowed(fqin.get_sdv_package_name()),
            bundle_name: Cow::Borrowed(fqin.get_service_bundle_name()),
        }
    }

    pub(crate) fn from_metadata(
        metadata: &ServiceBundleMetadata,
    ) -> Result<ServiceFqbn<'static>, SdvStatus> {
        Ok(ServiceFqbn {
            package_name: Cow::Owned(metadata.packageName.clone()),
            bundle_name: Cow::Owned(metadata.name.clone()),
        })
    }

    pub(crate) fn package_name(&self) -> &str {
        &self.package_name
    }

    pub(crate) fn bundle_name(&self) -> &str {
        &self.bundle_name
    }
}

impl<'a> fmt::Display for ServiceFqbn<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.package_name, self.bundle_name)
    }
}

impl<'fqin_lt> From<&'fqin_lt ServiceFqin> for ServiceFqbn<'fqin_lt> {
    fn from(fqin: &'fqin_lt ServiceFqin) -> Self {
        Self::from_fqin(fqin)
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Copy, Clone)]
pub(crate) enum DataType {
    SomeIp,
    Diagnostics,
    VsidlSchemas,
}

#[derive(Debug, Eq, Hash, PartialEq, Copy, Clone)]
pub(crate) enum DataSource {
    Files,
    Apexes,
    Agent,
}
