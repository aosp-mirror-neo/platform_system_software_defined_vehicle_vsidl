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
//! Generates apex files for a VSIDL project.

use crate::cli::{AndroidBpFormat, TextprotoFormat};
use crate::rust::generator::{APEX_DIR, CONFIGS_DIR};
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::util::write_bytes_to_file;
use anyhow::Result;
use handlebars::Handlebars;
use heck::ToSnakeCase;
use serde_derive::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vsidl_v1_parser::rpc_server_symbol::RpcServerSymbol;
use vsidl_v1_parser::service_bundle_symbol::ServiceBundleSource;
use vsidl_v1_parser::SymbolTable;

#[derive(Serialize)]
struct AclUnitType {
    name: String,
    acl_name: String,
    members: BTreeSet<String>,
}

#[derive(Serialize)]
struct AclServiceUnit {
    name: String,
    acl_name: String,
    members: BTreeSet<String>,
}

#[derive(Serialize)]
struct AclData {
    message_unit_types: Vec<AclUnitType>,
    message_service_units: Vec<AclServiceUnit>,
    rpc_unit_types: Vec<AclUnitType>,
    rpc_service_units: Vec<AclServiceUnit>,
}

#[derive(Serialize, Default)]
struct PermissionPublisher {
    message: String,
    topics: Vec<String>,
    allow_all_topics: bool,
}

#[derive(Serialize, Default)]
struct PermissionSubscriber {
    message: String,
    topics: Vec<String>,
    allow_all_topics: bool,
}

#[derive(Serialize, Default)]
struct PermissionServer {
    service: String,
    channels: Vec<String>,
    allow_all_channels: bool,
}

#[derive(Serialize, Default)]
struct PermissionClient {
    service: String,
    channels: Vec<String>,
    allow_all_channels: bool,
}

#[derive(Serialize, Default)]
struct PermissionBundleData {
    publishers: Vec<PermissionPublisher>,
    subscribers: Vec<PermissionSubscriber>,
    servers: Vec<PermissionServer>,
    clients: Vec<PermissionClient>,
}

pub struct GenerateApex;

impl GeneratorStepExecution for GenerateApex {
    fn should_run(&self, state: &GeneratorToolState) -> bool {
        state.cli.apex // Only run if --apex flag is provided
    }

    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let output_path = &state.cli.output_path.clone().unwrap_or_default();
        let symbol_table = &state.symbol_table;
        let handlebars = state.handlebars.as_ref().unwrap();

        let service_bundles_symbols: Vec<_> = symbol_table
            .get_all_service_bundle_symbols()
            .into_iter()
            .filter(|service_bundle| service_bundle.source == ServiceBundleSource::CATALOG)
            .collect();
        if service_bundles_symbols.is_empty() {
            anyhow::bail!("No service bundles found in the catalog.");
        }

        let package_name =
            service_bundles_symbols[0].symbol.enclosing_scope(symbol_table).full_name(symbol_table);

        let service_bundle_names: Vec<String> =
            service_bundles_symbols.iter().map(|s| s.name().to_string()).collect();

        let service_bundles_data: Vec<_> = service_bundles_symbols
            .iter()
            .map(|s| {
                let name = s.name();
                let full_name = s.full_name(symbol_table);
                let short_name =
                    name.strip_prefix("ServiceBundle").unwrap_or(&name).to_snake_case();
                json!({
                    "name": name,
                    "full_name": full_name,
                    "short_name": short_name,
                    "snake_case_name": name.to_snake_case(),
                })
            })
            .collect();

        let acl_data = generate_acl_data(symbol_table);
        let permissions_data = generate_permissions_data(symbol_table);
        let base_package_name =
            package_name.rsplit_once('.').map(|(p, _)| p).unwrap_or(&package_name);

        let use_permissions = state.cli.target_api.to_lowercase() == "latest";

        let data = json!({
            "year": crate::rust::generator::get_current_year(),
            "package_name": package_name,
            "base_package_name": base_package_name,
            "service_bundle_names": service_bundle_names,
            "service_bundles": service_bundles_data,
            "acls": acl_data,
            "permissions": permissions_data,
            "use_permissions": use_permissions,
            "use_acls": !use_permissions,
        });

        generate_apex_files(
            output_path,
            handlebars,
            &data,
            state.cli.android_bp_formatter,
            state.cli.textproto_formatter,
        )?;
        if use_permissions {
            generate_permissions_files(
                output_path,
                handlebars,
                &data,
                state.cli.android_bp_formatter,
                state.cli.textproto_formatter,
            )?;
        } else {
            generate_config_files(
                output_path,
                handlebars,
                &data,
                state.cli.android_bp_formatter,
                state.cli.textproto_formatter,
            )?;
        }
        generate_orchestration_files(
            output_path,
            handlebars,
            &data,
            state.cli.android_bp_formatter,
            state.cli.textproto_formatter,
        )?;
        Ok(())
    }
}

