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

use crate::rust::generator::extract_filename;
use crate::rust::rpc_service::RpcMethodContext;
use handlebars::Handlebars;
use indexmap::{IndexMap, IndexSet};
use serde_derive::Serialize;
use vsidl_v1_parser::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_parser::proto_publication_symbol::ProtoPublicationSymbol;
use vsidl_v1_parser::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_parser::publisher_symbol::PublisherSymbol;
use vsidl_v1_parser::rpc_client_symbol::RpcClientSymbol;
use vsidl_v1_parser::rpc_server_symbol::RpcServerSymbol;
use vsidl_v1_parser::service_bundle_symbol::ServiceBundleSymbol;
use vsidl_v1_parser::subscriber_symbol::SubscriberSymbol;
use vsidl_v1_parser::SymbolTable;

/// Extract protobuf message name from Unit Type typename since it might be subpackaged
/// E.g. extract `ProtoMsg` from `sub.pkg.ProtoMsg`
pub(super) fn extract_proto_typename_from_unit_type(type_name: &str) -> &str {
    if let Some(idx) = type_name.rfind('.') {
        &type_name[idx + 1..]
    } else {
        type_name
    }
}

#[derive(Serialize, Hash, Eq, PartialEq, Clone)]
pub(super) struct ServiceUnitContext {
    pub(super) unit_type: UnitTypeContext,
    pub(super) has_user_defined_name: bool,
    pub(super) user_defined_name: String,
    pub(super) servers: Vec<(String, String)>,
    pub(super) has_alternative_servers: bool,
    pub(super) channel: String,
}

impl ServiceUnitContext {
    pub(super) fn from_rpc_server(
        rpc_server: &RpcServerSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        Self {
            unit_type: UnitTypeContext::from_rpc_service(
                rpc_server.service(symbol_table),
                symbol_table,
            ),
            has_user_defined_name: rpc_server.has_user_defined_name(),
            user_defined_name: rpc_server.service_unit_name(symbol_table).clone(),
            servers: Vec::new(),
            has_alternative_servers: false,
            channel: if symbol_table.uses_pdk29() {
                "".to_string()
            } else {
                RpcServerSymbol::channel(rpc_server, symbol_table).name()
            },
        }
    }

    pub(super) fn from_rpc_client(
        rpc_client: &RpcClientSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let mut servers: Vec<(String, String)> = rpc_client
            .service(symbol_table)
            .servers(symbol_table)
            .iter()
            .map(|s| s.owning_service_bundle(symbol_table))
            .map(|sb| (sb.package(symbol_table).to_string(), sb.name.to_string()))
            .collect();

        servers.sort();
        servers.dedup();

        Self {
            unit_type: UnitTypeContext::from_rpc_service(
                rpc_client.service(symbol_table),
                symbol_table,
            ),
            has_user_defined_name: false, // RPC clients don't have user-defined names
            user_defined_name: "".to_string(),
            servers: servers.clone(),
            has_alternative_servers: servers.len() > 1,
            channel: if symbol_table.uses_pdk29() {
                "".to_string()
            } else {
                RpcClientSymbol::channel(rpc_client, symbol_table).name()
            },
        }
    }
}

#[derive(Serialize, Hash, Eq, PartialEq, Clone)]
pub(super) struct UnitTypeContext {
    pub(super) crate_name: String,
    pub(super) proto_filename: String,
    pub(super) package: String,
    bundle_name: String,
    pub(super) type_name: String,
    pub(super) proto_type_name: String,
    pub(super) is_rpc_service: bool,
    pub(super) methods: Vec<RpcMethodContext>,
}
impl UnitTypeContext {
    pub(super) fn from_rpc_service(
        rpc_service: &ProtoRpcServiceSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let crate_name = crate::rust::generator::get_rpc_crate_name(
            rpc_service.package(symbol_table),
            rpc_service.name().clone(),
        );

        let methods = rpc_service
            .method
            .iter()
            .map(|method| RpcMethodContext::from(method.clone(), rpc_service.clone(), symbol_table))
            .collect();

        UnitTypeContext {
            crate_name,
            proto_filename: extract_filename(&rpc_service.proto_metadata.filename).to_string(),
            package: rpc_service.package(symbol_table),
            bundle_name: "".to_string(), // service bundle names as part of unit types are deprecated
            type_name: rpc_service.name().clone(),
            proto_type_name: extract_proto_typename_from_unit_type(&rpc_service.name()).to_string(),
            is_rpc_service: true,
            methods,
        }
    }

