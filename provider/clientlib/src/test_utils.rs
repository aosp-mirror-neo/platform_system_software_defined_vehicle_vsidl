// Copyright 2024 Google LLC
//! Provides functions for unit tests.

use crate::types::DataType;
use crate::vsidl_provider_api::VsidlConfigProviderV1;
use googletest::prelude::*;
use log::info;
use protobuf::MessageField;
use sdv_comms::ContextRef;
use sdv_comms::{id::ServiceFqin, sd::UnitType};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::{routine::Method, DataItem, Routine};
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;

/// Enum to represent the different subpackage configurations for testing.
#[derive(Clone, Copy, Debug)]
pub enum Subpackage {
    PerBundle,
    Shared,
}

impl Subpackage {
    /// Returns an iterator over all variants of the enum.
    pub fn all() -> impl Iterator<Item = Self> {
        [Subpackage::PerBundle, Subpackage::Shared].into_iter()
    }
}

impl fmt::Display for Subpackage {
    /// Formats the enum variant into its string representation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subpackage::PerBundle => write!(f, "apex_with_per_bundle_config_file"),
            Subpackage::Shared => write!(f, "apex_with_shared_config_file"),
        }
    }
}

fn get_vm_name(ctx: Option<ContextRef>) -> String {
    match ctx {
        Some(ctx) => ctx.get_self_fqin().get_sdv_vm_name().to_owned(),
        None => "ignored".to_owned(),
    }
}

fn diag_fqin(ctx: Option<ContextRef>, subpackage: &Subpackage) -> ServiceFqin {
    ServiceFqin::from_str(&format!(
        "{}:com.sdv.google.sample.vsidl_provider.{}.DiagService/ignored",
        get_vm_name(ctx),
        subpackage
    ))
    .unwrap()
}

fn someip_config_fqin(ctx: Option<ContextRef>) -> ServiceFqin {
    ServiceFqin::from_str(&(get_vm_name(ctx) + ":com.sdv.someip.SomeIpBroker/ignored")).unwrap()
}

fn non_existent_fqin(ctx: Option<ContextRef>) -> ServiceFqin {
    ServiceFqin::from_str(
        &(get_vm_name(ctx) + ":com.sdv.google.sample.non_existent.NonExistent/ignored"),
    )
    .unwrap()
}

fn source_fqin_hello_service(ctx: Option<ContextRef>) -> ServiceFqin {
    ServiceFqin::from_str(
        &(get_vm_name(ctx) + ":com.sdv.google.sample.vsidl_provider.apex_with_per_bundle_config_file.HelloService/ignored"),
    )
    .unwrap()
}

fn source_fqin_manager_service(ctx: Option<ContextRef>) -> ServiceFqin {
    ServiceFqin::from_str(
        &(get_vm_name(ctx) + ":com.sdv.google.sample.vsidl_provider.apex_with_per_bundle_config_file.Manager/ignored"),
    )
    .unwrap()
}

fn source_fqin_monitor_service(ctx: Option<ContextRef>) -> ServiceFqin {
    ServiceFqin::from_str(
        &(get_vm_name(ctx) + ":com.sdv.google.sample.vsidl_provider.apex_with_per_bundle_config_file.Monitor/ignored"),
    )
    .unwrap()
}

pub async fn test_fixture_vsidl_init_succeeds<TProvider: VsidlConfigProviderV1>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    assert_eq!(provider.get_someip_mappings(&non_existent_fqin(ctx)).await, None);

    let someip_query = provider.get_someip_mappings(&someip_config_fqin(ctx)).await;
    if data_types.contains(&DataType::SomeIp) {
        assert!(someip_query.is_some());
    } else {
        assert!(someip_query.is_none());
    }

    // Iterate over all subpackages to test diag_fqin
    for subpackage in Subpackage::all() {
        let diag_query = provider.get_diagnostics_declaration(&diag_fqin(ctx, &subpackage)).await;
        if data_types.contains(&DataType::Diagnostics) {
            assert!(
                diag_query.is_some(),
                "Diagnostics query failed for subpackage: {:?}",
                subpackage
            );
        } else {
            assert!(
                diag_query.is_none(),
                "Diagnostics query should be None for subpackage: {:?}",
                subpackage
            );
        }
    }
}

