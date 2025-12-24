// Copyright 2024 Google LLC
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