    pub(super) fn from_publication(
        publication: &ProtoPublicationSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let crate_name = publication.proto_metadata.rust_crate_name.clone().unwrap();
        let methods = Vec::new();

        UnitTypeContext {
            crate_name,
            proto_filename: extract_filename(&publication.proto_metadata.filename).to_string(),
            package: publication.package(symbol_table),
            bundle_name: "".to_string(), // service bundle names as part of unit types are deprecated
            type_name: publication.name().clone(),
            proto_type_name: extract_proto_typename_from_unit_type(&publication.name()).to_string(),
            is_rpc_service: false,
            methods,
        }
    }

    pub(super) fn from_message(message: &ProtoMessageSymbol, symbol_table: &SymbolTable) -> Self {
        let crate_name = message.proto_metadata.rust_crate_name.clone().unwrap();
        let methods = Vec::new();

        UnitTypeContext {
            crate_name,
            proto_filename: extract_filename(&message.proto_metadata.filename).to_string(),
            package: message.package(symbol_table),
            bundle_name: "".to_string(), // service bundle names as part of unit types are deprecated
            type_name: message.name().clone(),
            proto_type_name: extract_proto_typename_from_unit_type(&message.name()).to_string(),
            is_rpc_service: false,
            methods,
        }
    }
}

#[derive(Serialize, Clone)]
pub(super) struct PublicationTypeContext {
    pub(super) unit_type: UnitTypeContext,
    pub(super) proto_enum_filename: String,
    pub(super) proto_enum_name: String,
    pub(super) proto_enum_crate_name: String,
    pub(super) message_size: usize,
    pub(super) message_count: u32,
    pub(super) is_multipub: bool,
    pub(super) message_fqn: String,
    pub(super) proto_dependencies: Vec<ProtoDependencyContext>,
}
impl PublicationTypeContext {
    pub(super) fn from(
        proto_publication: &ProtoPublicationSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let channels_enum = proto_publication.channels_enum(symbol_table);

        PublicationTypeContext {
            unit_type: UnitTypeContext::from_publication(proto_publication, symbol_table),
            proto_enum_filename: extract_filename(&channels_enum.proto_metadata.filename.clone())
                .to_string(),
            proto_enum_crate_name: channels_enum.proto_metadata.rust_crate_name.clone().unwrap(),
            proto_enum_name: channels_enum.name.clone().unwrap().clone(),
            message_size: proto_publication.message_size.size,
            message_count: proto_publication.message_count,
            is_multipub: proto_publication.is_multi_pub(),
            message_fqn: proto_publication.full_name(symbol_table),
            proto_dependencies: proto_publication
                .proto_dependencies
                .iter()
                .map(ProtoDependencyContext::from)
                .collect(),
        }
    }