pub async fn test_fixture_vsidl_provider_diagnostics_api_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    // Iterate over all subpackages to test diag_fqin
    for subpackage in Subpackage::all() {
        let declaration = provider.get_diagnostics_declaration(&diag_fqin(ctx, &subpackage)).await;

        if !data_types.contains(&DataType::Diagnostics) {
            assert!(
                declaration.is_none(),
                "Declaration should be None for subpackage: {:?}",
                subpackage
            );
            continue;
        }

        assert_that!(
            declaration.unwrap(),
            eq(&DiagnosticsDeclaration {
                data_item: vec![DataItem {
                    id: "data_item".to_string(),
                    message_name: "com.sdv.google.sample.vsidl_provider.DataItem".to_string(),
                    ..Default::default()
                }],
                routine: vec![Routine {
                    id: "routine".to_string(),
                    start: MessageField::some(Method {
                        request: "com.sdv.google.sample.vsidl_provider.DataItem".to_string(),
                        response: "google.protobuf.Timestamp".to_string(),
                        ..Default::default()
                    }),
                    stop: MessageField::some(Method {
                        request: "google.protobuf.Empty".to_string(),
                        response: "com.sdv.google.sample.vsidl_provider.TirePressure".to_string(),
                        ..Default::default()
                    }),
                    result: MessageField::some(Method {
                        request: "google.protobuf.Empty".to_string(),
                        response: "google.protobuf.Empty".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            })
        );
    }
}

pub async fn test_fixture_vsidl_provider_pubsub_metadata_api_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    let topic_metadata = provider
        .get_publication_descriptor(
            &source_fqin_monitor_service(ctx),
            &UnitType::from_str("com.sdv.google.sample.vsidl_provider.Monitor#TirePressureRange")
                .unwrap(),
        )
        .await;

    if data_types.contains(&DataType::VsidlSchemas) {
        assert!(topic_metadata.is_some());
        assert_eq!(
            topic_metadata.unwrap().message_descriptor.full_name(),
            "com.sdv.google.sample.vsidl_provider.TirePressureRange"
        );
    } else {
        assert!(topic_metadata.is_none());
    }
}

pub async fn test_fixture_vsidl_provider_rpc_descriptor_api_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    let rpc_data_result = provider
        .get_rpc_method_descriptor(
            &source_fqin_hello_service(ctx),
            &UnitType::from_str("com.sdv.google.sample.vsidl_provider.HelloService#HelloInterface")
                .unwrap(),
            "SayHello",
        )
        .await;
    info!("{rpc_data_result:?}");

    if data_types.contains(&DataType::VsidlSchemas) {
        assert!(rpc_data_result.is_some());
        let rpc_data = rpc_data_result.unwrap();
        assert_eq!(
            rpc_data.request_message.unwrap().full_name(),
            "com.sdv.google.sample.vsidl_provider.HelloMessageRequest"
        );
        assert_eq!(
            rpc_data.response_message.unwrap().full_name(),
            "com.sdv.google.sample.vsidl_provider.HelloMessageResponse"
        );
    } else {
        assert!(rpc_data_result.is_none());
    }
}

pub async fn test_fixture_vsidl_provider_api_fails<TProvider: VsidlConfigProviderV1>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    _data_types: HashSet<DataType>,
) {
    assert!(provider
        .get_publication_descriptor(
            &source_fqin_hello_service(ctx),
            &UnitType::from_str("sdv.non_existent.NonExistent#NonExistent").unwrap()
        )
        .await
        .is_none());
}

pub async fn test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    let topic_metadata = provider
        .get_publication_descriptor(
            &source_fqin_manager_service(ctx),
            &UnitType::from_str("com.sdv.google.sample.vsidl_provider.Manager#TirePressure")
                .unwrap(),
        )
        .await;

    if data_types.contains(&DataType::VsidlSchemas) {
        assert!(topic_metadata.is_some());
        assert_eq!(
            topic_metadata.unwrap().message_descriptor.full_name(),
            "com.sdv.google.sample.vsidl_provider.TirePressure"
        );
    } else {
        assert!(topic_metadata.is_none());
    }
}

pub async fn test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    let rpc_data_result = provider
        .get_rpc_method_descriptor(
            &source_fqin_hello_service(ctx),
            &UnitType::from_str("com.sdv.google.sample.vsidl_provider.HelloService#HelloInterface")
                .unwrap(),
            "Foo",
        )
        .await;

    if !data_types.contains(&DataType::VsidlSchemas) {
        assert!(rpc_data_result.is_none());
        return;
    }

    assert!(rpc_data_result.is_some());
    let rpc_data = rpc_data_result.unwrap();
    assert_eq!(rpc_data.request_message.unwrap().full_name(), "google.protobuf.Empty");
    assert_eq!(rpc_data.response_message.unwrap().full_name(), "google.protobuf.BoolValue");
}

pub async fn test_fixture_vsidl_provider_message_descriptor_api_succeeds<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    data_types: HashSet<DataType>,
) {
    let message_descriptor = provider
        .get_message_descriptor(
            &source_fqin_manager_service(ctx),
            "com.sdv.google.sample.vsidl_provider.TirePressure",
        )
        .await;

    if data_types.contains(&DataType::VsidlSchemas) {
        assert!(message_descriptor.is_some());
        assert_eq!(
            message_descriptor.unwrap().full_name(),
            "com.sdv.google.sample.vsidl_provider.TirePressure"
        );
    } else {
        assert!(message_descriptor.is_none());
    }
}

pub async fn test_fixture_vsidl_provider_message_descriptor_api_fails<
    TProvider: VsidlConfigProviderV1,
>(
    provider: TProvider,
    ctx: Option<ContextRef>,
    _data_types: HashSet<DataType>,
) {
    let message_descriptor = provider
        .get_message_descriptor(
            &source_fqin_manager_service(ctx),
            "com.sdv.google.sample.vsidl_provider.NonExistent",
        )
        .await;
    assert!(message_descriptor.is_none());
}
