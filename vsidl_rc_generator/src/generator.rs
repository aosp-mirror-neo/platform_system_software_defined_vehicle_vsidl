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

use std::collections::hash_map::VacantEntry;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use protobuf::descriptor::FileDescriptorProto;
use protobuf::Message;
use vsidl_v1_parser::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_parser::proto_publication_symbol::ProtoPublicationSymbol;
use vsidl_v1_parser::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_parser::publisher_symbol::PublisherSymbol;
use vsidl_v1_parser::service_bundle_symbol::ServiceBundleSymbol;

use crate::android_bp;
use chrono::Datelike;
use sdv_vsidl_common::WELLKNOWN_PROTOS_TO_DESCRIPTOR;
use tinytemplate::TinyTemplate;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::runtime_config::vsidl_runtime_config::unit_type_descriptor_mapping::Archetype_metadata;
use vsidl_v1::runtime_config::vsidl_runtime_config::{
    FieldSizeConstraint, RpcMetadata, RpcMethodMapping, TopicMetadata,
};
use vsidl_v1::runtime_config::vsidl_runtime_config::{
    MessageDescriptorMapping, UnitTypeDescriptorMapping,
};
use vsidl_v1::runtime_config::{SomeipRuntimeConfig, VsidlRuntimeConfig};
use vsidl_v1_parser::{Catalog, DiagnosticsInfo, ProtoMessage, SymbolTable};

const VSIDL_SYNTAX_PROTO: &str = "sdv/vsidl/v1/syntax.proto";
const VSIDL_ANNOTATIONS_PROTO: &str = "sdv/vsidl/v1/annotations.proto";
const DIAG_SYNTAX_PROTO: &str = "sdv/diagnostics/v1/diagnostics_syntax.proto";
const DIAG_ANNOTATIONS_PROTO: &str = "sdv/diagnostics/v1/diagnostics.proto";

#[derive(Debug, Eq, PartialEq, Hash)]
struct UnitType {
    /// Package name
    pub package: String,
    /// [Owning] Service bundle name
    pub service_bundle: String,
    /// Type name
    pub typename: String,
}

impl From<(&ProtoMessageSymbol, &SymbolTable)> for UnitType {
    fn from((symbol, symbol_table): (&ProtoMessageSymbol, &SymbolTable)) -> UnitType {
        UnitType {
            package: symbol.package(symbol_table).clone(),
            service_bundle: "".to_string(), // deprecated
            typename: symbol.name().clone(),
        }
    }
}

impl From<(&ProtoPublicationSymbol, &SymbolTable)> for UnitType {
    fn from((symbol, symbol_table): (&ProtoPublicationSymbol, &SymbolTable)) -> UnitType {
        UnitType {
            package: symbol.package(symbol_table).clone(),
            service_bundle: "".to_string(), // deprecated
            typename: symbol.name().clone(),
        }
    }
}

impl From<(&ProtoRpcServiceSymbol, &SymbolTable)> for UnitType {
    fn from((symbol, symbol_table): (&ProtoRpcServiceSymbol, &SymbolTable)) -> UnitType {
        UnitType {
            package: symbol.package(symbol_table).clone(),
            service_bundle: "".to_string(), // deprecated
            typename: symbol.name().clone(),
        }
    }
}

/// Generate runtime configuration files from the catalog. Generated targets are written into respective folder inside `output_path`

pub fn generate_runtime_config(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &Vec<ServiceBundleSymbol>,
    output_path: &Path,
    service_bundle_name: Option<String>,
    package_name: Option<String>,
    target_api: &str,
) -> anyhow::Result<()> {
    match (service_bundle_name.as_deref(), package_name.as_deref()) {
        (Some(bundle_name), Some(pkg_name)) => generate_for_package_and_bundle(
            catalog,
            symbol_table,
            bundles,
            output_path,
            bundle_name,
            pkg_name,
            target_api,
        ),

        (None, Some(pkg_name)) => {
            generate_for_package(catalog, symbol_table, bundles, output_path, pkg_name, target_api)
        }

        (None, None) => {
            generate_for_all_bundles(catalog, symbol_table, bundles, output_path, target_api)
        }

        (Some(bundle_name), None) => generate_for_package_and_bundle(
            catalog,
            symbol_table,
            bundles,
            output_path,
            bundle_name,
            try_find_package_for_bundle_config(symbol_table, bundles, bundle_name)?.as_str(),
            target_api,
        ),
    }
}

fn try_find_package_for_bundle_config(
    symbol_table: &SymbolTable,
    bundles: &[ServiceBundleSymbol],
    service_bundle_name: &str,
) -> anyhow::Result<String> {
    let mut service_bundles_filter =
        bundles.iter().filter(|service_bundle| service_bundle.name == service_bundle_name);

    let Some(service_bundle) = service_bundles_filter.next() else {
        return Err(anyhow::Error::msg(format!(
            "Unable to find service bundle with name {service_bundle_name}"
        )));
    };

    if service_bundles_filter.next().is_some() {
        return Err(anyhow::Error::msg(format!(
            "There are multiple bundles in different packages with service bundle name {service_bundle_name}"
        )));
    }

    Ok(service_bundle.package(symbol_table))
}

/// Generates vsidl-config and diagnostics-config for a single, specific bundle.
fn generate_for_package_and_bundle(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &[ServiceBundleSymbol],
    output_path: &Path,
    service_bundle_name: &str,
    package_name: &str,
    target_api: &str,
) -> anyhow::Result<()> {
    let filtered_bundles = bundles
        .iter()
        .filter(|service_bundle| service_bundle.package(symbol_table) == package_name)
        .filter(|service_bundle| service_bundle.name == service_bundle_name)
        .cloned()
        .collect::<Vec<_>>();

    if filtered_bundles.is_empty() {
        return Err(anyhow::Error::msg(format!(
            "There is no bundle named {service_bundle_name} in package {package_name}"
        )));
    }

    if filtered_bundles.len() != 1 {
        return Err(anyhow::Error::msg(format!(
            "There are multiple bundles with service bundle name {service_bundle_name}"
        )));
    }

    generate_vsidl_runtime_config_to_file(
        catalog,
        symbol_table,
        &filtered_bundles,
        &output_path.join("vsidl-config.binpb"),
        target_api,
    )?;

    generate_diagnostics_declaration_config_to_file(
        filtered_bundles[0].clone(),
        &output_path.join("diagnostics-config.binpb"),
    )?;

    Ok(())
}

/// Generates vsidl-config for all bundles within that package.
fn generate_for_package(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &[ServiceBundleSymbol],
    output_path: &Path,
    package_name: &str,
    target_api: &str,
) -> anyhow::Result<()> {
    let filtered_bundles = bundles
        .iter()
        .filter(|service_bundle| service_bundle.package(symbol_table) == package_name)
        .cloned()
        .collect::<Vec<_>>();

    if filtered_bundles.is_empty() {
        return Err(anyhow::Error::msg(format!("There are no bundles in package {package_name}")));
    }

    generate_vsidl_runtime_config_to_file(
        catalog,
        symbol_table,
        &filtered_bundles,
        &output_path.join("vsidl-config.binpb"),
        target_api,
    )?;

    Ok(())
}

/// Generates global vsidl-config and someip-config.
fn generate_for_all_bundles(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &Vec<ServiceBundleSymbol>,
    output_path: &Path,
    target_api: &str,
) -> anyhow::Result<()> {
    let print_path: &_ = &output_path.display();
    eprintln!("Generating runtime configuration to folder '{print_path}'");

    generate_vsidl_runtime_config_to_file(
        catalog,
        symbol_table,
        bundles,
        &output_path.join("vsidl-config.binpb"),
        target_api,
    )?;
    generate_some_ip_mapping_to_file(catalog, &output_path.join("someip-config.binpb"))?;

    eprintln!("Generated runtime configuration is written to folder '{print_path}'");
    Ok(())
}

/// Writes the content to the file with path, if content is different
fn write_proto_to_file<T: Message>(proto: T, path: &Path) -> anyhow::Result<()> {
    let content = proto.write_to_bytes().unwrap();

    // Skips if the same content already exists in the path.
    if let Ok(existing_content) = fs::read(path) {
        if existing_content == content {
            return Ok(());
        }
    }
    fs::write(path, &content).with_context(|| format!("Failed to save file: {path:#?}"))
}

fn generate_vsidl_runtime_config_to_file(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &Vec<ServiceBundleSymbol>,
    file_path: &Path,
    target_api: &str,
) -> anyhow::Result<()> {
    let runtime_config_proto =
        generate_vsidl_runtime_config(catalog, symbol_table, bundles, target_api)?;
    write_proto_to_file(runtime_config_proto, file_path)
}