    pub(super) fn from_message(
        proto_message: &ProtoMessageSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        PublicationTypeContext {
            unit_type: UnitTypeContext::from_message(proto_message, symbol_table),
            proto_enum_filename: extract_filename(&proto_message.proto_metadata.filename.clone())
                .to_string(),
            proto_enum_crate_name: proto_message.proto_metadata.rust_crate_name.clone().unwrap(),
            proto_enum_name: proto_message.name.clone().unwrap().clone(),
            message_size: proto_message.max_size.clone().unwrap().size,
            // In the deprecated annotation-based channel definition, all Proto messages of the
            // same type shared a common 'message_count'. Now, every publisher can set its own capacity.
            // For backwards compatibility, temporarily use the maximum of all channels.
            message_count: proto_message
                .topics(symbol_table)
                .iter()
                .map(|c| c.capacity as u32)
                .max()
                .unwrap_or_default(),
            // TODO: b/477164517 - Set the is_multipub flag to true once all types of subscribers / observers
            // support the new channel concept. Then test with atest sdv_core_middleware_unit_vsidl_pubsub_e2e_test
            is_multipub: proto_message
                .topics(symbol_table)
                .iter()
                .any(|channel| channel.publishers(symbol_table).len() > 1),
            message_fqn: proto_message.full_name(symbol_table),
            proto_dependencies: proto_message
                .proto_dependencies
                .iter()
                .map(ProtoDependencyContext::from)
                .collect(),
        }
    }
}

#[derive(Serialize, Clone)]
pub(super) struct ProtoDependencyContext {
    pub(super) file_name: String,
    pub(super) crate_name: String,
}
impl ProtoDependencyContext {
    pub(super) fn from(proto_metadata: &vsidl_v1_parser::ProtoMetadata) -> Self {
        Self {
            file_name: Self::file_name_no_extension(&proto_metadata.filename).to_string(),
            crate_name: proto_metadata.rust_crate_name.clone().unwrap(),
        }
    }
    fn file_name_no_extension(file_name: &str) -> &str {
        let base_name_delimiter = '/';
        let file_ext_delimiter = '.';
        let start =
            if let Some(index) = file_name.rfind(base_name_delimiter) { index + 1 } else { 0 };
        let end = if let Some(index) = file_name.rfind(file_ext_delimiter) {
            index
        } else {
            file_name.len() - 1
        };
        file_name.get(start..end).expect("No filename found for proto file path.")
    }
}

#[derive(Serialize)]
pub(super) struct PublicationChannelContext {
    pub(super) channel_name: String,
    pub(super) service_unit_name: String,
    pub(super) capacity: u32,
}

impl PublicationChannelContext {
    pub(super) fn new(channel_name: String, service_unit_name: String, capacity: u32) -> Self {
        Self { channel_name, service_unit_name, capacity }
    }
}

#[derive(Serialize)]
pub(super) struct SubscriptionChannelContext {
    pub(super) channel_name: String,
    pub(super) service_unit_name: String,
    pub(super) offering_service_bundle_name: String,
    pub(super) offering_service_bundle_package: String,
}

impl SubscriptionChannelContext {
    pub(super) fn new(
        channel_name: String,
        service_unit_name: String,
        offering_service_bundle_name: String,
        offering_service_bundle_package: String,
    ) -> Self {
        Self {
            channel_name,
            service_unit_name,
            offering_service_bundle_name,
            offering_service_bundle_package,
        }
    }
}

#[derive(Serialize)]
pub(super) struct ServiceBundleContext {
    year: i32,
    pub(super) service_bundle_name: String,
    has_unit_types: bool,
    has_clients_or_subscriptions: bool,
    is_publisher: bool,
    is_subscriber: bool,
    is_server: bool,
    is_client: bool,
    has_diagnostics: bool,
    is_empty: bool,
    pub(super) unit_types: IndexSet<UnitTypeContext>,
    pub(super) rpc_servers: IndexSet<ServiceUnitContext>,
    pub(super) rpc_clients: IndexSet<ServiceUnitContext>,
    pub(super) rpc_client_groups: Vec<(ServiceUnitContext, Vec<ServiceUnitContext>)>,
    pub(super) publications: Vec<(PublicationTypeContext, Vec<PublicationChannelContext>)>,
    pub(super) subscriptions: Vec<(PublicationTypeContext, Vec<SubscriptionChannelContext>)>,
    mutable_service: bool,
    used_comms: bool,
    register_reflection_metadata: bool,
}