fn generate_acl_data(symbol_table: &SymbolTable) -> AclData {
    let mut message_unit_types_map: BTreeMap<String, AclUnitType> = BTreeMap::new();
    let mut message_service_units = Vec::new();

    // Keep stable order across multiple runs of VSIDLC
    let mut publishers = symbol_table.get_all_publisher_symbols();
    publishers.sort_by_cached_key(|p| {
        (
            p.message(symbol_table).name().to_string(),
            p.owning_service_bundle(symbol_table).full_name(symbol_table),
        )
    });

    for publisher in publishers {
        let message = publisher.message(symbol_table);
        let message_name = message.name().to_string();
        let publisher_bundle_fqn =
            publisher.owning_service_bundle(symbol_table).full_name(symbol_table);

        // Unit Type for the Publication Topic
        let unit_type =
            message_unit_types_map.entry(message_name.clone()).or_insert_with(|| AclUnitType {
                name: message_name.clone(),
                acl_name: format!("{}-topic-acl", message_name.to_lowercase().replace('_', "-")),
                members: BTreeSet::new(),
            });
        unit_type.members.insert(format!("*:{}", publisher_bundle_fqn));

        // Service Unit for each channel of the Publisher
        let channels_or_topics = if symbol_table.uses_pdk29() {
            publisher.channels(symbol_table)
        } else {
            publisher.topics(symbol_table)
        };

        for channel in channels_or_topics {
            let subscribers: BTreeSet<String> = message
                .subscribers(symbol_table)
                .iter()
                .map(|s| {
                    format!("*:{}", s.owning_service_bundle(symbol_table).full_name(symbol_table))
                })
                .collect();

            let service_unit_name = publisher.service_unit_name(symbol_table, &channel);
            message_service_units.push(AclServiceUnit {
                name: service_unit_name.clone(),
                acl_name: format!("{}-publisher-acl", service_unit_name.replace('_', "-")),
                members: subscribers,
            });
        }
    }

    let mut rpc_unit_types_map: BTreeMap<String, AclUnitType> = BTreeMap::new();
    let mut rpc_service_units = Vec::new();

    // Keep stable order across multiple runs of VSIDLC
    let mut rpc_servers = symbol_table.get_all_rpc_server_symbols();
    rpc_servers.sort_by_cached_key(|s| {
        (
            s.service(symbol_table).name().to_string(),
            s.owning_service_bundle(symbol_table).full_name(symbol_table),
        )
    });

    for server in rpc_servers {
        let service = server.service(symbol_table);
        let service_name = service.name().to_string();
        let server_bundle_fqn = server.owning_service_bundle(symbol_table).full_name(symbol_table);

        // Unit Type for the RPC Service
        let unit_type =
            rpc_unit_types_map.entry(service_name.clone()).or_insert_with(|| AclUnitType {
                name: service_name.clone(),
                acl_name: format!("{}-service-acl", service_name.to_lowercase().replace('_', "-")),
                members: BTreeSet::new(),
            });
        unit_type.members.insert(format!("*:{}", server_bundle_fqn));

        // Service Unit for the specific Server
        let clients: BTreeSet<String> = service
            .clients(symbol_table)
            .iter()
            .map(|c| format!("*:{}", c.owning_service_bundle(symbol_table).full_name(symbol_table)))
            .collect();

        let service_unit_name = server.service_unit_name(symbol_table);
        rpc_service_units.push(AclServiceUnit {
            name: service_unit_name.clone(),
            acl_name: format!("{}-client-acl", service_unit_name.replace('_', "-")),
            members: clients,
        });
    }

    AclData {
        message_unit_types: message_unit_types_map.into_values().collect(),
        message_service_units,
        rpc_unit_types: rpc_unit_types_map.into_values().collect(),
        rpc_service_units,
    }
}