fn generate_vsidl_runtime_config(
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &Vec<ServiceBundleSymbol>,
    target_api: &str,
) -> anyhow::Result<VsidlRuntimeConfig> {
    let mut vsidl_runtime_config = VsidlRuntimeConfig::new();
    let mut unit_type_cache = HashMap::new();
    for service_bundle_symbol in bundles {
        add_unit_types_from_service_bundle(
            service_bundle_symbol.clone(),
            symbol_table,
            &mut unit_type_cache,
            target_api,
        );
    }
    for (unit_type, archetype_metadata) in &unit_type_cache {
        let mut unit_type_descriptor_mapping = UnitTypeDescriptorMapping::new();
        unit_type_descriptor_mapping.package_name.clone_from(&unit_type.package);
        unit_type_descriptor_mapping.service_bundle_name.clone_from(&unit_type.service_bundle);
        unit_type_descriptor_mapping.type_name.clone_from(&unit_type.typename);
        unit_type_descriptor_mapping.archetype_metadata = Some(archetype_metadata.clone());

        vsidl_runtime_config.unit_type_mapping.push(unit_type_descriptor_mapping);
    }

    let mut proto_filename_to_descriptor = HashMap::new();
    for (_, file_descriptor_proto) in &catalog.proto_descriptors {
        proto_filename_to_descriptor.insert(
            file_descriptor_proto.name.as_ref().unwrap().to_string(),
            file_descriptor_proto.clone(),
        );
    }
    // All FileDescriptorProto objects used by the catalog, indexed by their file name. When parsing all service bundles in the catalog, the FileDescriptorProto values of this map should be equivalent
    // to the FileDescriptorProto values in `catalog.proto_descriptors`
    let mut fd_protos_cache = HashMap::new();
    // All top level proto messages used by the catalog and the name of the file that contains their proto definition
    let mut message_cache = HashMap::new();
    for service_bundle in bundles {
        add_schemas_from_service_bundle(
            service_bundle.clone(),
            symbol_table,
            &proto_filename_to_descriptor,
            &mut fd_protos_cache,
            &mut message_cache,
            target_api,
        );
    }

    for (message_name, proto_filename) in &message_cache {
        let mut message_descriptor_mapping = MessageDescriptorMapping::new();
        message_descriptor_mapping.message_name.clone_from(message_name);
        message_descriptor_mapping.file_name.clone_from(proto_filename);

        vsidl_runtime_config.descriptor_mapping.push(message_descriptor_mapping);
    }

    for file_descriptor_proto in fd_protos_cache.values() {
        vsidl_runtime_config.file_descriptor_proto.push(file_descriptor_proto.clone());
    }
    Ok(vsidl_runtime_config)
}