impl ServiceBundleContext {
    pub(super) fn from(
        service_bundle: &ServiceBundleSymbol,
        symbol_table: &SymbolTable,
        cli: &crate::Cli,
    ) -> Self {
        let unit_types = Self::collect_utilized_unit_types(service_bundle.clone(), symbol_table);
        let is_publisher = !service_bundle.publishers(symbol_table).is_empty();
        let is_subscriber = !service_bundle.subscribers(symbol_table).is_empty();
        let is_server = !service_bundle.servers(symbol_table).is_empty();
        let is_client = !service_bundle.clients(symbol_table).is_empty();
        let has_diagnostics = service_bundle.diagnostics_info.is_some();
        let is_empty =
            !(is_publisher || is_subscriber || is_server || is_client || has_diagnostics);

        ServiceBundleContext {
            year: crate::rust::generator::get_current_year(),
            service_bundle_name: service_bundle.name.clone(),
            has_unit_types: !unit_types.is_empty(),
            has_clients_or_subscriptions: is_subscriber || is_client,
            is_publisher,
            is_subscriber,
            is_server,
            is_client,
            has_diagnostics,
            is_empty,
            unit_types,
            publications: Self::get_publications_context(service_bundle, symbol_table),
            subscriptions: Self::get_subscriptions_context(service_bundle, symbol_table),
            rpc_clients: service_bundle
                .clients(symbol_table)
                .iter()
                .map(|client| ServiceUnitContext::from_rpc_client(client, symbol_table))
                .collect(),
            rpc_servers: service_bundle
                .servers(symbol_table)
                .iter()
                .map(|server| ServiceUnitContext::from_rpc_server(server, symbol_table))
                .collect(),
            rpc_client_groups: Self::get_rpc_client_groups(service_bundle, symbol_table),
            mutable_service: is_server || (is_publisher && cli.target_api == "pdk29"),
            used_comms: (is_subscriber || is_publisher || is_client || is_server)
                && cli.target_api == "pdk29",
            register_reflection_metadata: service_bundle.register_reflection_metadata,
        }
    }

