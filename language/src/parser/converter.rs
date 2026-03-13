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

// Enables consistent conversion code even for AST nodes with only one rule
#![allow(clippy::single_match)]

use crate::parse_number::parse_string;
use crate::pest_parser::Rule;
use pest::iterators::Pair;
use protobuf::MessageField;
use vsidl_v1::span::Span;
use vsidl_v1::syntax::service_bundle::{Client, Publisher, Server, Subscriber};
use vsidl_v1::syntax::{BuildConfiguration, ServiceBundle, VsidlEntry};

use vsidl_v1::diagnostics_syntax::diagnostics_declaration;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::routine::Method as DiagnosticsMethod;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;

use vsidl_v1::someip::some_ip_mapping::sdv_type_mapping::rpc_interface::Method;
use vsidl_v1::someip::some_ip_mapping::sdv_type_mapping::topic;
use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::rpc_interface;
use vsidl_v1::someip::{
    some_ip_mapping,
    some_ip_mapping::{
        sdv_type_mapping,
        sdv_type_mapping::{
            publication_mapping, rpc_interface_mapping, service_oriented_mapping,
            PublicationMapping, RpcInterfaceMapping, ServiceOrientedMapping,
        },
        some_ip_instance_implementation,
        some_ip_instance_implementation::event,
        some_ip_service_mapping,
    },
    SomeIpMapping,
};
use vsidl_v1::someip_type::some_ip_type;
use vsidl_v1::someip_type::{
    self,
    some_ip_type::{some_ip_array, some_ip_string, some_ip_struct},
};

use vsidl_v1::vhal_proxy::vhal_mapping::{
    data_source::SdvAction as VhalSdvAction, AreaMapping as VhalAreaMapping,
    DataSource as VhalDataSource, DefaultValue as VhalDefaultValue,
    PropertyArea as VhalPropertyArea, VehicleProperty as VhalVehicleProperty,
};
use vsidl_v1::vhal_proxy::VhalMapping;
use vsidl_v1::VehicleAreaConfig::VehicleAreaConfig;
use vsidl_v1::VehiclePropConfig::VehiclePropConfig;
use vsidl_v1::VehiclePropertyAccess::VehiclePropertyAccess;
use vsidl_v1::VehiclePropertyChangeMode::VehiclePropertyChangeMode;

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn get_inner_str(pair: Pair<Rule>) -> &str {
    pair.into_inner().next().unwrap().as_str().trim()
}

fn get_inner_pair(pair: Pair<Rule>) -> Pair<Rule> {
    pair.into_inner().next().unwrap()
}

fn proto_span(pair: Pair<Rule>, file_path: &str) -> MessageField<Span> {
    Some(Span {
        file_path: file_path.to_string(),
        start: pair.as_span().start() as u32,
        end: pair.as_span().end() as u32,
        ..Default::default()
    })
    .into()
}

pub fn convert_to_vsidl_entry(pair: Pair<Rule>, file_path: &str) -> VsidlEntry {
    assert_eq!(pair.as_rule(), Rule::vsidl_file);
    let mut vsidl_entry = VsidlEntry::new();
    vsidl_entry.span = proto_span(pair.clone(), file_path);

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::package => {
                let value = item.into_inner().next().unwrap();
                vsidl_entry.package = unquote(value.as_str());
                vsidl_entry.package_span = proto_span(value, file_path);
            }
            Rule::sdv_service_bundle => {
                vsidl_entry.service_bundle.push(convert_to_service_bundle(item, file_path));
            }
            Rule::some_ip_mapping => {
                vsidl_entry.some_ip_mapping.push(convert_to_some_ip_mapping(item, file_path));
            }
            Rule::vhal_mapping => {
                vsidl_entry.vhal_mapping.push(convert_to_vhal_mapping(item, file_path));
            }
            _ => (),
        }
    }
    vsidl_entry
}

pub fn convert_to_service_bundle(pair: Pair<Rule>, file_path: &str) -> ServiceBundle {
    let mut bundle = ServiceBundle::new();
    bundle.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::service_bundle_body)
        .unwrap_or_else(|| panic!("Expected Rule::service_bundle_body inside service_bundle"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::service_bundle_name_prop => {
                let inner = get_inner_pair(item);
                bundle.name = unquote(inner.as_str());
                bundle.name_span = proto_span(inner, file_path);
            }
            Rule::publisher => {
                bundle.publisher.push(convert_to_publisher(item, file_path));
            }
            Rule::subscriber => {
                bundle.subscriber.push(convert_to_subscriber(item, file_path));
            }
            Rule::server => {
                bundle.server.push(convert_to_server(item, file_path));
            }
            Rule::client => {
                bundle.client.push(convert_to_client(item, file_path));
            }
            Rule::build_configuration => {
                bundle.build_cfg = Some(convert_to_build_configuration(item, file_path)).into();
            }
            Rule::service_bundle_register_reflection_metadata => {
                let inner = get_inner_pair(item);
                bundle.register_reflection_metadata = inner.as_str().parse().unwrap();
                bundle.register_reflection_metadata_span = proto_span(inner, file_path);
            }
            Rule::diagnostics_declaration => {
                bundle.diagnostics_declaration =
                    Some(convert_to_diagnostics_declaration(item, file_path)).into();
            }
            _ => (),
        }
    }
    bundle
}

pub fn convert_to_publisher(pair: Pair<Rule>, file_path: &str) -> Publisher {
    let mut publisher = Publisher::new();
    publisher.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::publisher_body)
        .unwrap_or_else(|| panic!("Expected Rule::publisher_body inside publisher"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::publisher_message => {
                let inner = get_inner_pair(item);
                publisher.message = unquote(inner.as_str());
                publisher.message_span = proto_span(inner, file_path);
            }
            Rule::publisher_topic => {
                let inner = get_inner_pair(item);
                publisher.topic.push(unquote(inner.as_str()));
                publisher.topic_span.push(proto_span(inner, file_path).unwrap());
            }
            Rule::publisher_channel => {
                let inner = get_inner_pair(item);
                publisher.channel.push(unquote(inner.as_str()));
                publisher.channel_span.push(proto_span(inner, file_path).unwrap());
            }
            Rule::publisher_capacity => {
                let inner = get_inner_pair(item.clone());
                publisher.capacity = parse_string::<i64>(get_inner_str(item)).unwrap();
                publisher.capacity_span = proto_span(inner, file_path);
            }
            Rule::publisher_service_unit_name => {
                let inner = get_inner_pair(item);
                publisher.service_unit_name = unquote(inner.as_str());
                publisher.service_unit_name_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    publisher
}

pub fn convert_to_subscriber(pair: Pair<Rule>, file_path: &str) -> Subscriber {
    let mut subscriber = Subscriber::new();
    subscriber.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::subscriber_body)
        .unwrap_or_else(|| panic!("Expected Rule::subscriber_body inside subscriber"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::subscriber_message => {
                let inner = get_inner_pair(item);
                subscriber.message = unquote(inner.as_str());
                subscriber.message_span = proto_span(inner, file_path);
            }
            Rule::subscriber_topic => {
                let inner = get_inner_pair(item);
                subscriber.topic.push(unquote(inner.as_str()));
                subscriber.topic_span.push(proto_span(inner, file_path).unwrap());
            }
            Rule::subscriber_channel => {
                let inner = get_inner_pair(item);
                subscriber.channel.push(unquote(inner.as_str()));
                subscriber.channel_span.push(proto_span(inner, file_path).unwrap());
            }
            _ => (),
        }
    }
    subscriber
}

pub fn convert_to_server(pair: Pair<Rule>, file_path: &str) -> Server {
    let mut server = Server::new();
    server.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::server_body)
        .unwrap_or_else(|| panic!("Expected Rule::server_body inside server"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::server_interface => {
                let inner = get_inner_pair(item);
                server.service = unquote(inner.as_str());
                server.service_span = proto_span(inner, file_path);
            }
            Rule::server_service_unit_name => {
                let inner = get_inner_pair(item);
                server.service_unit_name = unquote(inner.as_str());
                server.service_unit_name_span = proto_span(inner, file_path);
            }
            Rule::server_channel => {
                let inner = get_inner_pair(item);
                server.channel = unquote(inner.as_str());
                server.channel_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    server
}

pub fn convert_to_client(pair: Pair<Rule>, file_path: &str) -> Client {
    let mut client = Client::new();
    client.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::client_body)
        .unwrap_or_else(|| panic!("Expected Rule::client_body inside client"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::client_interface => {
                let inner = get_inner_pair(item);
                client.service = unquote(inner.as_str());
                client.service_span = proto_span(inner, file_path);
            }
            Rule::client_channel => {
                let inner = get_inner_pair(item);
                client.channel = unquote(inner.as_str());
                client.channel_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    client
}

pub fn convert_to_build_configuration(pair: Pair<Rule>, file_path: &str) -> BuildConfiguration {
    let mut build_cfg = BuildConfiguration::new();
    build_cfg.span = proto_span(pair.clone(), file_path);
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::build_configuration_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::build_configuration_body inside build_configuration")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::build_configuration_target_name => {
                let inner = get_inner_pair(item);
                build_cfg.target_name = Some(unquote(inner.as_str()));
                build_cfg.target_name_span = proto_span(inner, file_path);
            }
            Rule::build_configuration_skip_codegen => {
                let inner = get_inner_pair(item);
                build_cfg.skip_codegen = Some(inner.as_str().parse().unwrap());
                build_cfg.skip_codegen_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    build_cfg
}

pub fn convert_to_diagnostics_declaration(
    pair: Pair<Rule>,
    _file_path: &str,
) -> DiagnosticsDeclaration {
    let mut diagnostics_declaration = DiagnosticsDeclaration::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::diagnostics_declaration_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::diagnostics_declaration_body inside diagnostics_declaration")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_data_item => {
                diagnostics_declaration.data_item.push(convert_to_diagnostics_data_item(item));
            }
            Rule::diagnostics_io_control => {
                diagnostics_declaration.io_control.push(convert_to_diagnostics_io_control(item));
            }
            Rule::diagnostics_routine => {
                diagnostics_declaration.routine.push(convert_to_diagnostics_routine(item));
            }
            Rule::diagnostics_event => {
                diagnostics_declaration.event.push(convert_to_diagnostics_event(item));
            }
            _ => (),
        }
    }
    diagnostics_declaration
}

pub fn convert_to_diagnostics_data_item(pair: Pair<Rule>) -> diagnostics_declaration::DataItem {
    let mut data_item = diagnostics_declaration::DataItem::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::diagnostics_data_item_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::diagnostics_data_item_body inside diagnostics_data_item")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_data_item_id => {
                data_item.id = unquote(get_inner_str(item));
            }
            Rule::diagnostics_data_item_message_name => {
                data_item.message_name = unquote(get_inner_str(item));
            }
            Rule::diagnostics_data_item_is_writable => {
                data_item.is_writable = get_inner_str(item).parse().unwrap();
            }
            _ => (),
        }
    }
    data_item
}

