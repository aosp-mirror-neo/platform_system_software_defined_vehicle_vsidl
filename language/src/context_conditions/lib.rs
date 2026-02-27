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

// General
pub mod context_condition;
pub use context_condition::ContextCondition;
pub mod context_condition_checker;
pub use context_condition_checker::ContextConditionChecker;
pub mod finding_id;
pub use finding_id::get_explanation_doc;
pub use finding_id::FindingId;
pub mod finding;
pub use finding::Finding;
pub mod finding_collector;
pub use finding_collector::FindingCollector;
pub mod finding_severity;
pub use finding_severity::FindingSeverity;
mod printing_utils;
pub use printing_utils::print_list_of_element;
pub mod pdk29;

// WARNINGS

// Warn if a service bundle contains multiple servers for the same service
mod multiple_rpc_servers;
pub use multiple_rpc_servers::MultipleRpcServersForSameInterface;

// Warn if catalog is empty or contains only empty .vsidl files
mod empty_catalog;
pub use empty_catalog::EmptyCatalog;

// ERRORS

mod rpc_client_channel_requires_server_channel;
pub use rpc_client_channel_requires_server_channel::RpcClientChannelRequiresServerChannel;

// Topics must be globally unique
mod topic_name_unique;
pub use topic_name_unique::TopicNameUnique;

mod topic_name_kebab_case;
pub use topic_name_kebab_case::TopicNameKebabCase;

// Publishers may not user-defined names without specifying the channel if the type has multiple channels
mod no_user_defined_names_for_multi_topic_publishers;
pub use no_user_defined_names_for_multi_topic_publishers::NoUserDefinedNamesForMultiChannelPublishers;

/// Issues an error if the type of a publisher is either not defined at all or not a publication
mod publication_type;
pub use publication_type::PublicationType;

mod publisher_capacity;
pub use publisher_capacity::PublisherCapacity;

mod publisher_capacity_mandatory;
pub use publisher_capacity_mandatory::PublisherCapacityMandatory;

mod publisher_topic_mandatory;
pub use publisher_topic_mandatory::PublisherTopicMandatory;

/// Issues an error if the unit type specified for an RPC client is unknown
mod rpc_client_type;
pub use rpc_client_type::RpcClientType;

/// Issues an error if there's no RPC server for an RPC client
mod rpc_client_requires_rpc_server;
pub use rpc_client_requires_rpc_server::RpcClientRequiresRpcServer;

/// Issues an error if the unit type specified for an RPC server is unknown
mod rpc_server_type;
pub use rpc_server_type::RpcServerType;

/// Checks that the fully-qualified names of each service bundle name is unique
mod service_bundle_fqn_duplicate;
pub use service_bundle_fqn_duplicate::ServiceBundleFqnDuplicate;

/// Checks that the fully-qualified names of each Protobuf message / RPC service is unique
mod proto_fqn_duplicate;
pub use proto_fqn_duplicate::ProtoFqnDuplicate;

/// Checks that the name of a service unit is not exceeding the internal limit
mod service_unit_name_too_long;
pub use service_unit_name_too_long::ServiceUnitNameTooLong;

/// Checks that the name of a service unit is not used multiple times in a service bundle
mod service_unit_name_duplicate;
pub use service_unit_name_duplicate::ServiceUnitNameDuplicate;

/// The type used by a subscriber must exist and must be a publication
mod subscriber_type;
pub use subscriber_type::SubscriberType;

/// Each subscriber must have a publisher
mod subscriber_requires_publisher;
pub use subscriber_requires_publisher::SubscriberRequiresPublisher;

// User-defined service unit names must be unique per service bundle
mod unique_user_defined_service_unit_names_per_bundle;
pub use unique_user_defined_service_unit_names_per_bundle::UniqueUserDefinedServiceUnitNamesPerBundle;

// Unique target_name considering both user-defined target name and automatically generated one
mod unique_target_name;
pub use unique_target_name::UniqueTargetName;

// User-defined service unit names must be (globally) unique per unit type
mod unique_user_defined_service_unit_names_per_type;
pub use unique_user_defined_service_unit_names_per_type::UniqueUserDefinedServiceUnitNamesPerType;

// User-defined service unit names must be in kebap case
mod user_defined_name_naming_convention;
pub use user_defined_name_naming_convention::UserDefinedNameNamingConvention;

// Checks that service bundle names do not have invalid names
mod service_bundle_naming_convention;
pub use service_bundle_naming_convention::ServiceBundleNamingConvention;

// User-defined service unit names may not be used in publishers with multiple channels
mod user_defined_names_only_one_topic;
pub use user_defined_names_only_one_topic::UserDefinedNamesOnlyOneTopic;

// User-defined build_cfg target_name must be snake case
mod user_defined_target_name_snake_case;
pub use user_defined_target_name_snake_case::UserDefinedTargetNameConvention;

// Android.bp rust_protobuf fields
mod android_bp_fields;
pub use android_bp_fields::AndroidBpFields;

// Android.bp discoverability of valid proto files
mod android_bp_mapping_protos;
pub use android_bp_mapping_protos::AndroidBpMappingProtos;

// Android.bp additional check on filegroups
mod android_bp_filegroups;
pub use android_bp_filegroups::AndroidBpFilegroups;

// Android.bp checks on filegroups pointing to other filegroups
mod android_bp_filegroups_composition;
pub use android_bp_filegroups_composition::AndroidBpFilegroupsComposition;

// Android.bp check for dangling files
mod android_bp_dangling_files;
pub use android_bp_dangling_files::AndroidBpDanglingFiles;

// Proto message metadata
mod proto_metadata;
pub use proto_metadata::ProtoMetadata;

// Proto message certain datatype require annotation
mod proto_message_datatype_require_annotation;
pub use proto_message_datatype_require_annotation::ProtoMessageRequireAnnotationType;

// VSIDL unnecessary fully qualified name
mod vsidl_unnecessary_fully_qualified_name;
pub use vsidl_unnecessary_fully_qualified_name::VsidlUnnecessaryFullyQualifiedName;

// Proto message exceed max size
mod proto_message_exceeds_max_size;
pub use proto_message_exceeds_max_size::ProtoMessageExceedsMaxSize;

// redundant explicit topic listing
mod redundant_explicit_topic_listing;
pub use redundant_explicit_topic_listing::VsidlRedundantExplicitTopicListing;

// Subscriber channel should not redeclare in same service bundle
mod subscriber_topic_redeclaration;
pub use subscriber_topic_redeclaration::SubscriberChannelsRedeclaration;

mod rpc_streaming;
pub use rpc_streaming::RpcStreaming;

mod rpc_duplicate_method_name;
pub use rpc_duplicate_method_name::RpcDuplicateMethodName;

mod rpc_channel_naming_convention;
pub use rpc_channel_naming_convention::RpcChannelNamingConvention;

mod rpc_channel_mandatory;
pub use rpc_channel_mandatory::RpcChannelMandatory;