    pub(super) fn collect_utilized_unit_types(
        service_bundle: ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> IndexSet<UnitTypeContext> {
        let mut unit_types = IndexSet::new();

        for subscriber in &service_bundle.subscribers(symbol_table) {
            unit_types.insert(UnitTypeContext::from_message(
                subscriber.message(symbol_table),
                symbol_table,
            ));
        }

        for publisher in &service_bundle.publishers(symbol_table) {
            unit_types.insert(UnitTypeContext::from_message(
                publisher.message(symbol_table),
                symbol_table,
            ));
        }

        for rpc_server in &service_bundle.servers(symbol_table) {
            unit_types.insert(UnitTypeContext::from_rpc_service(
                rpc_server.service(symbol_table),
                symbol_table,
            ));
        }

        unit_types
    }

    fn get_publications_context(
        service_bundle: &ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> Vec<(PublicationTypeContext, Vec<PublicationChannelContext>)> {
        let mut publications = Vec::new();
        let mut publishers_for_publication: IndexMap<String, Vec<&PublisherSymbol>> =
            IndexMap::new();
        for publisher in &service_bundle.publishers(symbol_table) {
            let message_symbol = publisher.message(symbol_table);
            publishers_for_publication
                .entry(message_symbol.full_name(symbol_table))
                .or_default()
                .push(publisher);
        }

        for (publication_fqn, publishers) in &publishers_for_publication {
            let mut channel_contexts = Vec::new();
            for publisher in publishers {
                if symbol_table.uses_pdk29() {
                    for channel_name in &publisher.channels(symbol_table) {
                        let service_unit_name =
                            publisher.service_unit_name(symbol_table, channel_name);
                        channel_contexts.push(PublicationChannelContext::new(
                            channel_name.to_string(),
                            service_unit_name,
                            symbol_table.resolve_proto_publication_fqn(publication_fqn)[0]
                                .message_count,
                        ));
                    }
                } else {
                    for topic in &publisher.topic_symbols(symbol_table) {
                        let service_unit_name =
                            publisher.service_unit_name(symbol_table, &topic.name());
                        channel_contexts.push(PublicationChannelContext::new(
                            topic.name().to_string(),
                            service_unit_name,
                            publisher.capacity,
                        ));
                    }
                }
            }
            if symbol_table.uses_pdk29() {
                publications.push((
                    PublicationTypeContext::from(
                        symbol_table.resolve_proto_publication_fqn(publication_fqn)[0],
                        symbol_table,
                    ),
                    channel_contexts,
                ));
            } else {
                publications.push((
                    PublicationTypeContext::from_message(
                        symbol_table.resolve_proto_message_fqn(publication_fqn)[0],
                        symbol_table,
                    ),
                    channel_contexts,
                ));
            }
        }
        publications
    }

    fn get_subscriptions_context(
        service_bundle: &ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> Vec<(PublicationTypeContext, Vec<SubscriptionChannelContext>)> {
        let mut subscriptions = Vec::new();
        let mut subscribers_for_publication: IndexMap<String, Vec<&SubscriberSymbol>> =
            IndexMap::new();
        for subscriber in &service_bundle.subscribers(symbol_table) {
            let message_symbol = subscriber.message(symbol_table);
            subscribers_for_publication
                .entry(message_symbol.full_name(symbol_table))
                .or_default()
                .push(subscriber);
        }

        for (publication_fqn, subscribers) in &subscribers_for_publication {
            let mut channel_contexts = Vec::new();
            for subscriber in subscribers {
                if symbol_table.uses_pdk29() {
                    for channel_name in subscriber.channels(symbol_table) {
                        let publisher_for_channel =
                            subscriber.publisher_for_channel(symbol_table, &channel_name);
                        let publisher_service_bundle_symbol =
                            publisher_for_channel.owning_service_bundle(symbol_table);

                        channel_contexts.push(SubscriptionChannelContext::new(
                            channel_name.clone(),
                            publisher_for_channel.service_unit_name(symbol_table, &channel_name),
                            publisher_service_bundle_symbol.name.clone(),
                            publisher_service_bundle_symbol.package(symbol_table),
                        ));
                    }
                } else {
                    for topic in subscriber.topics(symbol_table) {
                        channel_contexts.push(SubscriptionChannelContext::new(
                            topic.clone(),
                            "".to_string(),
                            "".to_string(),
                            "".to_string(),
                        ));
                    }
                }
            }

            if symbol_table.uses_pdk29() {
                subscriptions.push((
                    PublicationTypeContext::from(
                        symbol_table.resolve_proto_publication_fqn(publication_fqn)[0],
                        symbol_table,
                    ),
                    channel_contexts,
                ));
            } else {
                subscriptions.push((
                    PublicationTypeContext::from_message(
                        symbol_table.resolve_proto_message_fqn(publication_fqn)[0],
                        symbol_table,
                    ),
                    channel_contexts,
                ));
            }
        }
        subscriptions
    }

    fn get_rpc_client_groups(
        service_bundle: &ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> Vec<(ServiceUnitContext, Vec<ServiceUnitContext>)> {
        let mut groups: IndexMap<UnitTypeContext, Vec<ServiceUnitContext>> = IndexMap::new();
        for client in &service_bundle.clients(symbol_table) {
            let context = ServiceUnitContext::from_rpc_client(client, symbol_table);
            groups.entry(context.unit_type.clone()).or_default().push(context);
        }

        groups.into_iter().map(|(_, clients)| (clients[0].clone(), clients)).collect()
    }
}

pub(super) fn render(
    service_bundle: &ServiceBundleSymbol,
    symbol_table: &SymbolTable,
    cli: &crate::Cli,
    handlebars: &Handlebars,
) -> String {
    handlebars
        .render("service_bundle", &ServiceBundleContext::from(service_bundle, symbol_table, cli))
        .unwrap()
}