fn generate_permissions_data(symbol_table: &SymbolTable) -> BTreeMap<String, PermissionBundleData> {
    let mut permissions_map: BTreeMap<String, PermissionBundleData> = BTreeMap::new();

    // Publishers
    for publisher in symbol_table.get_all_publisher_symbols() {
        let bundle_fqn = publisher.owning_service_bundle(symbol_table).full_name(symbol_table);
        let bundle_perms = permissions_map.entry(bundle_fqn).or_default();

        let message = publisher.message(symbol_table).full_name(symbol_table);
        let topics = publisher.topics(symbol_table);

        bundle_perms.publishers.push(PermissionPublisher {
            message,
            topics,
            allow_all_topics: false,
        });
    }

    // Subscribers
    for subscriber in symbol_table.get_all_subscriber_symbols() {
        let bundle_fqn = subscriber.owning_service_bundle(symbol_table).full_name(symbol_table);
        let bundle_perms = permissions_map.entry(bundle_fqn).or_default();

        let message = subscriber.message(symbol_table).full_name(symbol_table);
        let topics = subscriber.topics(symbol_table);

        bundle_perms.subscribers.push(PermissionSubscriber {
            message,
            topics,
            allow_all_topics: false,
        });
    }

    // Servers
    for server in symbol_table.get_all_rpc_server_symbols() {
        let bundle_fqn = server.owning_service_bundle(symbol_table).full_name(symbol_table);
        let bundle_perms = permissions_map.entry(bundle_fqn).or_default();

        let service = server.service(symbol_table).full_name(symbol_table);
        let channel = RpcServerSymbol::channel(server, symbol_table).name().to_string();

        bundle_perms.servers.push(PermissionServer {
            service,
            channels: vec![channel],
            allow_all_channels: false,
        });
    }

    // Clients
    for client in symbol_table.get_all_rpc_client_symbols() {
        let bundle_fqn = client.owning_service_bundle(symbol_table).full_name(symbol_table);
        let bundle_perms = permissions_map.entry(bundle_fqn).or_default();

        let service = client.service(symbol_table).full_name(symbol_table);
        let channel = client.channel(symbol_table).name().to_string();

        bundle_perms.clients.push(PermissionClient {
            service,
            channels: vec![channel],
            allow_all_channels: false,
        });
    }

    permissions_map
}

/// Generates the files in the `apex` directory.
fn generate_apex_files(
    output_path: &Path,
    handlebars: &Handlebars,
    data: &serde_json::Value,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let apex_dir = output_path.join(APEX_DIR);
    fs::create_dir_all(&apex_dir)?;

    let package_name = data["package_name"].as_str().unwrap();

    let created_private = create_private_key(package_name, &apex_dir)?;
    let pubkey_path = apex_dir.join(format!("{}.avbpubkey", package_name));
    if created_private || !pubkey_path.exists() {
        extract_public_key(package_name, &apex_dir)?;
    }

    let bp_content = handlebars.render("apex_android_bp", data)?;
    write_bytes_to_file(
        &bp_content,
        &apex_dir.join("Android.bp"),
        android_bp_format,
        textproto_format,
    )?;

    let manifest_content = handlebars.render("apex_manifest_json", data)?;
    write_bytes_to_file(
        &manifest_content,
        &apex_dir.join("apex_manifest.json"),
        android_bp_format,
        textproto_format,
    )?;

    let file_contexts_content = handlebars.render("file_contexts", data)?;
    write_bytes_to_file(
        &file_contexts_content,
        &apex_dir.join("file_contexts"),
        android_bp_format,
        textproto_format,
    )?;

    let linker_config_content = handlebars.render("linker_config_json", data)?;
    write_bytes_to_file(
        &linker_config_content,
        &apex_dir.join("linker.config.json"),
        android_bp_format,
        textproto_format,
    )?;

    let sdv_manifest_content = handlebars.render("sdv_service_bundles_manifest", data)?;
    write_bytes_to_file(
        &sdv_manifest_content,
        &apex_dir.join("sdv_service_bundles_manifest.textproto"),
        android_bp_format,
        textproto_format,
    )?;

    Ok(())
}

/// Generates the files in the `configs` directory.
fn generate_config_files(
    output_path: &Path,
    handlebars: &Handlebars,
    data: &serde_json::Value,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let config_dir = output_path.join(CONFIGS_DIR).join("acls");
    fs::create_dir_all(&config_dir)?;

    let bp_content = handlebars.render("acls_android_bp", data)?;
    write_bytes_to_file(
        &bp_content,
        &config_dir.join("Android.bp"),
        android_bp_format,
        textproto_format,
    )?;

    let acl_file_name = format!("{}_acls.textproto", data["package_name"].as_str().unwrap());
    let acl_content = handlebars.render("service_bundles_acls", data)?;
    write_bytes_to_file(
        &acl_content,
        &config_dir.join(acl_file_name),
        android_bp_format,
        textproto_format,
    )?;

    Ok(())
}