pub fn convert_to_diagnostics_io_control(
    pair: Pair<Rule>,
) -> diagnostics_declaration::InputOutputControlData {
    let mut io_control = diagnostics_declaration::InputOutputControlData::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::diagnostics_io_control_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::diagnostics_io_control_body inside diagnostics_io_control")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_io_control_id => {
                io_control.id = unquote(get_inner_str(item));
            }
            Rule::diagnostics_io_control_message_name => {
                io_control.message_name = unquote(get_inner_str(item));
            }
            _ => (),
        }
    }
    io_control
}

pub fn convert_to_diagnostics_routine(pair: Pair<Rule>) -> diagnostics_declaration::Routine {
    let mut routine = diagnostics_declaration::Routine::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::diagnostics_routine_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::diagnostics_routine_body inside diagnostics_routine")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_routine_id => {
                routine.id = unquote(get_inner_str(item));
            }
            Rule::diagnostics_routine_start => {
                let method_body = item
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::diagnostics_method_body)
                    .unwrap();
                routine.start = MessageField::some(convert_to_diagnostics_method(method_body));
            }
            Rule::diagnostics_routine_stop => {
                let method_body = item
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::diagnostics_method_body)
                    .unwrap();
                routine.stop = MessageField::some(convert_to_diagnostics_method(method_body));
            }
            Rule::diagnostics_routine_result => {
                let method_body = item
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::diagnostics_method_body)
                    .unwrap();
                routine.result = MessageField::some(convert_to_diagnostics_method(method_body));
            }
            _ => (),
        }
    }
    routine
}

pub fn convert_to_diagnostics_method(pair: Pair<Rule>) -> DiagnosticsMethod {
    let mut method = DiagnosticsMethod::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_method_request => {
                method.request = unquote(get_inner_str(item));
            }
            Rule::diagnostics_method_response => {
                method.response = unquote(get_inner_str(item));
            }
            _ => (),
        }
    }
    method
}

pub fn convert_to_diagnostics_event(pair: Pair<Rule>) -> diagnostics_declaration::Event {
    let mut event = diagnostics_declaration::Event::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::diagnostics_event_body)
        .unwrap_or_else(|| {
            panic!("Expected Rule::diagnostics_event_body inside diagnostics_event")
        });

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::diagnostics_event_id => {
                event.id = unquote(get_inner_str(item));
            }
            Rule::diagnostics_event_extended_data_message_name => {
                event.extended_data_message_name = unquote(get_inner_str(item));
            }
            Rule::diagnostics_event_fault_listener_mask => {
                event.fault_listener_mask = parse_string::<u32>(get_inner_str(item)).unwrap();
            }
            Rule::diagnostics_event_service_unit_name => {
                event.service_unit_name = unquote(get_inner_str(item));
            }
            _ => (),
        }
    }
    event
}