fn add_schemas_from_service_bundle(
    service_bundle: ServiceBundleSymbol,
    symbol_table: &SymbolTable,
    proto_filename_to_descriptor: &HashMap<String, FileDescriptorProto>,
    fd_protos_cache: &mut HashMap<String, FileDescriptorProto>,
    message_cache: &mut HashMap<String, String>,
    target_api: &str,
) {
    for publisher in &service_bundle.publishers(symbol_table) {
        let (message_name, proto_file_name) = if target_api == "latest" {
            let message = publisher.message(symbol_table);
            (message.full_name(symbol_table), message.proto_metadata.filename.clone())
        } else {
            let publication = publisher.publication(symbol_table);
            (publication.full_name(symbol_table), publication.proto_metadata.filename.clone())
        };

        add_fd_protos_from_publication(
            message_name,
            proto_file_name,
            proto_filename_to_descriptor,
            fd_protos_cache,
            message_cache,
        );
    }
    for subscriber in &service_bundle.subscribers(symbol_table) {
        let (message_name, proto_file_name) = if target_api == "latest" {
            let message = subscriber.message(symbol_table);
            (message.full_name(symbol_table), message.proto_metadata.filename.clone())
        } else {
            let publication = subscriber.publication(symbol_table);
            (publication.full_name(symbol_table), publication.proto_metadata.filename.clone())
        };
        add_fd_protos_from_publication(
            message_name,
            proto_file_name,
            proto_filename_to_descriptor,
            fd_protos_cache,
            message_cache,
        );
    }
    for server in &service_bundle.servers(symbol_table) {
        add_fd_protos_from_rpc_service(
            server.service(symbol_table),
            symbol_table,
            proto_filename_to_descriptor,
            fd_protos_cache,
            message_cache,
        );
    }
    for client in &service_bundle.clients(symbol_table) {
        add_fd_protos_from_rpc_service(
            client.service(symbol_table),
            symbol_table,
            proto_filename_to_descriptor,
            fd_protos_cache,
            message_cache,
        );
    }

    if let Some(diagnostics_info) = &service_bundle.diagnostics_info {
        for data_item in &diagnostics_info.data_items {
            add_proto_message_schema(
                &data_item.message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
        }

        for routine in &diagnostics_info.routines {
            add_proto_message_schema(
                &routine.start_method_info.request_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
            add_proto_message_schema(
                &routine.start_method_info.response_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
            add_proto_message_schema(
                &routine.stop_method_info.request_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
            add_proto_message_schema(
                &routine.stop_method_info.response_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
            add_proto_message_schema(
                &routine.result_method_info.request_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
            add_proto_message_schema(
                &routine.result_method_info.response_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            );
        }

        for event in &diagnostics_info.events {
            add_proto_message_schema(
                &event.extended_data_message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            )
        }

        for io_control in &diagnostics_info.io_controls {
            add_proto_message_schema(
                &io_control.message.proto_message,
                proto_filename_to_descriptor,
                fd_protos_cache,
                message_cache,
            )
        }
    }
}

fn add_proto_message_schema(
    proto_message: &ProtoMessage,
    proto_filename_to_descriptor: &HashMap<String, FileDescriptorProto>,
    fd_protos_cache: &mut HashMap<String, FileDescriptorProto>,
    message_cache: &mut HashMap<String, String>,
) {
    let proto_file_name = &proto_message.proto_metadata.filename;
    let message_name = format!("{}.{}", &proto_message.package, &proto_message.name);

    if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(proto_file_name).is_none() {
        message_cache.insert(message_name, proto_file_name.clone());
    }

    fill_file_descriptor_and_dependencies(
        proto_file_name.clone(),
        proto_filename_to_descriptor,
        fd_protos_cache,
    );
}

// fill the `fd_protos_cache` with the file descriptor proto for the given file name, along with all of its dependencies
fn fill_file_descriptor_and_dependencies(
    file_name: String,
    proto_filename_to_descriptor: &HashMap<String, FileDescriptorProto>,
    fd_protos_cache: &mut HashMap<String, FileDescriptorProto>,
) {
    if !fd_protos_cache.contains_key(&file_name) {
        let Some(file_descriptor_proto) = proto_filename_to_descriptor.get(&file_name) else {
            eprintln!("Could not find file descriptor proto for file {file_name:?}");
            return;
        };
        for dependency in &file_descriptor_proto.dependency {
            // Don't include VSIDL and well-known protos in the runtime config, as they can be referenced directly by VSIDL provider
            if dependency != VSIDL_SYNTAX_PROTO
                && dependency != VSIDL_ANNOTATIONS_PROTO
                && dependency != DIAG_SYNTAX_PROTO
                && dependency != DIAG_ANNOTATIONS_PROTO
                && WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(dependency).is_none()
            {
                fill_file_descriptor_and_dependencies(
                    dependency.clone(),
                    proto_filename_to_descriptor,
                    fd_protos_cache,
                );
            }
        }
        if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(&file_name).is_none() {
            fd_protos_cache.insert(file_name.clone(), file_descriptor_proto.clone());
        }
    }
}

fn add_fd_protos_from_publication(
    message_name: String,
    proto_file_name: String,
    proto_filename_to_descriptor: &HashMap<String, FileDescriptorProto>,
    fd_protos_cache: &mut HashMap<String, FileDescriptorProto>,
    message_cache: &mut HashMap<String, String>,
) {
    if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(&proto_file_name).is_none() {
        message_cache.insert(message_name, proto_file_name.clone());
    }

    fill_file_descriptor_and_dependencies(
        proto_file_name,
        proto_filename_to_descriptor,
        fd_protos_cache,
    );
}

fn add_fd_protos_from_rpc_service(
    service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
    proto_filename_to_descriptor: &HashMap<String, FileDescriptorProto>,
    fd_protos_cache: &mut HashMap<String, FileDescriptorProto>,
    message_cache: &mut HashMap<String, String>,
) {
    let message_name = service.full_name(symbol_table);
    let proto_file_name = service.proto_metadata.filename.clone();
    if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(&proto_file_name).is_none() {
        message_cache.insert(message_name, proto_file_name.clone());
    }

    fill_file_descriptor_and_dependencies(
        proto_file_name,
        proto_filename_to_descriptor,
        fd_protos_cache,
    );

    for method in &service.method {
        let input_type = method.input_type.clone().unwrap();
        let input_message_name = input_type.trim_start_matches('.');
        let input_message_symbols = service
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, input_message_name);
        let input_message_symbol = input_message_symbols.first().unwrap();

        let input_proto_file_name = input_message_symbol.proto_metadata.filename.clone();

        if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(&input_proto_file_name).is_none() {
            message_cache.insert(input_message_name.to_string(), input_proto_file_name.clone());
        }

        fill_file_descriptor_and_dependencies(
            input_proto_file_name,
            proto_filename_to_descriptor,
            fd_protos_cache,
        );

        let output_type = method.output_type.clone().unwrap();
        let output_message_name = output_type.trim_start_matches('.');
        let output_message_symbols = service
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, output_message_name);
        let output_message_symbol = output_message_symbols.first().unwrap();
        let output_proto_file_name = output_message_symbol.proto_metadata.filename.clone();

        if WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(&output_proto_file_name).is_none() {
            message_cache.insert(output_message_name.to_string(), output_proto_file_name.clone());
        }
        fill_file_descriptor_and_dependencies(
            output_proto_file_name,
            proto_filename_to_descriptor,
            fd_protos_cache,
        );
    }
}

fn add_unit_types_from_service_bundle(
    service_bundle: ServiceBundleSymbol,
    symbol_table: &SymbolTable,
    unit_type_cache: &mut HashMap<UnitType, Archetype_metadata>,
    target_api: &str,
) {
    for publisher in &service_bundle.publishers(symbol_table) {
        add_unit_type_from_publication_definition(
            publisher,
            symbol_table,
            unit_type_cache,
            target_api,
        );
    }
    for subscriber in &service_bundle.subscribers(symbol_table) {
        let any_publisher = if target_api == "latest" {
            subscriber
                .topic_symbols(symbol_table)
                .iter()
                .find_map(|topic_symbol| topic_symbol.publishers(symbol_table).first().cloned())
                .expect("subscriber has a publisher in valid catalogs")
        } else {
            subscriber
                .publication(symbol_table)
                .publishers(symbol_table)
                .first()
                .cloned()
                .expect("subscriber has a publisher in valid catalogs")
        };

        add_unit_type_from_publication_definition(
            any_publisher,
            symbol_table,
            unit_type_cache,
            target_api,
        );
    }
    for server in &service_bundle.servers(symbol_table) {
        add_unit_type_from_rpc_definition(
            server.service(symbol_table),
            symbol_table,
            unit_type_cache,
        );
    }
    for client in &service_bundle.clients(symbol_table) {
        add_unit_type_from_rpc_definition(
            client.service(symbol_table),
            symbol_table,
            unit_type_cache,
        );
    }
}

fn add_unit_type_from_rpc_definition(
    rpc_service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
    unit_type_cache: &mut HashMap<UnitType, Archetype_metadata>,
) {
    let unit_type = UnitType::from((rpc_service, symbol_table));
    if let std::collections::hash_map::Entry::Vacant(entry) = unit_type_cache.entry(unit_type) {
        insert_rpc_metadata(rpc_service, entry);
    }
}

fn add_unit_type_from_publication_definition(
    publisher: &PublisherSymbol,
    symbol_table: &SymbolTable,
    unit_type_cache: &mut HashMap<UnitType, Archetype_metadata>,
    target_api: &str,
) {
    if target_api == "latest" {
        let message = publisher.message(symbol_table);
        let unit_type = UnitType::from((message, symbol_table));
        let message_name = format!("{}.{}", &message.package(symbol_table), &unit_type.typename);
        if let std::collections::hash_map::Entry::Vacant(entry) = unit_type_cache.entry(unit_type) {
            insert_topic_metadata_from_message(
                message_name,
                publisher,
                symbol_table,
                message,
                entry,
            );
        }
    } else {
        let publication = publisher.publication(symbol_table);
        let unit_type = UnitType::from((publication, symbol_table));
        let message_name =
            format!("{}.{}", &publication.package(symbol_table), &unit_type.typename);
        if let std::collections::hash_map::Entry::Vacant(entry) = unit_type_cache.entry(unit_type) {
            insert_topic_metadata(message_name, publication, entry);
        }
    }
}

fn insert_topic_metadata(
    message_name: String,
    publication: &ProtoPublicationSymbol,
    entry: VacantEntry<UnitType, Archetype_metadata>,
) {
    let mut topic_metadata = TopicMetadata::new();
    topic_metadata.message_name = message_name;
    topic_metadata.message_size = publication.message_size.size as u32;
    topic_metadata.message_count = publication.message_count;
    topic_metadata.field_size_constraint = publication
        .message_size
        .size_constraints
        .iter()
        .map(|(k, v)| {
            let mut size_constraint = FieldSizeConstraint::new();
            let repeated_max_count = v.repeated_max_count.unwrap_or(0);
            let variable_type_max_size = v.variable_type_max_size.unwrap_or(0);
            size_constraint.repeated_max_count = repeated_max_count as u32;
            size_constraint.variable_type_max_size = variable_type_max_size as u32;
            (k.clone(), size_constraint)
        })
        .collect::<HashMap<String, FieldSizeConstraint>>();
    let unit_type_archetype = Archetype_metadata::TopicMetadata(topic_metadata);
    entry.insert(unit_type_archetype);
}

fn insert_topic_metadata_from_message(
    message_name: String,
    publisher: &PublisherSymbol,
    symbol_table: &SymbolTable,
    message: &ProtoMessageSymbol,
    entry: VacantEntry<UnitType, Archetype_metadata>,
) {
    let message_size = message.max_size.clone().unwrap();
    let max_message_count = publisher
        .message(symbol_table)
        .publishers(symbol_table)
        .into_iter()
        .map(|publisher| publisher.capacity)
        .max()
        .expect("Expect publisher to be defined");
    let mut topic_metadata = TopicMetadata::new();
    topic_metadata.message_name = message_name;
    topic_metadata.message_size = message_size.size as u32;
    topic_metadata.message_count = max_message_count;
    topic_metadata.field_size_constraint = message_size
        .size_constraints
        .iter()
        .map(|(k, v)| {
            let mut size_constraint = FieldSizeConstraint::new();
            let repeated_max_count = v.repeated_max_count.unwrap_or(0);
            let variable_type_max_size = v.variable_type_max_size.unwrap_or(0);
            size_constraint.repeated_max_count = repeated_max_count as u32;
            size_constraint.variable_type_max_size = variable_type_max_size as u32;
            (k.clone(), size_constraint)
        })
        .collect::<HashMap<String, FieldSizeConstraint>>();
    let unit_type_archetype = Archetype_metadata::TopicMetadata(topic_metadata);
    entry.insert(unit_type_archetype);
}

fn insert_rpc_metadata(
    service: &ProtoRpcServiceSymbol,
    entry: VacantEntry<UnitType, Archetype_metadata>,
) {
    let mut rpc_metadata = RpcMetadata::new();
    let mut method_mapping = Vec::new();
    for rpc_method in &service.method {
        let mut rpc_method_mapping = RpcMethodMapping::new();
        rpc_method_mapping.method_name.clone_from(&rpc_method.name.clone().unwrap());
        rpc_method_mapping.argument_message_name =
            rpc_method.input_type.clone().unwrap().trim_start_matches('.').to_string();
        rpc_method_mapping.return_message_name =
            rpc_method.output_type.clone().unwrap().trim_start_matches('.').to_string();
        method_mapping.push(rpc_method_mapping);
    }
    rpc_metadata.method_mapping = method_mapping;
    let unit_type_archetype = Archetype_metadata::RpcMetadata(rpc_metadata);
    entry.insert(unit_type_archetype);
}

/// Writes the content to the file with path, if content is different
fn write_bytes_to_file(content: &[u8], path: &Path) -> anyhow::Result<()> {
    // Skips if the same content already exists in the path.
    if let Ok(existing_content) = fs::read(path) {
        if existing_content == content {
            return Ok(());
        }
    }
    Ok(fs::write(path, content)?)
}

pub(crate) fn get_current_year() -> i32 {
    chrono::Utc::now().year()
}

/// Generate build file that declares `prebuilt` modules that contain the generated runtime config files. Generated target is written into respective folder inside `output_path`
#[allow(clippy::too_many_arguments)]
pub fn generate_runtime_config_prebuilts(
    catalog_path: &Path,
    catalog: &Catalog,
    symbol_table: &SymbolTable,
    bundles: &[ServiceBundleSymbol],
    output_path: &Path,
    filegroup: &str,
    dependencies: &[&std::path::Path],
    documentation_lines: &Vec<String>,
    target_api: &str,
) -> anyhow::Result<()> {
    let mut tt = TinyTemplate::new();
    tt.add_template("android_bp", include_str!("templates/android_bp.tpl"))?;

    let has_someip = catalog
        .catalog_vsidl_files
        .iter()
        .any(|(_, vsidl_entry)| !vsidl_entry.some_ip_mapping.is_empty());

    // Use BTreeSet to get unique package names in a sorted order, to ensure that the output
    // when using android_bp::render is consistent and does not change between independent runs.
    let packages = bundles
        .iter()
        .map(|service_bundle| service_bundle.package(symbol_table))
        .collect::<BTreeSet<_>>();

    fs::create_dir_all(
        output_path
            .parent()
            .unwrap_or_else(|| panic!("Parent directory for {} path", output_path.display())),
    )?;
    write_bytes_to_file(
        android_bp::render(
            catalog_path,
            symbol_table,
            bundles,
            &packages,
            filegroup,
            dependencies,
            has_someip,
            documentation_lines,
            target_api,
            &tt,
        )
        .as_bytes(),
        &PathBuf::from(&output_path),
    )?;
    Ok(())
}

fn generate_some_ip_mapping_to_file(catalog: &Catalog, path: &Path) -> anyhow::Result<()> {
    let some_ip_mapping =
        generate_some_ip_mapping(catalog).with_context(|| "Failed to generate some ip mapping.")?;
    write_proto_to_file(some_ip_mapping, path)
}

fn generate_some_ip_mapping(catalog: &Catalog) -> anyhow::Result<SomeipRuntimeConfig> {
    let mut someip_config = SomeipRuntimeConfig::new();
    for (_, vsidl_entry) in &catalog.catalog_vsidl_files {
        for someip_mapping in &vsidl_entry.some_ip_mapping {
            someip_config.some_ip_mapping.push(someip_mapping.clone())
        }
    }
    Ok(someip_config)
}

fn generate_diagnostics_declaration_config_to_file(
    service_bundle_symbol: ServiceBundleSymbol,
    path: &Path,
) -> anyhow::Result<()> {
    let diagnostics_declaration = generate_diagnostics_declaration_config(service_bundle_symbol);

    write_proto_to_file::<DiagnosticsDeclaration>(diagnostics_declaration.clone(), path)
}

fn generate_diagnostics_declaration_config(
    service_bundle_symbol: ServiceBundleSymbol,
) -> DiagnosticsDeclaration {
    service_bundle_symbol
        .diagnostics_info
        .as_ref()
        .map(DiagnosticsInfo::as_raw)
        .unwrap_or_default()
        .clone()
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use googletest::prelude::*;

    use vsidl_v1::runtime_config::vsidl_runtime_config::unit_type_descriptor_mapping::Archetype_metadata;
    use vsidl_v1::runtime_config::vsidl_runtime_config::{
        MessageDescriptorMapping, RpcMetadata, RpcMethodMapping, TopicMetadata,
        UnitTypeDescriptorMapping,
    };
    use vsidl_v1_parser::{
        service_bundle_symbol::ServiceBundleSource, service_bundle_symbol::ServiceBundleSymbol,
        Catalog, SymbolTable,
    };
    use vsidlc_cli::VsidlcCommonCli;

    use crate::generator::{
        generate_diagnostics_declaration_config, generate_runtime_config, generate_some_ip_mapping,
        write_bytes_to_file, write_proto_to_file,
    };
    use protobuf::MessageField;
    use vsidl_v1::diagnostics_syntax::diagnostics_declaration::{
        routine::Method, DataItem, Event, InputOutputControlData, Routine,
    };
    use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;

    use anyhow::Context;
    use protobuf::Message;
    use std::fs;
    use tempfile::tempdir;
    use vsidl_v1::runtime_config::SomeipRuntimeConfig;
    use vsidl_v1::runtime_config::VsidlRuntimeConfig;

    use protobuf::descriptor::{
        field_descriptor_proto::{Label, Type},
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    };

    const LEGACY_API: &str = "pdk29";

    static TIRE_PACKAGE: &str = "sdv.transmission";
    static TIRE_BUNDLE: &str = "Manager";
    static TIRE_TYPE: &str = "TirePressure";
    static TIRE_RANGE_BUNDLE: &str = "Monitor";
    static TIRE_RANGE_TYPE: &str = "TirePressureRange";
    static TIRE_PROTO_NAME: &str = "tire.proto";

    static TIRE2_PACKAGE: &str = "sdv.transmission2";
    static TIRE2_BUNDLE: &str = "OtherManager";
    static TIRE2_TYPE: &str = "OtherTirePressure";
    static TIRE2_RANGE_BUNDLE: &str = "OtherMonitor";
    static TIRE2_RANGE_TYPE: &str = "OtherTirePressureRange";
    static TIRE2_PROTO_NAME: &str = "other_tire.proto";

    static NESTED_PROTO_NAME: &str = "nested.proto";

    static HELLO_SERVICE_PACKAGE: &str = "someip.rpc";
    static HELLO_SERVICE_BUNDLE: &str = "HelloService";
    static HELLO_INTERFACE_TYPE: &str = "HelloInterface";
    static HELLO_SERVICE_PROTO: &str = "hello_rpc.proto";
    static HELLO_MESSAGES_PROTO: &str = "hello_messages.proto";

    static DIAGNOSTIC_MESSAGES_PROTO: &str = "diagnostics_messages.proto";

    /// Reads bytes from a file and parses them into a proto message
    fn read_proto_from_file<T: Message>(path: &Path) -> anyhow::Result<T> {
        let content = fs::read(path).with_context(|| format!("Failed to read file: {path:#?}"))?;
        T::parse_from_bytes(&content)
            .with_context(|| format!("Failed to parse proto from file: {path:#?}"))
    }

    /// Helper to run the full `generate_runtime_config` and read the
    /// resulting VsidlRuntimeConfig proto from the generated file.
    fn generate_and_read_vsidl_config(
        catalog: &Catalog,
        symbol_table: &SymbolTable,
        bundles: &Vec<ServiceBundleSymbol>,
    ) -> anyhow::Result<VsidlRuntimeConfig> {
        // 1. Create temp dir
        let dir = tempdir().expect("Failed to create temp dir for test");

        // 2. Call generate_runtime_config
        generate_runtime_config(
            catalog,
            symbol_table,
            bundles,
            dir.path(),
            None, // Generate for all bundles
            None, // No package filter
            LEGACY_API,
        )
        .expect("generate_runtime_config failed");

        // 3. Define output file path
        let config_path = dir.path().join("vsidl-config.binpb");

        // 4. Read and return
        read_proto_from_file(&config_path)
    }

    #[test]
    fn test_generate_vsidl_runtime_config_succeeds() {
        // GIVEN
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);

        // WHEN/THEN
        assert!(generate_and_read_vsidl_config(&catalog, &symbol_table, &bundles).is_ok());
    }

    #[test]
    fn test_generate_vsidl_runtime_config_generates_expected_unit_type_mappings() {
        // GIVEN
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.unit_type_mapping.len(), 9);
        assert_that!(
            vsidl_config.unit_type_mapping,
            contains_each![
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                        message_size: 14, // 5 bytes for each uint32 (2) + 1 for each data tag less than 16 (3) + 1 byte for length segment for nested message of size less than 2^7
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(),
                    type_name: TIRE_RANGE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                        message_size: 6, // 5 bytes for an enum + 1 for data tag less than 16
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE2_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(),
                    type_name: TIRE2_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                        message_size: 6, // 5 bytes for a uint32 + 1 for data tag less than 16
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE2_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(),
                    type_name: TIRE2_RANGE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE2_PACKAGE}.{TIRE2_RANGE_TYPE}"),
                        message_size: 6, // 5 bytes for an enum + 1 for data tag less than 16
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                eq(&UnitTypeDescriptorMapping {
                    package_name: HELLO_SERVICE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(),
                    type_name: HELLO_INTERFACE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::RpcMetadata(RpcMetadata {
                        method_mapping: vec![
                            RpcMethodMapping {
                                method_name: "SayHello".to_string(),
                                argument_message_name: "someip.rpc.HelloMessageRequest".to_string(),
                                return_message_name: "someip.rpc.HelloMessageResponse".to_string(),
                                ..Default::default()
                            },
                            RpcMethodMapping {
                                method_name: "Foo".to_string(),
                                argument_message_name: "google.protobuf.Empty".to_string(),
                                return_message_name: "google.protobuf.BoolValue".to_string(),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
            ]
        );
    }

    #[test]
    fn test_generate_vsidl_runtime_config_generates_expected_message_descriptor_mappings() {
        // GIVEN
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.descriptor_mapping.len(), 26);
        assert_that!(
            vsidl_config.descriptor_mapping,
            contains_each![
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                    file_name: TIRE2_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE2_PACKAGE}.{TIRE2_RANGE_TYPE}"),
                    file_name: TIRE2_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloInterface".to_string(),
                    file_name: HELLO_SERVICE_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloMessageRequest".to_string(),
                    file_name: HELLO_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloMessageResponse".to_string(),
                    file_name: HELLO_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "diagnostics.DataItem".to_string(),
                    file_name: DIAGNOSTIC_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "diagnostics.RoutineRequest".to_string(),
                    file_name: DIAGNOSTIC_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "diagnostics.IoData".to_string(),
                    file_name: DIAGNOSTIC_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "diagnostics.EventData".to_string(),
                    file_name: DIAGNOSTIC_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
            ]
        );
    }

    #[test]
    fn test_generate_vsidl_runtime_config_generates_expected_file_descriptor_protos() {
        // GIVEN
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 11);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(
            descriptor_names,
            contains_each![
                eq(&TIRE_PROTO_NAME.to_string()),
                eq(&TIRE2_PROTO_NAME.to_string()),
                eq(&NESTED_PROTO_NAME.to_string()),
                eq(&HELLO_SERVICE_PROTO.to_string()),
                eq(&HELLO_MESSAGES_PROTO.to_string()),
                eq(&DIAGNOSTIC_MESSAGES_PROTO.to_string())
            ]
        );
    }

    #[test]
    fn test_generate_vsidl_runtime_config_generates_expected_someip_mappings() {
        // GIVEN
        let catalog = build_catalog();

        // WHEN
        let someip_mapping =
            generate_some_ip_mapping(&catalog).expect("Failed to generate someip mappings");

        // THEN
        assert_eq!(someip_mapping.some_ip_mapping.len(), 1);
    }

    #[test]
    fn test_generate_vsidl_runtime_config_generates_expected_diagnostics_declarations() {
        // GIVEN
        let catalog = build_catalog();
        let (bundles, _) = form_bundles(&catalog);

        for bundle in bundles.iter() {
            if bundle.name != "DiagnosticsService" {
                continue;
            }

            // WHEN
            let diagnostics_declaration = generate_diagnostics_declaration_config(bundle.clone());

            // THEN
            assert_eq!(
                diagnostics_declaration,
                DiagnosticsDeclaration {
                    data_item: vec![DataItem {
                        id: "data_item".to_string(),
                        message_name: "diagnostics.DataItem".to_string(),
                        is_writable: false,
                        ..Default::default()
                    }],
                    routine: vec![Routine {
                        id: "routine".to_string(),
                        start: MessageField::some(Method {
                            request: "diagnostics.RoutineRequest".to_string(),
                            response: "google.protobuf.Empty".to_string(),
                            ..Default::default()
                        }),
                        stop: MessageField::some(Method {
                            request: "google.protobuf.Empty".to_string(),
                            response: "google.protobuf.Empty".to_string(),
                            ..Default::default()
                        }),
                        result: MessageField::some(Method {
                            request: "google.protobuf.Empty".to_string(),
                            response: "google.protobuf.Empty".to_string(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    io_control: vec![InputOutputControlData {
                        id: "io_control".to_string(),
                        message_name: "diagnostics.IoData".to_string(),
                        ..Default::default()
                    }],
                    event: vec![Event {
                        id: "event".to_string(),
                        extended_data_message_name: "diagnostics.EventData".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }
            );
        }
    }

    fn build_test_vsidlc() -> VsidlcCommonCli {
        VsidlcCommonCli {
            catalog_path: Path::new("src/tests/catalog/").to_path_buf(),
            dependency_catalog_path: Vec::from([
                Path::new("diagnostics_v1_catalog_files/").to_path_buf()
            ]),
            ..Default::default()
        }
    }

    fn build_catalog() -> Catalog {
        let test_cli = build_test_vsidlc();
        test_cli
            .create_catalog_using_pest(Some(Path::new("./").to_path_buf()))
            .expect("Unable to create catalog")
    }

    fn form_bundles(catalog: &Catalog) -> (Vec<ServiceBundleSymbol>, SymbolTable) {
        let test_cli = build_test_vsidlc();
        let symbol_table = test_cli
            .create_symbol_table_from_catalog(catalog)
            .map_err(|e| {
                eprintln!("Printing error before panic: {:#}", e);
                e // return the error back to the chain so expect() sees it
            })
            .expect("Input files could not be compiled");
        let mut bundles: Vec<ServiceBundleSymbol> = symbol_table
            .get_all_service_bundle_symbols()
            .iter()
            .filter(|bundle| bundle.source == ServiceBundleSource::CATALOG)
            .cloned()
            .cloned()
            .collect();
        bundles.sort_by_key(|sb| sb.file_path.clone());
        (bundles, symbol_table)
    }

    fn build_test_vsidlc_soa() -> VsidlcCommonCli {
        VsidlcCommonCli {
            catalog_path: Path::new("src/tests/soa/catalog/").to_path_buf(),
            dependency_catalog_path: [].to_vec(),
            target_api: "latest".to_string(),
            ..Default::default()
        }
    }

    fn build_catalog_soa() -> Catalog {
        let test_cli = build_test_vsidlc_soa();
        test_cli
            .create_catalog_using_pest(Some(Path::new("./").to_path_buf()))
            .expect("Unable to create catalog")
    }

    fn form_bundles_soa(catalog: &Catalog) -> (Vec<ServiceBundleSymbol>, SymbolTable) {
        let test_cli = build_test_vsidlc_soa();
        let symbol_table = test_cli
            .create_symbol_table_from_catalog(catalog)
            .map_err(|e| {
                eprintln!("Printing error before panic: {:#}", e);
                e // return the error back to the chain so expect() sees it
            })
            .expect("Input files could not be compiled");
        let mut bundles: Vec<ServiceBundleSymbol> = symbol_table
            .get_all_service_bundle_symbols()
            .iter()
            .filter(|bundle| bundle.source == ServiceBundleSource::CATALOG)
            .cloned()
            .cloned()
            .collect();
        bundles.sort_by_key(|sb| sb.file_path.clone());
        (bundles, symbol_table)
    }

    fn generate_and_read_vsidl_config_soa(
        catalog: &Catalog,
        symbol_table: &SymbolTable,
        bundles: &Vec<ServiceBundleSymbol>,
    ) -> anyhow::Result<VsidlRuntimeConfig> {
        let dir = tempdir().expect("Failed to create temp dir for test");
        generate_runtime_config(catalog, symbol_table, bundles, dir.path(), None, None, "latest")
            .expect("generate_runtime_config failed");
        let config_path = dir.path().join("vsidl-config.binpb");
        read_proto_from_file(&config_path)
    }

    #[test]
    fn test_generate_vsidl_runtime_config_soa_succeeds() {
        // GIVEN
        let catalog = build_catalog_soa();
        let (bundles, symbol_table) = form_bundles_soa(&catalog);

        // WHEN/THEN
        assert!(generate_and_read_vsidl_config_soa(&catalog, &symbol_table, &bundles).is_ok());
    }

    #[test]
    fn test_generate_vsidl_runtime_config_soa_generates_expected_unit_type_mappings() {
        // GIVEN
        let catalog = build_catalog_soa();
        let (bundles, symbol_table) = form_bundles_soa(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config_soa(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.unit_type_mapping.len(), 2);
        println!("{:?}", vsidl_config.unit_type_mapping);
        assert_that!(
            vsidl_config.unit_type_mapping,
            contains_each![
                eq(&UnitTypeDescriptorMapping {
                    package_name: "sdv.tests.soa.transmission".to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: "TirePressure".to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: "sdv.tests.soa.transmission.TirePressure".to_string(),
                        message_size: 6,
                        message_count: 124,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                eq(&UnitTypeDescriptorMapping {
                    package_name: "sdv.tests.soa.transmission".to_string(),
                    service_bundle_name: "".to_string(),
                    type_name: "TireTemperature".to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: "sdv.tests.soa.transmission.TireTemperature".to_string(),
                        message_size: 6,
                        message_count: 4,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
            ]
        );
    }

    #[test]
    fn test_generate_vsidl_runtime_config_soa_generates_expected_message_descriptor_mappings() {
        // GIVEN
        let catalog = build_catalog_soa();
        let (bundles, symbol_table) = form_bundles_soa(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config_soa(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.descriptor_mapping.len(), 2);
        assert_that!(
            vsidl_config.descriptor_mapping,
            contains_each![
                eq(&MessageDescriptorMapping {
                    message_name: "sdv.tests.soa.transmission.TirePressure".to_string(),
                    file_name: "tire.proto".to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "sdv.tests.soa.transmission.TireTemperature".to_string(),
                    file_name: "tire.proto".to_string(),
                    ..Default::default()
                }),
            ]
        );
    }

    #[test]
    fn test_generate_vsidl_runtime_config_soa_generates_expected_file_descriptor_protos() {
        // GIVEN
        let catalog = build_catalog_soa();
        let (bundles, symbol_table) = form_bundles_soa(&catalog);

        // WHEN
        let vsidl_config = generate_and_read_vsidl_config_soa(&catalog, &symbol_table, &bundles)
            .expect("Failed to generate VSIDL runtime config");

        // THEN
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 1);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(descriptor_names, contains_each![eq(&TIRE_PROTO_NAME.to_string()),]);
    }

    #[test]
    fn test_write_bytes_to_file_writes_new_file() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("test.bin");
        let content = b"hello world";

        // WHEN
        let result = write_bytes_to_file(content, &file_path);

        // THEN
        assert!(result.is_ok());
        let read_content = fs::read(&file_path).expect("Failed to read file");
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_write_bytes_to_file_overwrites_different_content() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("test.bin");
        fs::write(&file_path, b"initial content").expect("Failed to write initial file");
        let new_content = b"new content";

        // WHEN
        let result = write_bytes_to_file(new_content, &file_path);

        // THEN
        assert!(result.is_ok());
        let read_content = fs::read(&file_path).expect("Failed to read file");
        assert_eq!(read_content, new_content);
    }

    #[test]
    fn test_write_bytes_to_file_fails_for_directory() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let content = b"hello";

        // WHEN
        let result = write_bytes_to_file(content, dir.path());

        // THEN
        assert!(result.is_err());
    }

    #[test]
    fn test_write_proto_to_file_writes_new_file() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("test.proto.bin");
        let mut proto = SomeipRuntimeConfig::new();
        proto.some_ip_mapping.push(Default::default()); // Add some data

        // WHEN
        let result = write_proto_to_file(proto.clone(), &file_path);

        // THEN
        assert!(result.is_ok());

        // Verify by reading the proto back
        let read_proto: SomeipRuntimeConfig =
            read_proto_from_file(&file_path).expect("Failed to read proto back");
        assert_eq!(read_proto, proto);
    }

    #[test]
    fn test_write_proto_to_file_overwrites_different_content() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("test.proto.bin");

        // Write initial proto
        let proto1 = SomeipRuntimeConfig::new();
        write_proto_to_file(proto1, &file_path).expect("Failed to write initial proto");

        // Define new, different proto
        let mut proto2 = SomeipRuntimeConfig::new();
        proto2.some_ip_mapping.push(Default::default()); // Add data to make it different

        // WHEN
        let result = write_proto_to_file(proto2.clone(), &file_path);

        // THEN
        assert!(result.is_ok());

        // Verify the new proto content was written
        let read_proto: SomeipRuntimeConfig =
            read_proto_from_file(&file_path).expect("Failed to read proto back");
        assert_eq!(read_proto, proto2);
    }

    #[test]
    fn test_write_proto_to_file_skips_identical_proto() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("test.proto.bin");
        let proto = SomeipRuntimeConfig::new();

        // Write the file the first time
        let result1 = write_proto_to_file(proto.clone(), &file_path);
        assert!(result1.is_ok());
        let metadata_before = fs::metadata(&file_path).unwrap();

        // Sleep to ensure modification time would change
        std::thread::sleep(std::time::Duration::from_secs(1));

        // WHEN
        // Write the *same* proto again
        let result2 = write_proto_to_file(proto.clone(), &file_path);

        // THEN
        assert!(result2.is_ok());
        let metadata_after = fs::metadata(&file_path).unwrap();

        // 1. Check that the write was skipped
        assert_eq!(metadata_before.modified().unwrap(), metadata_after.modified().unwrap());

        // 2. Verify the content is still correct
        let read_proto: SomeipRuntimeConfig =
            read_proto_from_file(&file_path).expect("Failed to read proto back");
        assert_eq!(read_proto, proto);
    }

    #[test]
    fn test_read_proto_from_file_fails_for_bad_data() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("bad.bin");
        fs::write(&file_path, b"this is not a proto").unwrap();

        // WHEN
        let result = read_proto_from_file::<SomeipRuntimeConfig>(&file_path);

        // THEN
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse proto"));
    }

    #[test]
    fn test_read_proto_from_file_fails_for_missing_file() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("missing.bin");

        // WHEN
        let result = read_proto_from_file::<SomeipRuntimeConfig>(&file_path);

        // THEN
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read file"));
    }

    #[test]
    fn test_generate_runtime_config_all_files() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            None,
            None,
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());
        assert!(dir.path().join("vsidl-config.binpb").exists());
        assert!(dir.path().join("someip-config.binpb").exists());
        // Diagnostics config is only generated for specific bundle case
        assert!(!dir.path().join("diagnostics-config.binpb").exists());
    }

    #[test]
    fn test_generate_runtime_config_bundle_only_fails_for_nonexistent_bundle() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "ThisBundleDoesNotExist".to_string(); // Non-existent bundle

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name.clone()),
            None,
            LEGACY_API,
        );

        // THEN
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&format!("Unable to find service bundle with name {bundle_name}")));
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_diagnostics_service() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "DiagnosticsService".to_string(); // From existing tests

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            None,
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        assert!(vsidl_config_path.exists());

        // Read the generated config back
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        assert_eq!(
            vsidl_config.unit_type_mapping.len(),
            4,
            "Expected 4 unit types for the 4 diagnostic servers. More Unit type mappings were found, meaning other bundles were processed."
        );

        assert!(
            vsidl_config.descriptor_mapping.len() >= 4,
            "Expected at least 4 message descriptors."
        );

        let message_names: Vec<String> =
            vsidl_config.descriptor_mapping.iter().map(|m| m.message_name.clone()).collect();

        assert_that!(
            message_names,
            contains_each![
                eq("diagnostics.DataItem"),
                eq("diagnostics.RoutineRequest"),
                eq("diagnostics.IoData"),
                eq("diagnostics.EventData")
            ]
        );

        // Check that the diagnostic file descriptor (and its dependencies) were processed
        // and that its content matches the expected structure.
        assert!(
            !vsidl_config.file_descriptor_proto.is_empty(),
            "Expected at least one file descriptor proto"
        );

        let expected_fd_proto = FileDescriptorProto {
            name: Some(DIAGNOSTIC_MESSAGES_PROTO.to_string()),
            package: Some("diagnostics".to_string()),
            syntax: Some("proto3".to_string()),
            message_type: vec![
                DescriptorProto {
                    name: Some("DataItem".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("RoutineRequest".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("request".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_INT64.into()),
                        json_name: Some("request".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("IoData".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("EventData".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Find the specific diagnostic file descriptor and verify it matches expectation
        let diag_fd_proto = vsidl_config
            .file_descriptor_proto
            .iter()
            .find(|fd| fd.name.as_deref() == Some(DIAGNOSTIC_MESSAGES_PROTO))
            .expect("DIAGNOSTIC_MESSAGES_PROTO not found in generated config");

        assert_eq!(diag_fd_proto, &expected_fd_proto);
    }

    #[test]
    fn test_generate_runtime_config_for_diagnostics_service() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "DiagnosticsService".to_string();
        let package_name = "diagnostics".to_string();

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        assert!(vsidl_config_path.exists());

        // Read the generated config back
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        assert_eq!(
            vsidl_config.unit_type_mapping.len(),
            4,
            "Expected 4 unit types for the 4 diagnostic servers. More Unit type mappings were found, meaning other bundles were processed."
        );

        // Check that diagnostic message descriptors were processed.
        assert!(
            vsidl_config.descriptor_mapping.len() >= 4,
            "Expected at least 4 message descriptors."
        );

        let message_names: Vec<String> =
            vsidl_config.descriptor_mapping.iter().map(|m| m.message_name.clone()).collect();

        assert_that!(
            message_names,
            contains_each![
                eq("diagnostics.DataItem"),
                eq("diagnostics.RoutineRequest"),
                eq("diagnostics.IoData"),
                eq("diagnostics.EventData")
            ]
        );

        // Check that the diagnostic file descriptor (and its dependencies) were processed
        // and that its content matches the expected structure.
        assert!(
            !vsidl_config.file_descriptor_proto.is_empty(),
            "Expected at least one file descriptor proto"
        );

        let expected_fd_proto = FileDescriptorProto {
            name: Some(DIAGNOSTIC_MESSAGES_PROTO.to_string()),
            package: Some("diagnostics".to_string()),
            syntax: Some("proto3".to_string()),
            message_type: vec![
                DescriptorProto {
                    name: Some("DataItem".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("RoutineRequest".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("request".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_INT64.into()),
                        json_name: Some("request".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("IoData".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("EventData".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL.into()),
                        type_: Some(Type::TYPE_FLOAT.into()),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Find the specific diagnostic file descriptor and verify it matches expectation
        let diag_fd_proto = vsidl_config
            .file_descriptor_proto
            .iter()
            .find(|fd| fd.name.as_deref() == Some(DIAGNOSTIC_MESSAGES_PROTO))
            .expect("DIAGNOSTIC_MESSAGES_PROTO not found in generated config");

        assert_eq!(diag_fd_proto, &expected_fd_proto);
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_manager() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = TIRE_BUNDLE.to_string(); // "Manager"
        let package_name = TIRE_PACKAGE.to_string(); // "sdv.transmission"

        // WHEN
        // We must filter by package AND bundle name to uniquely identify it.
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created (but will be empty)
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());
        let diag_config: DiagnosticsDeclaration = read_proto_from_file(&diag_config_path)
            .expect("Failed to read diagnostics-config.binpb");
        assert_eq!(diag_config, DiagnosticsDeclaration::default());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        assert!(vsidl_config_path.exists());

        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should only contain "TirePressure" topic
        assert_eq!(vsidl_config.unit_type_mapping.len(), 1);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![eq(&UnitTypeDescriptorMapping {
                package_name: TIRE_PACKAGE.to_string(),
                service_bundle_name: "".to_string(), // deprecated
                type_name: TIRE_TYPE.to_string(),
                archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                    message_size: 14,
                    message_count: 8,
                    ..Default::default()
                })),
                ..Default::default()
            }),]
        );

        // 2. Check message descriptors: Should only contain "TirePressure" message
        assert_eq!(vsidl_config.descriptor_mapping.len(), 1);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![eq(&MessageDescriptorMapping {
                message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                file_name: TIRE_PROTO_NAME.to_string(),
                ..Default::default()
            }),]
        );

        // 3. Check file descriptors: Should contain "tire.proto" and its dependency "nested.proto"
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 2);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(
            descriptor_names,
            unordered_elements_are![
                eq(&TIRE_PROTO_NAME.to_string()),
                eq(&NESTED_PROTO_NAME.to_string())
            ]
        );
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_monitor() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = TIRE_RANGE_BUNDLE.to_string(); // "Monitor"
        let package_name = TIRE_PACKAGE.to_string(); // "sdv.transmission"

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created (empty)
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());
        let diag_config: DiagnosticsDeclaration = read_proto_from_file(&diag_config_path)
            .expect("Failed to read diagnostics-config.binpb");
        assert_eq!(diag_config, DiagnosticsDeclaration::default());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should contain *both* unit types from tire.proto
        assert_eq!(vsidl_config.unit_type_mapping.len(), 2);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![
                // The unit type from the filtered bundle
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE_RANGE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                        message_size: 6,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                // The *other* unit type from the same proto file
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE_TYPE.to_string(),    // "TirePressure"
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                        message_size: 14,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                })
            ]
        );

        // 2. Check message descriptors: Should contain *both* messages from tire.proto
        assert_eq!(vsidl_config.descriptor_mapping.len(), 2);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![
                // The message from the filtered bundle's unit type
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                // The *other* message from the same proto file
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                })
            ]
        );

        // 3. Check file descriptors: Should contain "tire.proto" and "nested.proto"
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 2);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(
            descriptor_names,
            unordered_elements_are![
                eq(&TIRE_PROTO_NAME.to_string()),
                eq(&NESTED_PROTO_NAME.to_string())
            ]
        );
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_other_manager() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = TIRE2_BUNDLE.to_string(); // "OtherManager"
        let package_name = TIRE2_PACKAGE.to_string(); // "sdv.transmission2"

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created (empty)
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());
        let diag_config: DiagnosticsDeclaration = read_proto_from_file(&diag_config_path)
            .expect("Failed to read diagnostics-config.binpb");
        assert_eq!(diag_config, DiagnosticsDeclaration::default());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should only contain "OtherTirePressure" topic
        assert_eq!(vsidl_config.unit_type_mapping.len(), 1);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![eq(&UnitTypeDescriptorMapping {
                package_name: TIRE2_PACKAGE.to_string(),
                service_bundle_name: "".to_string(), // deprecated
                type_name: TIRE2_TYPE.to_string(),
                archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                    message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                    message_size: 6,
                    message_count: 8,
                    ..Default::default()
                })),
                ..Default::default()
            }),]
        );

        // 2. Check message descriptors: Should only contain "OtherTirePressure" message
        assert_eq!(vsidl_config.descriptor_mapping.len(), 1);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![eq(&MessageDescriptorMapping {
                message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                file_name: TIRE2_PROTO_NAME.to_string(),
                ..Default::default()
            }),]
        );

        // 3. Check file descriptors: Should contain only "other_tire.proto" (no dependencies)
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 1);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(descriptor_names, unordered_elements_are![eq(&TIRE2_PROTO_NAME.to_string())]);
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_other_monitor() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = TIRE2_RANGE_BUNDLE.to_string(); // "OtherMonitor"
        let package_name = TIRE2_PACKAGE.to_string(); // "sdv.transmission2"

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created (empty)
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());
        let diag_config: DiagnosticsDeclaration = read_proto_from_file(&diag_config_path)
            .expect("Failed to read diagnostics-config.binpb");
        assert_eq!(diag_config, DiagnosticsDeclaration::default());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should contain *both* unit types defined in the proto
        assert_eq!(vsidl_config.unit_type_mapping.len(), 2);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![
                // The unit type from the filtered bundle
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE2_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE2_RANGE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE2_PACKAGE}.{TIRE2_RANGE_TYPE}"),
                        message_size: 6,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                // The *other* unit type from the same proto file, which is also included
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE2_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE2_TYPE.to_string(),   // "OtherTirePressure"
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                        message_size: 6,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                })
            ]
        );

        // 2. Check message descriptors: Should contain *both* messages from the proto
        assert_eq!(vsidl_config.descriptor_mapping.len(), 2);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![
                // The message from the filtered bundle's unit type
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE2_PACKAGE}.{TIRE2_RANGE_TYPE}"),
                    file_name: TIRE2_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                // The *other* message from the same proto file
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE2_PACKAGE}.{TIRE2_TYPE}"),
                    file_name: TIRE2_PROTO_NAME.to_string(),
                    ..Default::default()
                })
            ]
        );

        // 3. Check file descriptors: Should contain only "other_tire.proto"
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 1);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(descriptor_names, unordered_elements_are![eq(&TIRE2_PROTO_NAME.to_string())]);
    }

    #[test]
    fn test_generate_runtime_config_specific_bundle_is_isolated_for_hello_service() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = HELLO_SERVICE_BUNDLE.to_string(); // "HelloService"
        let package_name = HELLO_SERVICE_PACKAGE.to_string(); // "someip.rpc"

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was created (empty)
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(diag_config_path.exists());
        let diag_config: DiagnosticsDeclaration = read_proto_from_file(&diag_config_path)
            .expect("Failed to read diagnostics-config.binpb");
        assert_eq!(diag_config, DiagnosticsDeclaration::default());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should only contain "HelloInterface" RPC
        assert_eq!(vsidl_config.unit_type_mapping.len(), 1);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![eq(&UnitTypeDescriptorMapping {
                package_name: HELLO_SERVICE_PACKAGE.to_string(),
                service_bundle_name: "".to_string(), // deprecated
                type_name: HELLO_INTERFACE_TYPE.to_string(),
                archetype_metadata: Some(Archetype_metadata::RpcMetadata(RpcMetadata {
                    method_mapping: vec![
                        RpcMethodMapping {
                            method_name: "SayHello".to_string(),
                            argument_message_name: "someip.rpc.HelloMessageRequest".to_string(),
                            return_message_name: "someip.rpc.HelloMessageResponse".to_string(),
                            ..Default::default()
                        },
                        RpcMethodMapping {
                            method_name: "Foo".to_string(),
                            argument_message_name: "google.protobuf.Empty".to_string(),
                            return_message_name: "google.protobuf.BoolValue".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })),
                ..Default::default()
            }),]
        );

        // 2. Check message descriptors: Should contain the service, request, and response messages.
        // (Well-known types like Empty and BoolValue are excluded).
        assert_eq!(vsidl_config.descriptor_mapping.len(), 3);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloInterface".to_string(),
                    file_name: HELLO_SERVICE_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloMessageRequest".to_string(),
                    file_name: HELLO_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: "someip.rpc.HelloMessageResponse".to_string(),
                    file_name: HELLO_MESSAGES_PROTO.to_string(),
                    ..Default::default()
                })
            ]
        );

        // 3. Check file descriptors: Should contain "hello_rpc.proto" and "hello_messages.proto"
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 2);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(
            descriptor_names,
            unordered_elements_are![
                eq(&HELLO_SERVICE_PROTO.to_string()),
                eq(&HELLO_MESSAGES_PROTO.to_string())
            ]
        );
    }

    #[test]
    fn test_generate_runtime_config_nonexistent_bundle() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "ThisBundleDoesNotExist".to_string();

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            None,
            LEGACY_API,
        );

        // THEN
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unable to find service bundle"));
    }

    #[test]
    fn test_generate_runtime_config_nonexistent_bundle_but_existent_package() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "ThisBundleDoesNotExist".to_string();
        let package_name = "diagnostics".to_string();

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name.clone()),
            Some(package_name.clone()),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(&format!(
            "There is no bundle named {bundle_name} in package {package_name}"
        )));
    }

    #[test]
    fn test_generate_runtime_config_nonexistent_package() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let package_name = "ThisPacklageDoesNotExist".to_string();

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            None,
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("There are no bundles in package"));
    }

    #[test]
    fn test_generate_runtime_config_specific_package_only_sdv_transmission() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let package_name = TIRE_PACKAGE.to_string(); // "sdv.transmission"

        // WHEN
        // Filter by *only* package name
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            None, // No specific bundle
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was NOT created
        let diag_config_path = dir.path().join("diagnostics-config.binpb");
        assert!(!diag_config_path.exists());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        assert!(vsidl_config_path.exists());

        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should contain *both* bundles from the package
        assert_eq!(vsidl_config.unit_type_mapping.len(), 2);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![
                // From "Manager" bundle
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                        message_size: 14,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                }),
                // From "Monitor" bundle
                eq(&UnitTypeDescriptorMapping {
                    package_name: TIRE_PACKAGE.to_string(),
                    service_bundle_name: "".to_string(), // deprecated
                    type_name: TIRE_RANGE_TYPE.to_string(),
                    archetype_metadata: Some(Archetype_metadata::TopicMetadata(TopicMetadata {
                        message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                        message_size: 6,
                        message_count: 8,
                        ..Default::default()
                    })),
                    ..Default::default()
                })
            ]
        );

        // 2. Check message descriptors: Should contain *both* messages
        assert_eq!(vsidl_config.descriptor_mapping.len(), 2);
        assert_that!(
            vsidl_config.descriptor_mapping,
            unordered_elements_are![
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                }),
                eq(&MessageDescriptorMapping {
                    message_name: format!("{TIRE_PACKAGE}.{TIRE_RANGE_TYPE}"),
                    file_name: TIRE_PROTO_NAME.to_string(),
                    ..Default::default()
                })
            ]
        );

        // 3. Check file descriptors: Should contain "tire.proto" and its dependency "nested.proto"
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 2);
        let descriptor_names = vsidl_config
            .file_descriptor_proto
            .iter()
            .map(|fd_proto| fd_proto.name.clone().unwrap())
            .collect::<Vec<String>>();
        assert_that!(
            descriptor_names,
            unordered_elements_are![
                eq(&TIRE_PROTO_NAME.to_string()),
                eq(&NESTED_PROTO_NAME.to_string())
            ]
        );
    }

    #[test]
    fn test_generate_runtime_config_specific_package_only_hello_service() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let package_name = HELLO_SERVICE_PACKAGE.to_string(); // "someip.rpc"

        // WHEN
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            None, // No specific bundle
            Some(package_name),
            LEGACY_API,
        );

        // THEN
        assert!(result.is_ok());

        // Check that diagnostic config was NOT created
        // This is the key difference from the (Some, Some) test
        assert!(!dir.path().join("diagnostics-config.binpb").exists());

        // Check that SOME/IP config was NOT created
        assert!(!dir.path().join("someip-config.binpb").exists());

        // --- Verification of isolated VSIDL config ---
        let vsidl_config_path = dir.path().join("vsidl-config.binpb");
        let vsidl_config: VsidlRuntimeConfig =
            read_proto_from_file(&vsidl_config_path).expect("Failed to read vsidl-config.binpb");

        // 1. Check unit types: Should only contain "HelloInterface" RPC
        assert_eq!(vsidl_config.unit_type_mapping.len(), 1);
        assert_that!(
            vsidl_config.unit_type_mapping,
            unordered_elements_are![eq(&UnitTypeDescriptorMapping {
                package_name: HELLO_SERVICE_PACKAGE.to_string(),
                service_bundle_name: "".to_string(), // deprecated
                type_name: HELLO_INTERFACE_TYPE.to_string(),
                archetype_metadata: Some(Archetype_metadata::RpcMetadata(RpcMetadata {
                    method_mapping: vec![
                        RpcMethodMapping {
                            method_name: "SayHello".to_string(),
                            argument_message_name: "someip.rpc.HelloMessageRequest".to_string(),
                            return_message_name: "someip.rpc.HelloMessageResponse".to_string(),
                            ..Default::default()
                        },
                        RpcMethodMapping {
                            method_name: "Foo".to_string(),
                            argument_message_name: "google.protobuf.Empty".to_string(),
                            return_message_name: "google.protobuf.BoolValue".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })),
                ..Default::default()
            }),]
        );

        // 2. Check message descriptors
        assert_eq!(vsidl_config.descriptor_mapping.len(), 3);

        // 3. Check file descriptors
        assert_eq!(vsidl_config.file_descriptor_proto.len(), 2);
    }

    #[test]
    fn test_generate_runtime_config_bundle_only_succeeds() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = TIRE_BUNDLE.to_string(); // "Manager"

        // WHEN
        // Pass a bundle name but no package name.
        // The logic in `generate_runtime_config` should now call
        // `try_find_package_for_bundle_config` and succeed.
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            None, // Package is None
            LEGACY_API,
        );

        // THEN
        assert!(
            result.is_ok(),
            "Expected generation to succeed, but it failed: {:?}",
            result.err()
        );
        assert!(dir.path().join("vsidl-config.binpb").exists());
        assert!(dir.path().join("diagnostics-config.binpb").exists());
    }

    #[test]
    fn test_generate_runtime_config_invalid_bundle_only_returns_error() {
        // GIVEN
        let dir = tempdir().expect("Failed to create temp dir");
        let catalog = build_catalog();
        let (bundles, symbol_table) = form_bundles(&catalog);
        let bundle_name = "NonExistentBundle".to_string(); // A bundle that doesn't exist

        // WHEN
        // Pass a bundle name that cannot be found, and no package name
        let result = generate_runtime_config(
            &catalog,
            &symbol_table,
            &bundles,
            dir.path(),
            Some(bundle_name),
            None, // Package is None
            LEGACY_API,
        );

        // THEN
        // The logic should fail inside `try_find_package_for_bundle_config`
        // and propagate the error.
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();

        // Check for the error message from `try_find_package_for_bundle_config`
        assert!(
            error_message.contains("Unable to find service bundle with name NonExistentBundle"),
            "Error message was: {}",
            error_message
        );

        // Check that no files were created
        assert!(!dir.path().join("vsidl-config.binpb").exists());
        assert!(!dir.path().join("someip-config.binpb").exists());
        assert!(!dir.path().join("diagnostics-config.binpb").exists());
    }
}