/// Generates the files in the `configs/permissions` directory.
fn generate_permissions_files(
    output_path: &Path,
    handlebars: &Handlebars,
    data: &serde_json::Value,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let config_dir = output_path.join("configs/permissions");
    fs::create_dir_all(&config_dir)?;

    let bp_content = handlebars.render("permissions_android_bp", data)?;
    write_bytes_to_file(
        &bp_content,
        &config_dir.join("Android.bp"),
        android_bp_format,
        textproto_format,
    )?;

    let service_bundles = data["service_bundles"].as_array().unwrap();
    let permissions_data = data["permissions"].as_object().unwrap();

    for service_bundle_data in service_bundles {
        let service_bundle_full_name = service_bundle_data["full_name"].as_str().unwrap();
        let file_name = format!("{}.permissions.textproto", service_bundle_full_name);

        if let Some(bundle_perms) = permissions_data.get(service_bundle_full_name) {
            let mut template_data = bundle_perms.clone();
            template_data
                .as_object_mut()
                .unwrap()
                .insert("year".to_string(), json!(crate::rust::generator::get_current_year()));

            let content = handlebars.render("service_bundle_permissions", &template_data)?;
            write_bytes_to_file(
                &content,
                &config_dir.join(file_name),
                android_bp_format,
                textproto_format,
            )?;
        } else {
            // Write an empty permissions file if no permissions are defined for this bundle
            let empty_perms = json!({
                "year": crate::rust::generator::get_current_year(),
                "publishers": [],
                "subscribers": [],
                "servers": [],
                "clients": [],
            });
            let content = handlebars.render("service_bundle_permissions", &empty_perms)?;
            write_bytes_to_file(
                &content,
                &config_dir.join(file_name),
                android_bp_format,
                textproto_format,
            )?;
        }
    }

    Ok(())
}

/// Generates the files in the `configs/orchestration` directory.
fn generate_orchestration_files(
    output_path: &Path,
    handlebars: &Handlebars,
    data: &serde_json::Value,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let config_dir = output_path.join(CONFIGS_DIR).join("orchestration");
    fs::create_dir_all(&config_dir)?;

    let package_name = data["package_name"].as_str().unwrap();
    let vm_name = package_name.rsplit_once('.').map(|(_, s)| s).unwrap_or(package_name);
    let mut bp_data = data.clone();
    bp_data.as_object_mut().unwrap().insert("vm_name".to_string(), json!(vm_name));

    let bp_content = handlebars.render("orchestration_android_bp", &bp_data)?;
    write_bytes_to_file(
        &bp_content,
        &config_dir.join("Android.bp"),
        android_bp_format,
        textproto_format,
    )?;

    let service_bundles = data["service_bundles"].as_array().unwrap();

    for service_bundle_data in service_bundles {
        let service_bundle_name = service_bundle_data["name"].as_str().unwrap();
        let short_name = service_bundle_data["short_name"].as_str().unwrap();
        let file_name = format!("{}_config.textproto", short_name);

        let mut template_data = data.clone();
        let map = template_data.as_object_mut().unwrap();
        map.insert("service_bundle_name".to_string(), json!(service_bundle_name));
        map.insert("vm_name".to_string(), json!(vm_name));

        let content = handlebars.render("service_bundle_orchestration_config", &template_data)?;
        write_bytes_to_file(
            &content,
            &config_dir.join(file_name),
            android_bp_format,
            textproto_format,
        )?;
    }

    let vm_config_file_name = format!("vm_{}_orch_config.textproto", vm_name);
    let mut vm_template_data = data.clone();
    vm_template_data.as_object_mut().unwrap().insert("vm_name".to_string(), json!(vm_name));

    let vm_config_content = handlebars.render("vm_orchestration_config", &vm_template_data)?;
    write_bytes_to_file(
        &vm_config_content,
        &config_dir.join(vm_config_file_name),
        android_bp_format,
        textproto_format,
    )?;

    Ok(())
}

fn create_private_key(package_name: &str, apex_dir: &Path) -> anyhow::Result<bool> {
    let key_name = format!("{}.pem", package_name);
    let key_path = apex_dir.join(&key_name);
    if key_path.exists() {
        return Ok(false);
    }

    let output = Command::new("openssl")
        .current_dir(apex_dir)
        .arg("genrsa")
        .arg("-out")
        .arg(key_name)
        .arg("4096")
        .output()?;
    if !output.status.success() {
        anyhow::bail!("Failed to generate private key: {:?}", output);
    }
    Ok(true)
}

fn extract_public_key(package_name: &str, apex_dir: &Path) -> anyhow::Result<()> {
    let avbtool_binary = find_binary("avbtool")?;
    let output = Command::new(avbtool_binary)
        .arg("extract_public_key")
        .arg("--key")
        .arg(apex_dir.join(format!("{}.pem", package_name)))
        .arg("--output")
        .arg(apex_dir.join(format!("{}.avbpubkey", package_name)))
        .output()?;
    if !output.status.success() {
        anyhow::bail!("Failed to extract public key: {:?}", output);
    }
    Ok(())
}

fn find_binary(name: &str) -> anyhow::Result<PathBuf> {
    let mut current_exe = std::env::current_exe()?;
    current_exe.pop();
    let paths = std::env::var_os("PATH").unwrap();
    for mut path in std::iter::once(current_exe.clone()).chain(std::env::split_paths(&paths)) {
        path.push(name);
        if path.exists() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "could not find '{}' in the directory of the binary ({}) or in $PATH ({})",
        name,
        current_exe.to_string_lossy(),
        paths.to_string_lossy(),
    )
}