pub fn convert_to_some_ip_mapping(pair: Pair<Rule>, file_path: &str) -> SomeIpMapping {
    let mut mapping =
        SomeIpMapping { span: proto_span(pair.clone(), file_path), ..Default::default() };
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::some_ip_mapping_body)
        .unwrap_or_else(|| panic!("Expected Rule::some_ip_mapping_body inside some_ip_mapping"));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::some_ip_service_mapping => {
                let inner = item.clone();
                mapping.mapping_type = Some(some_ip_mapping::Mapping_type::SomeipServiceMapping(
                    convert_to_some_ip_service_mapping(inner, file_path),
                ));
                mapping.someip_service_mapping_span = proto_span(item, file_path);
            }
            Rule::some_ip_sdv_type_mapping => {
                let inner = item.clone();
                mapping.mapping_type = Some(some_ip_mapping::Mapping_type::SdvTypeMapping(
                    convert_to_some_ip_sdv_type_mapping(inner, file_path),
                ));
                mapping.sdv_type_mapping_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_service_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_mapping::SomeIpServiceMapping {
    let mut service_mapping = some_ip_mapping::SomeIpServiceMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_service_id => {
                let inner = get_inner_pair(item.clone());
                service_mapping.someip_service_id =
                    parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                service_mapping.someip_service_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_version_major => {
                let inner = get_inner_pair(item.clone());
                service_mapping.version_major =
                    parse_string::<u8>(get_inner_str(item)).unwrap() as u32;
                service_mapping.version_major_span = proto_span(inner, file_path);
            }
            Rule::some_ip_version_minor => {
                let inner = get_inner_pair(item.clone());
                service_mapping.version_minor = parse_string::<u32>(get_inner_str(item)).unwrap();
                service_mapping.version_minor_span = proto_span(inner, file_path);
            }
            Rule::some_ip_instance => {
                let inner = item.clone();
                service_mapping
                    .instance
                    .push(convert_to_some_ip_instance_mapping(inner, file_path));
                service_mapping.instance_span.push(proto_span(item, file_path).unwrap());
            }
            Rule::some_ip_instance_impl => {
                let inner = item.clone();
                service_mapping
                    .instance_impl
                    .push(convert_to_some_ip_instance_implementation(inner, file_path));
                service_mapping.instance_impl_span.push(proto_span(item, file_path).unwrap());
            }
            Rule::some_ip_sd_behavior => {
                let inner = get_inner_pair(item.clone());
                service_mapping.sd_behavior = match get_inner_str(item) {
                    "EXACT_OR_ANY_MINOR_VERSION" => {
                        Some(some_ip_service_mapping::SdVersionDrivenBehavior::EXACT_OR_ANY_MINOR_VERSION.into())
                    }
                    "MINIMUM_MINOR_VERSION" => Some(
                        some_ip_service_mapping::SdVersionDrivenBehavior::MINIMUM_MINOR_VERSION.into(),
                    ),
                    _ => Some(some_ip_service_mapping::SdVersionDrivenBehavior::UNKNOWN.into()),
                };
                service_mapping.sd_behavior_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    service_mapping
}

pub fn convert_to_some_ip_sdv_type_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_mapping::SdvTypeMapping {
    let mut sdv_type_mapping = some_ip_mapping::SdvTypeMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_sdv_type_mapping_unit_type => {
                let inner = get_inner_pair(item.clone()); // some_ip_unit_type_ref
                sdv_type_mapping.unit_type =
                    MessageField::some(convert_to_some_ip_unit_type(inner, file_path));
                sdv_type_mapping.unit_type_span = proto_span(item, file_path);
            }
            Rule::some_ip_sdv_type_mapping_service_id => {
                let inner = get_inner_pair(item.clone());
                sdv_type_mapping.someip_service_id =
                    parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                sdv_type_mapping.someip_service_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_sdv_type_mapping_version_major => {
                let inner = get_inner_pair(item.clone());
                sdv_type_mapping.version_major =
                    parse_string::<u8>(get_inner_str(item)).unwrap() as u32;
                sdv_type_mapping.version_major_span = proto_span(inner, file_path);
            }
            Rule::some_ip_sdv_type_mapping_version_minor => {
                let inner = get_inner_pair(item.clone());
                sdv_type_mapping.version_minor = parse_string::<u32>(get_inner_str(item)).unwrap();
                sdv_type_mapping.version_minor_span = proto_span(inner, file_path);
            }
            Rule::some_ip_sdv_offered_interface => {
                let inner = item.clone();
                sdv_type_mapping.interface_type =
                    Some(sdv_type_mapping::Interface_type::OfferedInterface(
                        convert_to_some_ip_sdv_rpc_interface(inner, file_path),
                    ));
                sdv_type_mapping.offered_interface_span = proto_span(item, file_path);
            }
            Rule::some_ip_offered_topic => {
                let inner = item.clone();
                sdv_type_mapping.interface_type =
                    Some(sdv_type_mapping::Interface_type::OfferedTopic(
                        convert_to_some_ip_offered_topic(inner, file_path),
                    ));
                sdv_type_mapping.offered_topic_span = proto_span(item, file_path);
            }
            Rule::some_ip_service_unit_mapping => {
                let inner = item.clone();
                sdv_type_mapping
                    .service_unit_mapping
                    .push(convert_to_some_ip_service_unit_mapping(inner, file_path));
                sdv_type_mapping
                    .service_unit_mapping_span
                    .push(proto_span(item, file_path).unwrap());
            }
            Rule::some_ip_service_oriented_mapping => {
                let inner = item.clone();
                sdv_type_mapping.service_oriented_mapping = MessageField::some(
                    convert_to_some_ip_service_oriented_mapping(inner, file_path),
                );
                sdv_type_mapping.service_oriented_mapping_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    sdv_type_mapping
}

pub fn convert_to_some_ip_instance_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_mapping::SomeIpInstanceMapping {
    let mut instance_mapping = some_ip_mapping::SomeIpInstanceMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_instance_id => {
                let inner = get_inner_pair(item.clone());
                instance_mapping.instance_id =
                    parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                instance_mapping.instance_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_sdv_service_unit_parameterization_block => {
                let mut key = 0;
                let mut value = String::new();
                for sub_item in item.into_inner() {
                    match sub_item.as_rule() {
                        Rule::some_ip_sdv_service_unit_parameterization_block_key => {
                            key = parse_string::<u32>(get_inner_str(sub_item.clone())).unwrap();
                            instance_mapping
                                .sdv_service_unit_parameterization_span
                                .insert(key, proto_span(sub_item, file_path).unwrap());
                        }
                        Rule::some_ip_sdv_service_unit_parameterization_block_value => {
                            value = unquote(get_inner_str(sub_item));
                        }
                        _ => (),
                    }
                }
                instance_mapping.sdv_service_unit_parameterization.insert(key, value);
            }
            _ => (),
        }
    }
    instance_mapping
}

pub fn convert_to_some_ip_instance_implementation(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_mapping::SomeIpInstanceImplementation {
    let mut instance_impl = some_ip_mapping::SomeIpInstanceImplementation {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_offered_interface => {
                let inner = item.clone();
                instance_impl.interface_type =
                    Some(some_ip_instance_implementation::Interface_type::OfferedInterface(
                        convert_to_some_ip_rpc_interface(inner, file_path),
                    ));
                instance_impl.offered_interface_span = proto_span(item, file_path);
            }
            Rule::some_ip_offered_event => {
                let inner = item.clone();
                instance_impl.interface_type =
                    Some(some_ip_instance_implementation::Interface_type::OfferedEvent(
                        convert_to_some_ip_event(inner, file_path),
                    ));
                instance_impl.offered_event_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    instance_impl
}

pub fn convert_to_some_ip_rpc_interface(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_instance_implementation::RpcInterface {
    let mut rpc_interface = some_ip_instance_implementation::RpcInterface {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_offered_interface_sdv_unit_type => {
                let inner = get_inner_pair(item.clone());
                rpc_interface.sdv_unit_type =
                    MessageField::some(convert_to_some_ip_unit_type(inner, file_path));
                rpc_interface.sdv_unit_type_span = proto_span(item, file_path);
            }
            Rule::some_ip_service_unit => {
                let inner = get_inner_pair(item.clone());
                rpc_interface.service_unit = parse_string::<u32>(get_inner_str(item)).unwrap();
                rpc_interface.service_unit_span = proto_span(inner, file_path);
            }
            Rule::some_ip_method => {
                let inner = item.clone();
                rpc_interface
                    .method
                    .push(convert_to_some_ip_request_response_method(inner, file_path));
                rpc_interface.method_span.push(proto_span(item, file_path).unwrap());
            }
            Rule::some_ip_channel => {
                let inner = get_inner_pair(item.clone());
                rpc_interface.channel = unquote(get_inner_str(item));
                rpc_interface.channel_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    rpc_interface
}

pub fn convert_to_some_ip_unit_type(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_mapping::UnitType {
    let mut unit_type = some_ip_mapping::UnitType {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_unit_type_ref_package_name => {
                let inner = get_inner_pair(item.clone());
                unit_type.package_name = unquote(get_inner_str(item));
                unit_type.package_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_unit_type_ref_service_bundle_name => {
                let inner = get_inner_pair(item.clone());
                unit_type.service_bundle_name = unquote(get_inner_str(item));
                unit_type.service_bundle_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_unit_type_ref_type_name => {
                let inner = get_inner_pair(item.clone());
                unit_type.type_name = unquote(get_inner_str(item));
                unit_type.type_name_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    unit_type
}

pub fn convert_to_some_ip_request_response_method(
    pair: Pair<Rule>,
    file_path: &str,
) -> rpc_interface::RequestResponseMethod {
    let mut method = rpc_interface::RequestResponseMethod {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_method_procedure => {
                let inner = get_inner_pair(item.clone());
                method.procedure = unquote(get_inner_str(item));
                method.procedure_span = proto_span(inner, file_path);
            }
            Rule::some_ip_method_method_id => {
                let inner = get_inner_pair(item.clone());
                method.method_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                method.method_id_span = proto_span(inner, file_path);
            }
            Rule::request_payload_mapping => {
                let inner = get_inner_pair(item.clone()); // some_ip_struct_body
                method.request_payload_mapping =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                method.request_payload_mapping_span = proto_span(item, file_path);
            }
            Rule::response_payload_mapping => {
                let inner = get_inner_pair(item.clone()); // some_ip_struct_body
                method.response_payload_mapping =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                method.response_payload_mapping_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    method
}

pub fn convert_to_some_ip_event(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_instance_implementation::Event {
    let mut event = some_ip_instance_implementation::Event {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_offered_event_group_id => {
                let inner = get_inner_pair(item.clone());
                event.group_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                event.group_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_offered_event_event_id => {
                let inner = get_inner_pair(item.clone());
                event.event_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                event.event_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_simple_event => {
                let inner = item.clone();
                event.event_type = Some(event::Event_type::SimpleEvent(
                    convert_to_some_ip_simple_event(inner, file_path),
                ));
                event.simple_event_span = proto_span(item, file_path);
            }
            Rule::some_ip_group_event => {
                let inner = item.clone();
                event.event_type = Some(event::Event_type::GroupEvent(
                    convert_to_some_ip_group_event(inner, file_path),
                ));
                event.group_event_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    event
}

pub fn convert_to_some_ip_simple_event(pair: Pair<Rule>, file_path: &str) -> event::SimpleEvent {
    let mut simple_event =
        event::SimpleEvent { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_simple_event_sdv_unit_type => {
                let inner = get_inner_pair(item.clone());
                simple_event.sdv_unit_type =
                    MessageField::some(convert_to_some_ip_unit_type(inner, file_path));
                simple_event.sdv_unit_type_span = proto_span(item, file_path);
            }
            Rule::some_ip_simple_event_service_unit => {
                let inner = get_inner_pair(item.clone());
                simple_event.service_unit = parse_string::<u32>(get_inner_str(item)).unwrap();
                simple_event.service_unit_span = proto_span(inner, file_path);
            }
            Rule::some_ip_simple_event_type_definition => {
                let inner = get_inner_pair(item.clone()); // struct_body
                simple_event.type_definition =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                simple_event.type_definition_span = proto_span(item, file_path);
            }
            Rule::some_ip_simple_event_message_translation_mode => {
                let inner = get_inner_pair(item.clone());
                simple_event.message_translation_mode = match get_inner_str(item) {
                    "DYNAMIC_LIBRARY" => {
                        Some(event::simple_event::MessageTranslationMode::DYNAMIC_LIBRARY.into())
                    }
                    "INTERPRET_AS_BYTES" => {
                        Some(event::simple_event::MessageTranslationMode::INTERPRET_AS_BYTES.into())
                    }
                    _ => Some(event::simple_event::MessageTranslationMode::REFLECTION.into()),
                };
                simple_event.message_translation_mode_span = proto_span(inner, file_path);
            }
            Rule::some_ip_simple_event_topic => {
                let inner = get_inner_pair(item.clone());
                simple_event.topic = unquote(get_inner_str(item));
                simple_event.topic_span = proto_span(inner, file_path);
            }
            Rule::some_ip_simple_event_capacity => {
                let inner = get_inner_pair(item.clone());
                simple_event.capacity = parse_string::<u32>(get_inner_str(item)).unwrap();
                simple_event.capacity_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    simple_event
}

pub fn convert_to_some_ip_group_event(pair: Pair<Rule>, file_path: &str) -> event::GroupEvent {
    let mut group_event =
        event::GroupEvent { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_group_event_type_definition => {
                let inner = get_inner_pair(item.clone()); // struct_body
                group_event.type_definition =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                group_event.type_definition_span = proto_span(item, file_path);
            }
            Rule::some_ip_event_part => {
                let inner = item.clone();
                group_event.event_part.push(convert_to_some_ip_group_event_part(inner, file_path));
                group_event.event_part_span.push(proto_span(item, file_path).unwrap());
            }
            _ => (),
        }
    }
    group_event
}

pub fn convert_to_some_ip_group_event_part(
    pair: Pair<Rule>,
    file_path: &str,
) -> event::GroupEventPart {
    let mut group_event_part =
        event::GroupEventPart { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_event_part_sdv_unit_type => {
                let inner = get_inner_pair(item.clone());
                group_event_part.sdv_unit_type =
                    MessageField::some(convert_to_some_ip_unit_type(inner, file_path));
                group_event_part.sdv_unit_type_span = proto_span(item, file_path);
            }
            Rule::some_ip_event_part_service_unit => {
                let inner = get_inner_pair(item.clone());
                group_event_part.service_unit = parse_string::<u32>(get_inner_str(item)).unwrap();
                group_event_part.service_unit_span = proto_span(inner, file_path);
            }
            Rule::some_ip_event_part_field_name_mapping => {
                let mut key = String::new();
                let mut value = String::new();
                let mut key_span = None;
                let inner_body = item.clone();
                for sub_item in inner_body.into_inner() {
                    if let Rule::some_ip_map_entry_str_str = sub_item.as_rule() {
                        for entry_part in sub_item.into_inner() {
                            match entry_part.as_rule() {
                                Rule::some_ip_map_entry_key => {
                                    key = unquote(get_inner_str(entry_part.clone()));
                                    key_span = proto_span(entry_part, file_path).into();
                                }
                                Rule::some_ip_map_entry_value => {
                                    value = unquote(get_inner_str(entry_part));
                                }
                                _ => (),
                            }
                        }
                    }
                }
                group_event_part.field_name_mapping.insert(key.clone(), value);
                if let Some(s) = key_span {
                    group_event_part.field_name_mapping_span.insert(key, s.unwrap());
                }
            }
            Rule::some_ip_event_part_topic => {
                let inner = get_inner_pair(item.clone());
                group_event_part.topic = unquote(get_inner_str(item));
                group_event_part.topic_span = proto_span(inner, file_path);
            }
            Rule::some_ip_event_part_capacity => {
                let inner = get_inner_pair(item.clone());
                group_event_part.capacity = parse_string::<u32>(get_inner_str(item)).unwrap();
                group_event_part.capacity_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    group_event_part
}

pub fn convert_to_some_ip_sdv_rpc_interface(
    pair: Pair<Rule>,
    file_path: &str,
) -> sdv_type_mapping::RpcInterface {
    let mut rpc_interface = sdv_type_mapping::RpcInterface {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        if let Rule::some_ip_sdv_method = item.as_rule() {
            let inner = item.clone();
            rpc_interface.method.push(convert_to_some_ip_sdv_method(inner, file_path));
            rpc_interface.method_span.push(proto_span(item, file_path).unwrap());
        }
    }
    rpc_interface
}

pub fn convert_to_some_ip_sdv_method(pair: Pair<Rule>, file_path: &str) -> Method {
    let mut method = Method { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_sdv_method_procedure => {
                let inner = get_inner_pair(item.clone());
                method.procedure = unquote(get_inner_str(item));
                method.procedure_span = proto_span(inner, file_path);
            }
            Rule::some_ip_sdv_method_method_id => {
                let inner = get_inner_pair(item.clone());
                method.method_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                method.method_id_span = proto_span(inner, file_path);
            }
            Rule::request_payload_mapping => {
                let inner = get_inner_pair(item.clone());
                method.request_payload_mapping =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                method.request_payload_mapping_span = proto_span(item, file_path);
            }
            Rule::response_payload_mapping => {
                let inner = get_inner_pair(item.clone());
                method.response_payload_mapping =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                method.response_payload_mapping_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    method
}

pub fn convert_to_some_ip_offered_topic(
    pair: Pair<Rule>,
    file_path: &str,
) -> sdv_type_mapping::Topic {
    let mut topic =
        sdv_type_mapping::Topic { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_offered_topic_type_definition => {
                let inner = get_inner_pair(item.clone());
                topic.type_definition =
                    MessageField::some(convert_to_some_ip_struct(inner, file_path));
                topic.type_definition_span = proto_span(item, file_path);
            }
            Rule::some_ip_simple_mapping => {
                let inner = item.clone();
                topic.topic_mapping = Some(topic::Topic_mapping::SimpleMapping(
                    convert_to_some_ip_simple_topic_mapping(inner, file_path),
                ));
                topic.simple_mapping_span = proto_span(item, file_path);
            }
            Rule::some_ip_offered_topic_someip_ff_methods => {
                let inner = get_inner_pair(item.clone());
                topic.topic_mapping = Some(topic::Topic_mapping::SomeipFfMethods(
                    get_inner_str(item).parse().unwrap(),
                ));
                topic.someip_ff_methods_span = proto_span(inner, file_path);
            }
            Rule::some_ip_multipub_topic_info => {
                let inner = item.clone();
                topic.multipub_topic_info =
                    MessageField::some(convert_to_some_ip_multipub_topic_info(inner, file_path));
                topic.multipub_topic_info_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    topic
}

pub fn convert_to_some_ip_simple_topic_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> topic::SimpleTopicMapping {
    let mut mapping = topic::SimpleTopicMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_simple_mapping_group_id => {
                let inner = get_inner_pair(item.clone());
                mapping.group_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                mapping.group_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_simple_mapping_event_id => {
                let inner = get_inner_pair(item.clone());
                mapping.event_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                mapping.event_id_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_multipub_topic_info(
    pair: Pair<Rule>,
    file_path: &str,
) -> topic::MultipubTopicInfo {
    let mut info = topic::MultipubTopicInfo {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        if let Rule::some_ip_multipub_topic_info_someip_instance_id = item.as_rule() {
            let inner = get_inner_pair(item.clone());
            info.someip_instance_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
            info.someip_instance_id_span = proto_span(inner, file_path);
        }
    }
    info
}

pub fn convert_to_some_ip_service_unit_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> sdv_type_mapping::SdvServiceUnitMapping {
    let mut mapping = sdv_type_mapping::SdvServiceUnitMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_service_unit_mapping_service_unit_id => {
                let inner = get_inner_pair(item.clone()); // service_unit_id_block
                mapping.service_unit_id =
                    MessageField::some(convert_to_some_ip_service_unit_id(inner, file_path));
                mapping.service_unit_id_span = proto_span(item, file_path);
            }
            Rule::some_ip_service_unit_mapping_instance_id => {
                let inner = get_inner_pair(item.clone());
                mapping.instance_id = parse_string::<u32>(get_inner_str(item)).unwrap();
                mapping.instance_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_fire_and_forget_method => {
                let inner = item.clone();
                mapping.fire_and_forget_method.push(convert_to_some_ip_ff_method(inner, file_path));
                mapping.fire_and_forget_method_span.push(proto_span(item, file_path).unwrap());
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_service_unit_id(
    pair: Pair<Rule>,
    file_path: &str,
) -> sdv_type_mapping::ServiceUnitId {
    let mut id = sdv_type_mapping::ServiceUnitId {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_service_unit_id_vm_name => {
                let inner = get_inner_pair(item.clone());
                id.vm_name = Some(unquote(get_inner_str(item)));
                id.vm_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_service_unit_id_package_name => {
                let inner = get_inner_pair(item.clone());
                id.package_name = unquote(get_inner_str(item));
                id.package_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_service_unit_id_service_bundle_name => {
                let inner = get_inner_pair(item.clone());
                id.service_bundle_name = unquote(get_inner_str(item));
                id.service_bundle_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_service_unit_id_service_instance_name => {
                let inner = get_inner_pair(item.clone());
                id.service_instance_name = Some(unquote(get_inner_str(item)));
                id.service_instance_name_span = proto_span(inner, file_path);
            }
            Rule::some_ip_service_unit_id_service_unit_name => {
                let inner = get_inner_pair(item.clone());
                id.service_unit_name = unquote(get_inner_str(item));
                id.service_unit_name_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    id
}

pub fn convert_to_some_ip_ff_method(
    pair: Pair<Rule>,
    file_path: &str,
) -> sdv_type_mapping::SomeIpFFMethod {
    let mut method = sdv_type_mapping::SomeIpFFMethod {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_fire_and_forget_method_service_id => {
                let inner = get_inner_pair(item.clone());
                method.service_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                method.service_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_fire_and_forget_method_instance_id => {
                let inner = get_inner_pair(item.clone());
                method.instance_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                method.instance_id_span = proto_span(inner, file_path);
            }
            Rule::some_ip_fire_and_forget_method_version_major => {
                let inner = get_inner_pair(item.clone());
                method.version_major = parse_string::<u8>(get_inner_str(item)).unwrap() as u32;
                method.version_major_span = proto_span(inner, file_path);
            }
            Rule::some_ip_fire_and_forget_method_version_minor => {
                let inner = get_inner_pair(item.clone());
                method.version_minor = parse_string::<u32>(get_inner_str(item)).unwrap();
                method.version_minor_span = proto_span(inner, file_path);
            }
            Rule::some_ip_fire_and_forget_method_method_id => {
                let inner = get_inner_pair(item.clone());
                method.method_id = parse_string::<u16>(get_inner_str(item)).unwrap() as u32;
                method.method_id_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    method
}

pub fn convert_to_some_ip_service_oriented_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> ServiceOrientedMapping {
    let mut som =
        ServiceOrientedMapping { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_rpc_interface_mapping => {
                let inner = item.clone();
                som.soa_mapping_type =
                    Some(service_oriented_mapping::Soa_mapping_type::RpcInterfaceMapping(
                        convert_to_some_ip_rpc_interface_mapping(inner, file_path),
                    ));
                som.rpc_interface_mapping_span = proto_span(item, file_path);
            }
            Rule::some_ip_publication_mapping => {
                let inner = item.clone();
                som.soa_mapping_type =
                    Some(service_oriented_mapping::Soa_mapping_type::PublicationMapping(
                        convert_to_some_ip_publication_mapping(inner, file_path),
                    ));
                som.publication_mapping_span = proto_span(item, file_path);
            }
            _ => (),
        }
    }
    som
}

pub fn convert_to_some_ip_rpc_interface_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> RpcInterfaceMapping {
    let mut mapping =
        RpcInterfaceMapping { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_rpc_channel_mapping => {
                let inner = item.clone();
                mapping
                    .rpc_channel_mapping
                    .push(convert_to_some_ip_rpc_channel_mapping(inner, file_path));
                mapping.rpc_channel_mapping_span.push(proto_span(item, file_path).unwrap());
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_rpc_channel_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> rpc_interface_mapping::RpcChannelMapping {
    let mut mapping = rpc_interface_mapping::RpcChannelMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_rpc_channel_mapping_channel => {
                let inner = get_inner_pair(item.clone());
                mapping.channel = unquote(get_inner_str(item));
                mapping.channel_span = proto_span(inner, file_path);
            }
            Rule::some_ip_rpc_channel_mapping_instance_id => {
                let inner = get_inner_pair(item.clone());
                mapping.instance_id = parse_string::<u32>(get_inner_str(item)).unwrap();
                mapping.instance_id_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_publication_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> PublicationMapping {
    let mut mapping =
        PublicationMapping { span: proto_span(pair.clone(), file_path), ..Default::default() };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_topic_mapping => {
                let inner = item.clone();
                mapping.topic_mapping.push(convert_to_some_ip_topic_mapping(inner, file_path));
                mapping.topic_mapping_span.push(proto_span(item, file_path).unwrap());
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_topic_mapping(
    pair: Pair<Rule>,
    file_path: &str,
) -> publication_mapping::TopicMapping {
    let mut mapping = publication_mapping::TopicMapping {
        span: proto_span(pair.clone(), file_path),
        ..Default::default()
    };
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_topic_mapping_topic => {
                let inner = get_inner_pair(item.clone());
                mapping.topic = unquote(get_inner_str(item));
                mapping.topic_span = proto_span(inner, file_path);
            }
            Rule::some_ip_topic_mapping_instance_id => {
                let inner = get_inner_pair(item.clone());
                mapping.instance_id = parse_string::<u32>(get_inner_str(item)).unwrap();
                mapping.instance_id_span = proto_span(inner, file_path);
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_some_ip_some_ip_type(
    pair: Pair<Rule>,
    file_path: &str,
) -> someip_type::SomeIpType {
    let mut someip_type = someip_type::SomeIpType::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_type_someip_primitive => {
                let _inner = get_inner_pair(item.clone());
                someip_type.someip_type =
                    Some(some_ip_type::Someip_type::SomeipPrimitive(match get_inner_str(item) {
                        "BOOL" => some_ip_type::SomeIpPrimitive::BOOL.into(),
                        "INT8" => some_ip_type::SomeIpPrimitive::INT8.into(),
                        "INT16" => some_ip_type::SomeIpPrimitive::INT16.into(),
                        "INT32" => some_ip_type::SomeIpPrimitive::INT32.into(),
                        "INT64" => some_ip_type::SomeIpPrimitive::INT64.into(),
                        "UINT8" => some_ip_type::SomeIpPrimitive::UINT8.into(),
                        "UINT16" => some_ip_type::SomeIpPrimitive::UINT16.into(),
                        "UINT32" => some_ip_type::SomeIpPrimitive::UINT32.into(),
                        "UINT64" => some_ip_type::SomeIpPrimitive::UINT64.into(),
                        "FLOAT32" => some_ip_type::SomeIpPrimitive::FLOAT32.into(),
                        "FLOAT64" => some_ip_type::SomeIpPrimitive::FLOAT64.into(),
                        _ => some_ip_type::SomeIpPrimitive::UNKNOWN_TYPE.into(),
                    }));
            }
            Rule::some_ip_string_body => {
                let inner = item.clone();
                someip_type.someip_type = Some(some_ip_type::Someip_type::SomeipString(
                    convert_to_some_ip_string(inner, file_path),
                ));
            }
            Rule::some_ip_array_body => {
                let inner = item.clone();
                someip_type.someip_type = Some(some_ip_type::Someip_type::SomeipArray(
                    convert_to_some_ip_array(inner, file_path),
                ));
            }
            Rule::some_ip_struct_body => {
                let inner = item.clone();
                someip_type.someip_type = Some(some_ip_type::Someip_type::SomeipStruct(
                    convert_to_some_ip_struct(inner, file_path),
                ));
            }
            _ => (),
        }
    }
    someip_type
}

pub fn convert_str_to_length_field_size(
    s: &str,
) -> ::protobuf::EnumOrUnknown<some_ip_type::LengthFieldSize> {
    match s {
        "ONE" => some_ip_type::LengthFieldSize::ONE.into(),
        "TWO" => some_ip_type::LengthFieldSize::TWO.into(),
        "FOUR" => some_ip_type::LengthFieldSize::FOUR.into(),
        "ZERO" => some_ip_type::LengthFieldSize::ZERO.into(),
        _ => some_ip_type::LengthFieldSize::UNSPECIFIED_SIZE.into(),
    }
}

pub fn convert_to_some_ip_string(pair: Pair<Rule>, _file_path: &str) -> some_ip_type::SomeIpString {
    let mut s = some_ip_type::SomeIpString::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_string_body_fixed_length => {
                s.string_length = Some(some_ip_string::String_length::FixedLength(
                    parse_string::<u32>(get_inner_str(item)).unwrap(),
                ));
            }
            Rule::some_ip_string_body_length_field_size => {
                s.string_length = Some(some_ip_string::String_length::LengthFieldSize(
                    convert_str_to_length_field_size(get_inner_str(item)),
                ));
            }
            Rule::some_ip_string_body_string_encoding => {
                s.string_encoding = match get_inner_str(item) {
                    "UTF_8" => some_ip_string::StringEncoding::UTF_8.into(),
                    "UTF_16_BE" => some_ip_string::StringEncoding::UTF_16_BE.into(),
                    "UTF_16_LE" => some_ip_string::StringEncoding::UTF_16_LE.into(),
                    _ => some_ip_string::StringEncoding::UNSPECIFIED_ENCODING.into(),
                };
            }
            _ => (),
        }
    }
    s
}

pub fn convert_to_some_ip_array(pair: Pair<Rule>, file_path: &str) -> some_ip_type::SomeIpArray {
    let mut a = some_ip_type::SomeIpArray::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_array_body_fixed_length => {
                a.array_length = Some(some_ip_array::Array_length::FixedLength(
                    parse_string::<u32>(get_inner_str(item)).unwrap(),
                ));
            }
            Rule::some_ip_array_body_length_field_size => {
                a.array_length = Some(some_ip_array::Array_length::LengthFieldSize(
                    convert_str_to_length_field_size(get_inner_str(item)),
                ));
            }
            Rule::some_ip_array_type => {
                let inner = get_inner_pair(item.clone()); // some_ip_type
                a.array_type =
                    MessageField::some(convert_to_some_ip_some_ip_type(inner, file_path));
            }
            _ => (),
        }
    }
    a
}

pub fn convert_to_some_ip_struct(pair: Pair<Rule>, file_path: &str) -> some_ip_type::SomeIpStruct {
    let mut s = some_ip_type::SomeIpStruct::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_struct_body_length_field_size => {
                s.length_field_size = Some(convert_str_to_length_field_size(get_inner_str(item)));
            }
            Rule::some_ip_struct_field => {
                let inner = item.clone();
                s.field.push(convert_to_some_ip_struct_field(inner, file_path));
            }
            _ => (),
        }
    }
    s
}

pub fn convert_to_some_ip_struct_field(
    pair: Pair<Rule>,
    file_path: &str,
) -> some_ip_struct::SomeIpStructField {
    let mut f = some_ip_struct::SomeIpStructField::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::some_ip_struct_field_data_tag => {
                f.data_tag = Some(parse_string::<u32>(get_inner_str(item)).unwrap());
            }
            Rule::some_ip_struct_field_field_name => {
                f.field_name = unquote(get_inner_str(item));
            }
            Rule::some_ip_struct_field_type => {
                let inner = get_inner_pair(item.clone()); // some_ip_type
                f.field_type =
                    MessageField::some(convert_to_some_ip_some_ip_type(inner, file_path));
            }
            _ => (),
        }
    }
    f
}

pub fn convert_to_vhal_mapping(pair: Pair<Rule>, file_path: &str) -> VhalMapping {
    let mut mapping = VhalMapping::default();
    let body = pair
        .into_inner()
        .find(|inner_pair| inner_pair.as_rule() == Rule::vhal_mapping_body)
        .unwrap_or_else(|| panic!("Expected Rule::vhal_mapping_body inside vhal_mapping."));

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::vhal_vehicle_property => {
                mapping.property =
                    MessageField::some(convert_to_vhal_vehicle_property(item, file_path));
            }
            Rule::vhal_data_source => {
                mapping.source = MessageField::some(convert_to_vhal_data_source(item, file_path));
            }
            Rule::vhal_area_mapping => {
                mapping.area_mapping.push(convert_to_vhal_area_mapping(item, file_path));
            }
            _ => (),
        }
    }
    mapping
}

pub fn convert_to_vhal_vehicle_property(pair: Pair<Rule>, file_path: &str) -> VhalVehicleProperty {
    let mut property = VhalVehicleProperty::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_vehicle_prop_config => {
                let inner = get_inner_pair(item); // prop_config_body
                property.prop_config =
                    MessageField::some(convert_to_vhal_vehicle_prop_config(inner, file_path));
            }
            Rule::vhal_vehicle_default_value => {
                let inner = get_inner_pair(item); // default_value_body
                property.default_value =
                    MessageField::some(convert_to_vhal_default_value(inner, file_path));
            }
            Rule::vhal_vehicle_default_area_config => {
                let inner = get_inner_pair(item); // area_config_body
                property.default_area_config =
                    MessageField::some(convert_to_vhal_vehicle_area_config(inner, file_path));
            }
            _ => (),
        }
    }
    property
}

pub fn convert_to_vhal_data_source(pair: Pair<Rule>, _file_path: &str) -> VhalDataSource {
    let mut data_source = VhalDataSource::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_data_source_unit_type => {
                data_source.unit_type = unquote(get_inner_str(item));
            }
            Rule::vhal_data_source_action => {
                data_source.action = match get_inner_str(item) {
                    "ACTION_SUBSCRIBE" => VhalSdvAction::ACTION_SUBSCRIBE.into(),
                    "ACTION_PUBLISH" => VhalSdvAction::ACTION_PUBLISH.into(),
                    _ => VhalSdvAction::ACTION_UNSPECIFIED.into(),
                };
            }
            Rule::vhal_data_source_field_path => {
                data_source.field_path = unquote(get_inner_str(item));
            }
            Rule::vhal_data_source_status_field_path => {
                data_source.status_field_path = unquote(get_inner_str(item));
            }
            _ => (),
        }
    }
    data_source
}

pub fn convert_to_vhal_area_mapping(pair: Pair<Rule>, file_path: &str) -> VhalAreaMapping {
    let mut area_mapping = VhalAreaMapping::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_area_mapping_area => {
                let inner = get_inner_pair(item); // property_area_body
                area_mapping.area =
                    MessageField::some(convert_to_vhal_property_area(inner, file_path));
            }
            Rule::vhal_area_mapping_publication_instance => {
                area_mapping.publication_instance = unquote(get_inner_str(item));
            }
            _ => (),
        }
    }
    area_mapping
}

pub fn convert_to_vhal_property_area(pair: Pair<Rule>, file_path: &str) -> VhalPropertyArea {
    let mut property_area = VhalPropertyArea::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_property_area_config => {
                let inner = get_inner_pair(item); // area_config_body
                property_area.area_config =
                    MessageField::some(convert_to_vhal_vehicle_area_config(inner, file_path));
            }
            Rule::vhal_property_area_default_value => {
                let inner = get_inner_pair(item); // default_value_body
                property_area.default_value =
                    MessageField::some(convert_to_vhal_default_value(inner, file_path));
            }
            _ => (),
        }
    }
    property_area
}

pub fn convert_to_vhal_default_value(pair: Pair<Rule>, _file_path: &str) -> VhalDefaultValue {
    let mut default_value = VhalDefaultValue::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_default_value_string_value => {
                default_value.string_value = unquote(get_inner_str(item));
            }
            Rule::vhal_default_value_bytes_value => {
                default_value.bytes_value = unquote(get_inner_str(item)).into_bytes();
            }
            Rule::vhal_default_value_int32_values => {
                default_value.int32_values.push(parse_string::<i32>(get_inner_str(item)).unwrap());
            }
            Rule::vhal_default_value_int64_values => {
                default_value.int64_values.push(parse_string::<i64>(get_inner_str(item)).unwrap());
            }
            Rule::vhal_default_value_float_values => {
                default_value.float_values.push(parse_string::<f32>(get_inner_str(item)).unwrap());
            }
            _ => (),
        }
    }
    default_value
}

pub fn convert_to_vhal_vehicle_prop_config(pair: Pair<Rule>, file_path: &str) -> VehiclePropConfig {
    let mut prop_config = VehiclePropConfig::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_vehicle_prop_config_prop => {
                prop_config.prop = parse_string::<i32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_prop_config_access => {
                prop_config.access = match get_inner_str(item) {
                    "READ" => VehiclePropertyAccess::READ.into(),
                    "WRITE" => VehiclePropertyAccess::WRITE.into(),
                    "READ_WRITE" => VehiclePropertyAccess::READ_WRITE.into(),
                    _ => VehiclePropertyAccess::NONE.into(),
                };
            }
            Rule::vhal_vehicle_prop_config_change_mode => {
                prop_config.change_mode = match get_inner_str(item) {
                    "ON_CHANGE" => VehiclePropertyChangeMode::ON_CHANGE.into(),
                    "CONTINUOUS" => VehiclePropertyChangeMode::CONTINUOUS.into(),
                    _ => VehiclePropertyChangeMode::STATIC.into(),
                };
            }
            Rule::vhal_vehicle_prop_config_area_configs => {
                let inner = get_inner_pair(item); // area_config_body
                prop_config
                    .area_configs
                    .push(convert_to_vhal_vehicle_area_config(inner, file_path));
            }
            Rule::vhal_vehicle_prop_config_config_array => {
                prop_config.config_array.push(parse_string::<i32>(get_inner_str(item)).unwrap());
            }
            Rule::vhal_vehicle_prop_config_config_string => {
                prop_config.config_string = unquote(get_inner_str(item)).into_bytes();
            }
            Rule::vhal_vehicle_prop_config_min_sample_rate => {
                prop_config.min_sample_rate = parse_string::<f32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_prop_config_max_sample_rate => {
                prop_config.max_sample_rate = parse_string::<f32>(get_inner_str(item)).unwrap();
            }
            _ => (),
        }
    }
    prop_config
}

pub fn convert_to_vhal_vehicle_area_config(
    pair: Pair<Rule>,
    _file_path: &str,
) -> VehicleAreaConfig {
    let mut area_config = VehicleAreaConfig::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::vhal_vehicle_area_config_area_id => {
                area_config.area_id = parse_string::<i32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_min_int32_value => {
                area_config.min_int32_value = parse_string::<i32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_max_int32_value => {
                area_config.max_int32_value = parse_string::<i32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_min_int64_value => {
                area_config.min_int64_value = parse_string::<i64>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_max_int64_value => {
                area_config.max_int64_value = parse_string::<i64>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_min_float_value => {
                area_config.min_float_value = parse_string::<f32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_max_float_value => {
                area_config.max_float_value = parse_string::<f32>(get_inner_str(item)).unwrap();
            }
            Rule::vhal_vehicle_area_config_supported_enum_values => {
                area_config
                    .supported_enum_values
                    .push(parse_string::<i64>(get_inner_str(item)).unwrap());
            }
            Rule::vhal_vehicle_area_config_access => {
                area_config.access = match get_inner_str(item) {
                    "READ" => VehiclePropertyAccess::READ.into(),
                    "WRITE" => VehiclePropertyAccess::WRITE.into(),
                    "READ_WRITE" => VehiclePropertyAccess::READ_WRITE.into(),
                    _ => VehiclePropertyAccess::NONE.into(),
                };
            }
            Rule::vhal_vehicle_area_config_support_variable_update_rate => {
                area_config.support_variable_update_rate = get_inner_str(item).parse().unwrap();
            }
            _ => (),
        }
    }
    area_config
}

#[cfg(test)]
mod test {
    use crate::converter;
    use crate::pest_parser::{Rule, VsidlParser};
    use pest::Parser;

    const FILE_PATH: &str = "test.vsidl";

    #[test]
    fn test_vsidl_entry_is_correctly_converted() {
        // GIVEN a VSIDL file with all AST node types
        let vsidl_content =
            std::fs::read_to_string("src/tests_vsidl_v1/vsidl/all_ast_node_types.vsidl").unwrap();

        // WHEN parsing the file
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content)
            .unwrap_or_else(|e| panic!("failed to parse vsidl file: {}", e));
        let vsidl_entry =
            converter::convert_to_vsidl_entry(parsed_file.into_iter().next().unwrap(), FILE_PATH);

        // THEN the package is correct
        assert_eq!(vsidl_entry.package, "sdv.test");

        // THEN the service bundle is correct
        assert_eq!(vsidl_entry.service_bundle.len(), 1);
        let service_bundle = &vsidl_entry.service_bundle[0];
        assert_eq!(service_bundle.name, "AllAstNodeTypes");

        // THEN the publishers are correct
        assert_eq!(service_bundle.publisher.len(), 2);
        assert_eq!(service_bundle.publisher[0].message, "Pressure");
        assert_eq!(service_bundle.publisher[0].topic, vec!["one"]);
        assert_eq!(service_bundle.publisher[0].channel, vec!["ONE"]);
        assert_eq!(service_bundle.publisher[1].message, "Pressure");
        assert_eq!(service_bundle.publisher[1].topic, vec!["two"]);
        assert_eq!(service_bundle.publisher[1].channel, vec!["TWO"]);

        // THEN the subscribers are correct
        assert_eq!(service_bundle.subscriber.len(), 1);
        assert_eq!(service_bundle.subscriber[0].message, "Pressure");
        assert_eq!(service_bundle.subscriber[0].topic, vec!["two"]);
        assert_eq!(service_bundle.subscriber[0].channel, vec!["TWO"]);

        // THEN the servers are correct
        assert_eq!(service_bundle.server.len(), 1);
        assert_eq!(service_bundle.server[0].service, "DummyRpc");

        // THEN the clients are correct
        assert_eq!(service_bundle.client.len(), 1);
        assert_eq!(service_bundle.client[0].service, "DummyRpc");

        // THEN the build configuration is correct
        assert!(service_bundle.build_cfg.is_some());
        let build_cfg = service_bundle.build_cfg.as_ref().unwrap();
        assert_eq!(build_cfg.target_name, Some("my_custom_target_name".to_string()));
    }

    #[test]
    fn test_diagnostics_is_correctly_converted() {
        // GIVEN a VSIDL file with diagnostics
        let vsidl_content =
            std::fs::read_to_string("src/tests_vsidl_v1/vsidl/all_ast_node_types.vsidl").unwrap();

        // WHEN parsing the file
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content)
            .unwrap_or_else(|e| panic!("failed to parse vsidl file: {}", e));
        let vsidl_entry =
            converter::convert_to_vsidl_entry(parsed_file.into_iter().next().unwrap(), FILE_PATH);

        // THEN the diagnostics is correct
        assert_eq!(vsidl_entry.service_bundle.len(), 1);
        let service_bundle = &vsidl_entry.service_bundle[0];
        assert_eq!(service_bundle.name, "AllAstNodeTypes");
        assert!(service_bundle.diagnostics_declaration.clone().is_some());
        let diagnostics_declaration = &service_bundle.diagnostics_declaration.clone().unwrap();

        assert_eq!(diagnostics_declaration.data_item.len(), 1);
        let data_item = diagnostics_declaration.data_item[0].clone();
        assert_eq!(data_item.id, "data_item_id1");
        assert_eq!(data_item.message_name, "sdv.test.diagnostics.DataItemOne");
        assert!(data_item.is_writable);

        assert_eq!(diagnostics_declaration.io_control.len(), 1);
        let io_control = diagnostics_declaration.io_control[0].clone();
        assert_eq!(io_control.id, "io_control_data1");
        assert_eq!(io_control.message_name, "sdv.test.diagnostics.IoControllableData");

        assert_eq!(diagnostics_declaration.event.len(), 1);
        let event = diagnostics_declaration.event[0].clone();
        assert_eq!(event.id, "event1");
        assert_eq!(event.extended_data_message_name, "sdv.test.diagnostics.FirstEventExt");
        assert_eq!(event.fault_listener_mask, 0x3);

        assert_eq!(diagnostics_declaration.routine.len(), 1);
        let routine = diagnostics_declaration.routine[0].clone();
        assert_eq!(routine.id, "routine1");
        assert!(routine.start.is_some());
        let start = &routine.start.clone().unwrap();
        assert_eq!(start.request, "sdv.test.diagnostics.RoutineRequestParameter");
        assert_eq!(start.response, "sdv.test.diagnostics.RoutineResponseParameter");

        assert!(routine.stop.is_some());
        let stop = &routine.stop.clone().unwrap();
        assert_eq!(stop.response, "Temperature");

        assert!(routine.result.is_some());
        let result = &routine.result.clone().unwrap();
        assert_eq!(result.response, "Temperature");
    }

    #[test]
    fn test_some_ip_mapping_is_correctly_converted() {
        // GIVEN a VSIDL file with some_ip_mapping
        let vsidl_content =
            std::fs::read_to_string("src/tests_vsidl_v1/vsidl/all_ast_node_types.vsidl").unwrap();

        // WHEN parsing the file
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content)
            .unwrap_or_else(|e| panic!("failed to parse vsidl file: {}", e));
        let vsidl_entry =
            converter::convert_to_vsidl_entry(parsed_file.into_iter().next().unwrap(), FILE_PATH);

        // THEN the some_ip_mapping is correct
        assert_eq!(vsidl_entry.some_ip_mapping.len(), 4);
        let some_ip_mapping = &vsidl_entry.some_ip_mapping[0];
        let service_mapping = some_ip_mapping.mapping_type.as_ref().unwrap();
        match service_mapping {
            vsidl_v1::someip::some_ip_mapping::Mapping_type::SomeipServiceMapping(mapping) => {
                assert_eq!(mapping.someip_service_id, 0x3200);
                assert_eq!(mapping.version_major, 0x1);
                assert_eq!(mapping.version_minor, 0x0);

                assert_eq!(mapping.instance.len(), 1);
                assert_eq!(mapping.instance[0].instance_id, 0x1000);
                assert_eq!(
                    mapping.instance[0].sdv_service_unit_parameterization.get(&0),
                    Some(&"first_service_unit".to_string())
                );
                assert_eq!(
                    mapping.instance[0].sdv_service_unit_parameterization.get(&1),
                    Some(&"second_service_unit".to_string())
                );

                assert_eq!(mapping.instance_impl.len(), 4);

                // offered_event 1 (SimpleEvent)
                let instance_impl_0 = &mapping.instance_impl[0];
                if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::Interface_type::OfferedEvent(event)) = &instance_impl_0.interface_type {
                    if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::Event_type::SimpleEvent(simple_event)) = &event.event_type {
                        assert_eq!(simple_event.topic, "pressure-topic");
                        assert_eq!(simple_event.capacity, 100);
                    } else {
                        panic!("Expected SimpleEvent");
                    }
                } else {
                    panic!("Expected OfferedEvent");
                }

                // offered_event 2 (SimpleEvent)
                let instance_impl_1 = &mapping.instance_impl[1];
                if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::Interface_type::OfferedEvent(event)) = &instance_impl_1.interface_type {
                    if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::Event_type::SimpleEvent(simple_event)) = &event.event_type {
                        assert_eq!(simple_event.topic, "temperature-topic");
                        assert_eq!(simple_event.capacity, 50);
                    } else {
                        panic!("Expected SimpleEvent");
                    }
                } else {
                    panic!("Expected OfferedEvent");
                }

                // offered_event 3 (GroupEvent)
                let instance_impl_2 = &mapping.instance_impl[2];
                if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::Interface_type::OfferedEvent(event)) = &instance_impl_2.interface_type {
                    if let Some(vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::Event_type::GroupEvent(group_event)) = &event.event_type {
                        assert_eq!(group_event.event_part.len(), 2);
                        assert_eq!(group_event.event_part[0].topic, "bool-part-topic");
                        assert_eq!(group_event.event_part[0].capacity, 10);
                        assert_eq!(group_event.event_part[1].topic, "int-part-topic");
                        assert_eq!(group_event.event_part[1].capacity, 20);
                    } else {
                        panic!("Expected GroupEvent");
                    }
                } else {
                    panic!("Expected OfferedEvent");
                }

                let instance_impl = &mapping.instance_impl[3]; // The offered_interface is the 4th item
                let interface_type = instance_impl.interface_type.as_ref().unwrap();
                match interface_type {
                    vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::Interface_type::OfferedInterface(rpc_interface) => {
                        assert_eq!(rpc_interface.sdv_unit_type.as_ref().unwrap().package_name, "com.android.sdv.someip");
                        assert_eq!(rpc_interface.sdv_unit_type.as_ref().unwrap().service_bundle_name, "SomeIpBroker");
                        assert_eq!(rpc_interface.sdv_unit_type.as_ref().unwrap().type_name, "SomeIpInterface");
                        assert_eq!(rpc_interface.service_unit, 1);
                        assert_eq!(rpc_interface.channel, "rpc-channel-1");
                        assert_eq!(rpc_interface.method.len(), 1);
                        assert_eq!(rpc_interface.method[0].procedure, "DoubleNum");
                        assert_eq!(rpc_interface.method[0].method_id, 0x1500);
                    },
                    _ => panic!("Expected OfferedInterface"),
                }
            }
            _ => panic!("Expected SomeipServiceMapping"),
        }

        let some_ip_mapping_2 = &vsidl_entry.some_ip_mapping[1];
        let service_mapping_2 = some_ip_mapping_2.mapping_type.as_ref().unwrap();
        match service_mapping_2 {
            vsidl_v1::someip::some_ip_mapping::Mapping_type::SdvTypeMapping(mapping) => {
                assert_eq!(
                    mapping.unit_type.as_ref().unwrap().package_name,
                    "com.android.sdv.someip.sample"
                );
                assert_eq!(
                    mapping.unit_type.as_ref().unwrap().service_bundle_name,
                    "ServiceBundleFoo"
                );
                assert_eq!(mapping.unit_type.as_ref().unwrap().type_name, "SdvInterface");
                assert_eq!(mapping.someip_service_id, 0x48);
                assert_eq!(mapping.version_major, 0x1);
                assert_eq!(mapping.version_minor, 0x0);
                assert_eq!(mapping.service_unit_mapping.len(), 1);
                assert_eq!(mapping.service_unit_mapping[0].instance_id, 0x0001);
            }
            _ => panic!("Expected SdvTypeMapping"),
        }

        let some_ip_mapping_3 = &vsidl_entry.some_ip_mapping[2];
        let service_mapping_3 = some_ip_mapping_3.mapping_type.as_ref().unwrap();
        match service_mapping_3 {
            vsidl_v1::someip::some_ip_mapping::Mapping_type::SdvTypeMapping(mapping) => {
                assert_eq!(mapping.someip_service_id, 0x48);
                // Check service_oriented_mapping -> RpcInterfaceMapping
                let som = mapping.service_oriented_mapping.as_ref().unwrap();
                match som.soa_mapping_type.as_ref().unwrap() {
                    vsidl_v1::someip::some_ip_mapping::sdv_type_mapping::service_oriented_mapping::Soa_mapping_type::RpcInterfaceMapping(rpc_mapping) => {
                        assert_eq!(rpc_mapping.rpc_channel_mapping.len(), 2);
                        assert_eq!(rpc_mapping.rpc_channel_mapping[0].channel, "rpc-channel-1");
                        assert_eq!(rpc_mapping.rpc_channel_mapping[0].instance_id, 10);
                        assert_eq!(rpc_mapping.rpc_channel_mapping[1].channel, "rpc-channel-2");
                        assert_eq!(rpc_mapping.rpc_channel_mapping[1].instance_id, 11);
                    }
                    _ => panic!("Expected RpcInterfaceMapping"),
                }
            }
            _ => panic!("Expected SdvTypeMapping for mapping 3"),
        }

        let some_ip_mapping_4 = &vsidl_entry.some_ip_mapping[3];
        let service_mapping_4 = some_ip_mapping_4.mapping_type.as_ref().unwrap();
        match service_mapping_4 {
            vsidl_v1::someip::some_ip_mapping::Mapping_type::SdvTypeMapping(mapping) => {
                assert_eq!(mapping.someip_service_id, 0x48);
                // Check service_oriented_mapping -> PublicationMapping
                let som = mapping.service_oriented_mapping.as_ref().unwrap();
                match som.soa_mapping_type.as_ref().unwrap() {
                    vsidl_v1::someip::some_ip_mapping::sdv_type_mapping::service_oriented_mapping::Soa_mapping_type::PublicationMapping(pub_mapping) => {
                        assert_eq!(pub_mapping.topic_mapping.len(), 2);
                        assert_eq!(pub_mapping.topic_mapping[0].topic, "pressure-topic");
                        assert_eq!(pub_mapping.topic_mapping[0].instance_id, 20);
                        assert_eq!(pub_mapping.topic_mapping[1].topic, "temperature-topic");
                        assert_eq!(pub_mapping.topic_mapping[1].instance_id, 21);
                    }
                    _ => panic!("Expected PublicationMapping"),
                }
            }
            _ => panic!("Expected SdvTypeMapping for mapping 4"),
        }
    }

    #[test]
    fn test_vhal_mapping_is_correctly_converted() {
        // GIVEN a VSIDL file with vhal_mapping
        let vsidl_content =
            std::fs::read_to_string("src/tests_vsidl_v1/vsidl/all_ast_node_types.vsidl").unwrap();

        // WHEN parsing the file
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content)
            .unwrap_or_else(|e| panic!("failed to parse vsidl file: {}", e));
        let vsidl_entry =
            converter::convert_to_vsidl_entry(parsed_file.into_iter().next().unwrap(), FILE_PATH);

        // THEN the vhal_mapping is correct
        assert_eq!(vsidl_entry.vhal_mapping.len(), 1);
        let vhal_mapping = &vsidl_entry.vhal_mapping[0];

        // Property
        assert!(vhal_mapping.property.is_some());
        let property = vhal_mapping.property.as_ref().unwrap();
        assert!(property.prop_config.is_some());
        let prop_config = property.prop_config.as_ref().unwrap();
        assert_eq!(prop_config.prop, 392168201);

        // Source
        assert!(vhal_mapping.source.is_some());
        let source = vhal_mapping.source.as_ref().unwrap();
        assert_eq!(source.unit_type, "sdv.test.TirePressure");
        assert_eq!(source.field_path, "pressure");

        // Area mapping
        assert_eq!(vhal_mapping.area_mapping.len(), 4);
        let area_mapping = &vhal_mapping.area_mapping[0];
        assert_eq!(area_mapping.publication_instance, "FRONT_LEFT");
    }

    #[test]
    fn test_pest_vs_protobuf_parsing() {
        // GIVEN a VSIDL file with all AST node types
        let vsidl_content =
            std::fs::read_to_string("src/tests_vsidl_v1/vsidl/all_ast_node_types.vsidl").unwrap();

        // WHEN parsing using Protobuf (old way)
        let vsidl_entry_proto =
            protobuf::text_format::parse_from_str::<vsidl_v1::syntax::VsidlEntry>(&vsidl_content)
                .unwrap();

        // WHEN parsing using Pest (new way)
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content)
            .unwrap_or_else(|e| panic!("failed to parse vsidl file: {}", e));
        let mut vsidl_entry_pest = converter::convert_to_vsidl_entry(
            parsed_file.into_iter().next().unwrap(),
            "all_ast_node_types.vsidl",
        );

        // THEN they should be equal after clearing spans
        vsidl_entry_pest.span.clear();
        vsidl_entry_pest.package_span.clear();
        for bundle in &mut vsidl_entry_pest.service_bundle {
            bundle.span.clear();
            bundle.name_span.clear();
            bundle.register_reflection_metadata_span.clear();
            for p in &mut bundle.publisher {
                p.span.clear();
                p.message_span.clear();
                p.service_unit_name_span.clear();
                p.capacity_span.clear();
                p.topic_span.clear();
                p.channel_span.clear();
            }
            for s in &mut bundle.subscriber {
                s.span.clear();
                s.message_span.clear();
                s.topic_span.clear();
                s.channel_span.clear();
            }
            for s in &mut bundle.server {
                s.span.clear();
                s.service_span.clear();
                s.service_unit_name_span.clear();
                s.channel_span.clear();
            }
            for c in &mut bundle.client {
                c.span.clear();
                c.service_span.clear();
                c.channel_span.clear();
            }
            if let Some(cfg) = bundle.build_cfg.as_mut() {
                cfg.span.clear();
                cfg.target_name_span.clear();
                cfg.skip_codegen_span.clear();
                cfg.special_fields.clear();
            }
        }

        for mapping in &mut vsidl_entry_pest.some_ip_mapping {
            mapping.span.clear();
            mapping.someip_service_mapping_span.clear();
            mapping.sdv_type_mapping_span.clear();
            mapping.special_fields.clear();
            if mapping.has_someip_service_mapping() {
                let s = mapping.mut_someip_service_mapping();
                s.span.clear();
                s.someip_service_id_span.clear();
                s.version_major_span.clear();
                s.version_minor_span.clear();
                s.instance_span.clear();
                s.instance_impl_span.clear();
                s.sd_behavior_span.clear();
                s.special_fields.clear();
                for i in &mut s.instance {
                    i.span.clear();
                    i.instance_id_span.clear();
                    i.sdv_service_unit_parameterization_span.clear();
                    i.special_fields.clear();
                }
                for i in &mut s.instance_impl {
                    i.span.clear();
                    i.offered_interface_span.clear();
                    i.offered_event_span.clear();
                    i.special_fields.clear();
                    if i.has_offered_interface() {
                        let o = i.mut_offered_interface();
                        o.span.clear();
                        o.sdv_unit_type_span.clear();
                        o.service_unit_span.clear();
                        o.method_span.clear();
                        o.channel_span.clear();
                        o.special_fields.clear();
                        if let Some(u) = o.sdv_unit_type.as_mut() {
                            u.span.clear();
                            u.package_name_span.clear();
                            u.service_bundle_name_span.clear();
                            u.type_name_span.clear();
                            u.special_fields.clear();
                        }
                        for m in &mut o.method {
                            m.span.clear();
                            m.procedure_span.clear();
                            m.method_id_span.clear();
                            m.request_payload_mapping_span.clear();
                            m.response_payload_mapping_span.clear();
                            m.special_fields.clear();
                        }
                    }
                    if i.has_offered_event() {
                        let e = i.mut_offered_event();
                        e.span.clear();
                        e.group_id_span.clear();
                        e.event_id_span.clear();
                        e.simple_event_span.clear();
                        e.group_event_span.clear();
                        e.special_fields.clear();
                        if e.has_simple_event() {
                            let s = e.mut_simple_event();
                            s.span.clear();
                            s.sdv_unit_type_span.clear();
                            s.type_definition_span.clear();
                            s.service_unit_span.clear();
                            s.message_translation_mode_span.clear();
                            s.topic_span.clear();
                            s.capacity_span.clear();
                            s.special_fields.clear();
                            if let Some(u) = s.sdv_unit_type.as_mut() {
                                u.span.clear();
                                u.package_name_span.clear();
                                u.service_bundle_name_span.clear();
                                u.type_name_span.clear();
                                u.special_fields.clear();
                            }
                            if let Some(t) = s.type_definition.as_mut() {
                                t.special_fields.clear();
                                for f in &mut t.field {
                                    f.special_fields.clear();
                                }
                            }
                        }
                        if e.has_group_event() {
                            let g = e.mut_group_event();
                            g.span.clear();
                            g.type_definition_span.clear();
                            g.event_part_span.clear();
                            g.special_fields.clear();
                            if let Some(t) = g.type_definition.as_mut() {
                                t.special_fields.clear();
                                for f in &mut t.field {
                                    f.special_fields.clear();
                                }
                            }
                            for p in &mut g.event_part {
                                p.span.clear();
                                p.sdv_unit_type_span.clear();
                                p.field_name_mapping_span.clear();
                                p.service_unit_span.clear();
                                p.topic_span.clear();
                                p.capacity_span.clear();
                                p.special_fields.clear();
                                if let Some(u) = p.sdv_unit_type.as_mut() {
                                    u.span.clear();
                                    u.package_name_span.clear();
                                    u.service_bundle_name_span.clear();
                                    u.type_name_span.clear();
                                    u.special_fields.clear();
                                }
                            }
                        }
                    }
                }
            }
            if mapping.has_sdv_type_mapping() {
                let t = mapping.mut_sdv_type_mapping();
                t.span.clear();
                t.unit_type_span.clear();
                t.someip_service_id_span.clear();
                t.version_major_span.clear();
                t.version_minor_span.clear();
                t.offered_interface_span.clear();
                t.offered_topic_span.clear();
                t.service_unit_mapping_span.clear();
                t.service_oriented_mapping_span.clear();
                t.special_fields.clear();
                if let Some(u) = t.unit_type.as_mut() {
                    u.span.clear();
                    u.package_name_span.clear();
                    u.service_bundle_name_span.clear();
                    u.type_name_span.clear();
                    u.special_fields.clear();
                }
                if t.has_offered_interface() {
                    let o = t.mut_offered_interface();
                    o.span.clear();
                    o.method_span.clear();
                    o.special_fields.clear();
                    for m in &mut o.method {
                        m.span.clear();
                        m.procedure_span.clear();
                        m.method_id_span.clear();
                        m.request_payload_mapping_span.clear();
                        m.response_payload_mapping_span.clear();
                        m.special_fields.clear();
                    }
                }
                if t.has_offered_topic() {
                    let o = t.mut_offered_topic();
                    o.span.clear();
                    o.type_definition_span.clear();
                    o.simple_mapping_span.clear();
                    o.someip_ff_methods_span.clear();
                    o.multipub_topic_info_span.clear();
                    o.special_fields.clear();
                    if o.has_simple_mapping() {
                        let s = o.mut_simple_mapping();
                        s.span.clear();
                        s.group_id_span.clear();
                        s.event_id_span.clear();
                        s.special_fields.clear();
                    }
                    if let Some(m) = o.multipub_topic_info.as_mut() {
                        m.span.clear();
                        m.someip_instance_id_span.clear();
                        m.special_fields.clear();
                    }
                    if let Some(td) = o.type_definition.as_mut() {
                        td.special_fields.clear();
                        for f in &mut td.field {
                            f.special_fields.clear();
                        }
                    }
                }
                for s in &mut t.service_unit_mapping {
                    s.span.clear();
                    s.service_unit_id_span.clear();
                    s.instance_id_span.clear();
                    s.fire_and_forget_method_span.clear();
                    s.special_fields.clear();
                    if let Some(id) = s.service_unit_id.as_mut() {
                        id.span.clear();
                        id.vm_name_span.clear();
                        id.package_name_span.clear();
                        id.service_bundle_name_span.clear();
                        id.service_instance_name_span.clear();
                        id.service_unit_name_span.clear();
                        id.special_fields.clear();
                    }
                    for f in &mut s.fire_and_forget_method {
                        f.span.clear();
                        f.service_id_span.clear();
                        f.instance_id_span.clear();
                        f.version_major_span.clear();
                        f.version_minor_span.clear();
                        f.method_id_span.clear();
                        f.special_fields.clear();
                    }
                }
                if let Some(s) = t.service_oriented_mapping.as_mut() {
                    s.span.clear();
                    s.rpc_interface_mapping_span.clear();
                    s.publication_mapping_span.clear();
                    s.special_fields.clear();
                    if s.has_rpc_interface_mapping() {
                        let r = s.mut_rpc_interface_mapping();
                        r.span.clear();
                        r.offered_interface_span.clear();
                        r.rpc_channel_mapping_span.clear();
                        r.special_fields.clear();
                        if let Some(o) = r.offered_interface.as_mut() {
                            o.span.clear();
                            o.method_span.clear();
                            o.special_fields.clear();
                            for m in &mut o.method {
                                m.span.clear();
                                m.procedure_span.clear();
                                m.method_id_span.clear();
                                m.request_payload_mapping_span.clear();
                                m.response_payload_mapping_span.clear();
                                m.special_fields.clear();
                            }
                        }
                        for m in &mut r.rpc_channel_mapping {
                            m.span.clear();
                            m.channel_span.clear();
                            m.instance_id_span.clear();
                            m.special_fields.clear();
                        }
                    }
                    if s.has_publication_mapping() {
                        let p = s.mut_publication_mapping();
                        p.span.clear();
                        p.offered_topic_span.clear();
                        p.topic_mapping_span.clear();
                        p.special_fields.clear();
                        if let Some(o) = p.offered_topic.as_mut() {
                            o.span.clear();
                            o.type_definition_span.clear();
                            o.simple_mapping_span.clear();
                            o.someip_ff_methods_span.clear();
                            o.special_fields.clear();
                            if let Some(td) = o.type_definition.as_mut() {
                                td.special_fields.clear();
                                for f in &mut td.field {
                                    f.special_fields.clear();
                                }
                            }
                        }
                        for m in &mut p.topic_mapping {
                            m.span.clear();
                            m.topic_span.clear();
                            m.instance_id_span.clear();
                            m.fire_and_forget_method_span.clear();
                            m.special_fields.clear();
                        }
                    }
                }
            }
        }

        assert_eq!(vsidl_entry_pest, vsidl_entry_proto);
    }
}
