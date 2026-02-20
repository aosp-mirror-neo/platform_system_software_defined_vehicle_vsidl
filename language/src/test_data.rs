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
//! Set of static vsidl and proto test files.
//! Various combination of those objects forms catalogs which are used for unit tests.

use lazy_static::lazy_static;
use pest::Parser;
use protobuf::descriptor::FileDescriptorProto;
use std::path::PathBuf;
use vsidl_v1::syntax::VsidlEntry;
use vsidl_v1_pest_parser::{
    converter,
    pest_parser::{Rule, VsidlParser},
};

#[cfg(test)]
use crate::android_bp::{extract_android_bp_filegroup, extract_android_bp_info, AndroidBpFile};
#[cfg(test)]
use crate::catalog::ProtoBuildTargetMetadata;
#[cfg(not(test))]
use vsidl_v1_common::android_bp::{
    extract_android_bp_filegroup, extract_android_bp_info, AndroidBpFile,
};
#[cfg(not(test))]
use vsidl_v1_common::catalog::ProtoBuildTargetMetadata;

/// Adds a VSIDL file as a static ref
///
/// static_ref_name is the name the static ref created by this macro shall have.
/// directory is a string of the path to the directory in which the VSIDL file is stored.
/// file is a string of the name of the VSIDL file.
#[macro_export]
macro_rules! vsidl_file {
    ($static_ref_name:ident, $directory:expr, $file:expr) => {
        lazy_static! {
            pub static ref $static_ref_name: (PathBuf, VsidlEntry) = {
                let vsidl_content = include_str!(concat!($directory, "/", $file));
                let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content);
                (
                    PathBuf::from($file),
                    converter::convert_to_vsidl_entry(parsed_file.unwrap().next().unwrap(), $file),
                )
            };
        }
    };
}

/// Adds a Proto file as a static ref.
///
/// static_ref_name is the name the static ref created by this macro shall have.
/// crate_name is the name of the crate of the proto.
/// proto is the name of the proto.
#[macro_export]
macro_rules! proto_file {
    ($static_ref_name:ident, $crate_name:ident, $proto:ident) => {
        lazy_static! {
            pub static ref $static_ref_name: (ProtoBuildTargetMetadata, FileDescriptorProto) = (
                ProtoBuildTargetMetadata {
                    folder: std::env::current_dir().unwrap().join("src/tests_vsidl_v1/protos"),
                    target_name: Some(format!("lib{}", stringify!($crate_name))),
                    crate_name: Some(stringify!($crate_name).to_string()),
                },
                $crate_name::$proto::file_descriptor().proto().clone()
            );
        }
    };
}

/// Adds a Android.bp file as a static ref.
///
/// static_ref_name is the name the static ref created by this macro shall have.
/// file_name is the name of the Android.bp.
#[macro_export]
macro_rules! android_bp_file {
    ($static_ref_name:ident, $file_name:expr) => {
        lazy_static! {
            pub static ref $static_ref_name: AndroidBpFile = AndroidBpFile {
                file_path: PathBuf::from($file_name),
                content: std::fs::read_to_string($file_name).unwrap(),
                rust_protobuf_targets: extract_android_bp_info(
                    &std::fs::read_to_string($file_name).unwrap()
                )
                .unwrap(),
                filegroup_targets: extract_android_bp_filegroup(
                    &std::fs::read_to_string($file_name).unwrap()
                )
                .unwrap()
            };
        }
    };
}

/*
    VSIDL FILES
    Test *.vsidl files are read [from disk] during COMPILE time and stored in the following static
    objects.
*/

vsidl_file!(VSIDL_ALL_AST_NODE_TYPES, "tests_vsidl_v1/vsidl", "all_ast_node_types.vsidl");
vsidl_file!(VSIDL_TWO_BUNDLES_VALID, "tests_vsidl_v1/vsidl", "two_bundles_valid.vsidl");
vsidl_file!(VSIDL_BUNDLE_ONE_VALID, "tests_vsidl_v1/vsidl", "service_bundle_one.vsidl");
vsidl_file!(VSIDL_BUNDLE_TWO_VALID, "tests_vsidl_v1/vsidl", "service_bundle_two.vsidl");
vsidl_file!(
    VSIDL_PUB_INVALID_SERVICE_UNIT_NAME_TOO_LONG,
    "tests_vsidl_v1/pubsub_service_unit_name_too_long",
    "test_service.vsidl"
);
vsidl_file!(
    VSIDL_RPC_INVALID_SERVICE_UNIT_NAME_TOO_LONG,
    "tests_vsidl_v1/rpc_service_unit_name_too_long",
    "test_service.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_BUNDLE_WITH_RUST_LIBRARY_PROP,
    "tests_vsidl_v1/vsidl_rust_library_prop",
    "test_service.vsidl"
);
vsidl_file!(
    VSIDL_TESTING_PROTO_CONFLICTS,
    "tests_vsidl_v1/test_proto_conflict",
    "test_service.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_USER_DEFINED_RPC_NAME,
    "tests_vsidl_v1/vsidl",
    "duplicated_user_defined_rpc_name.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_BUNDLES_WITH_SAME_FQN,
    "tests_vsidl_v1/vsidl",
    "service_bundles_with_same_fqn.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_SAME_FOR_RPC_AND_PUB,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_same_for_rpc_and_pub.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_USER_DEFINED_RPC_NAME_PER_TYPE,
    "tests_vsidl_v1/vsidl",
    "duplicated_user_defined_rpc_name_per_type.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_NOT_NAMING_CONVENTION,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_not_naming_convention.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_BUNDLE_INVALID_NAME,
    "tests_vsidl_v1/vsidl",
    "service_bundle_invalid_name.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_NOT_NAMING_CONVENTION_IN_DIAGNOSTICS,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_not_naming_convention_in_diagnostics.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME,
    "tests_vsidl_v1/vsidl",
    "duplicated_user_defined_pub_name.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME_PER_TYPE,
    "tests_vsidl_v1/vsidl",
    "duplicated_user_defined_pub_name_per_type.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME_IN_DIAGNOSTICS,
    "tests_vsidl_v1/vsidl",
    "duplicated_user_defined_pub_name_in_diagnostics.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITH_TWO_CHANNELS_AND_USER_DEFINED_NAME,
    "tests_vsidl_v1/vsidl",
    "publisher_with_two_channels_and_user_defined_name.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_MULTIPUB_TYPE_USAGE,
    "tests_vsidl_v1/vsidl",
    "duplicated_multipub_type_usage.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_UNNAMED_SERVER,
    "tests_vsidl_v1/vsidl",
    "duplicated_unnamed_server.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_TOO_LONG,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_too_long.vsidl"
);
vsidl_file!(VSIDL_GENERATED_NAME_TOO_LONG, "tests_vsidl_v1/vsidl", "generated_name_too_long.vsidl");
vsidl_file!(
    VSIDL_GENERATED_NAME_TOO_LONG_PDK29,
    "tests_vsidl_v1/vsidl",
    "generated_name_too_long_pdk29.vsidl"
);
vsidl_file!(
    VSIDL_GENERATED_NAME_TOO_LONG_WITH_SHORT_USER_DEFINED_NAME,
    "tests_vsidl_v1/vsidl",
    "generated_name_too_long_with_short_user_defined_name.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_TOO_LONG_IN_DIAGNOSTICS,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_too_long_in_diagnostics.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_WITHOUT_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_without_channel.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_NAME_MULTI_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "user_defined_name_multichannel.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_INVALID_LIBRARY_NAME,
    "tests_vsidl_v1/vsidl_rust_library_prop",
    "build_target_test_names.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_UNIT_NAME_PUBSUB_CONFLICT_WITHIN_BUNDLE,
    "tests_vsidl_v1/service_unit_name_conflict_within_service_bundle",
    "test_pubsub_pubsub_service.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_UNIT_NAME_RPC_PUBSUB_CONFLICT_WITHIN_BUNDLE,
    "tests_vsidl_v1/service_unit_name_conflict_within_service_bundle",
    "test_rpc_pubsub_service.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_UNIT_NAME_GENERATED_AND_MANUAL_CONFLICT_WITHIN_BUNDLE,
    "tests_vsidl_v1/service_unit_name_conflict_within_service_bundle",
    "test_generated_and_manual_pubsub.vsidl"
);
vsidl_file!(
    VSIDL_SERVICE_UNIT_NAME_MANUAL_CONFLICT_WITH_ONE_IN_DIAGNOSTICS,
    "tests_vsidl_v1/service_unit_name_conflict_within_service_bundle",
    "test_diagnostics_and_manual.vsidl"
);
vsidl_file!(
    VSIDL_GENERATED_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_DIAGNOSTIC,
    "tests_vsidl_v1/service_unit_name_conflict_within_service_bundle",
    "test_diagnostics_conflict_with_autogenerated.vsidl"
);
vsidl_file!(
    VSIDL_GENERATED_SINGLE_PUB_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_BY_TYPE,
    "tests_vsidl_v1/service_unit_name_conflict_by_type",
    "test_single_pub_conflict.vsidl"
);
vsidl_file!(
    VSIDL_GENERATED_MULTI_PUB_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_BY_TYPE_ALLOWED,
    "tests_vsidl_v1/service_unit_name_conflict_by_type",
    "test_multi_pub_conflict_valid.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_PUBLICATION_TYPE,
    "tests_vsidl_v1/vsidl",
    "nonexisting_publication_type.vsidl"
);
vsidl_file!(
    VSIDL_INVALID_PUBLICATION_TYPE,
    "tests_vsidl_v1/vsidl",
    "invalid_publication_type.vsidl"
);
vsidl_file!(
    VSIDL_INVALID_PUBLICATION_TYPE_GENERATED_NAME,
    "tests_vsidl_v1/vsidl",
    "invalid_publication_type_generated_name.vsidl"
);
vsidl_file!(
    VSIDL_INVALID_PUBLICATION_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "invalid_publication_channel.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITH_EMPTY_CHANNEL_ENUM,
    "tests_vsidl_v1/vsidl",
    "publisher_with_empty_channel_enum.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITH_EXPLICIT_UNIQUE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "publisher_with_explicit_unique_channel.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITH_IMPLICIT_UNIQUE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "publisher_with_implicit_unique_channel.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITHOUT_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "publisher_without_channel.vsidl"
);
vsidl_file!(
    VSIDL_PUBLISHER_WITHOUT_CHANNEL_PDK29,
    "tests_vsidl_v1/vsidl",
    "publisher_without_channel_pdk29.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITHOUT_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "subscriber_without_channel.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITH_IMPLICIT_UNIQUE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "subscriber_with_implicit_unique_channel.vsidl"
);
vsidl_file!(
    VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_EXPLICIT,
    "tests_vsidl_v1/vsidl",
    "single_pub_multiple_publishers_explicit.vsidl"
);
vsidl_file!(
    VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_IMPLICIT,
    "tests_vsidl_v1/vsidl",
    "single_pub_multiple_publishers_implicit.vsidl"
);
vsidl_file!(
    VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_UNIQUE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "single_pub_multiple_publishers_unique_channel.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_RPC_SERVER_TYPE,
    "tests_vsidl_v1/vsidl",
    "nonexisting_rpc_server_type.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_RPC_CLIENT_TYPE,
    "tests_vsidl_v1/vsidl",
    "nonexisting_rpc_client_type.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_SUBSCRIBER_TYPE,
    "tests_vsidl_v1/vsidl",
    "nonexisting_subscriber_type.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_SUBSCRIBER_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "nonexisting_subscriber_channel.vsidl"
);
vsidl_file!(
    VSIDL_NON_EXISTING_SUBSCRIBER_TOPIC,
    "tests_vsidl_v1/vsidl",
    "nonexisting_subscriber_topic.vsidl"
);
vsidl_file!(
    VSIDL_PUBLICATION_AS_RPC_SERVER_SERVICE,
    "tests_vsidl_v1/vsidl",
    "publication_as_rpc_server_service.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_REQUIRES_PUBLISHER_VALID,
    "tests_vsidl_v1/vsidl",
    "subscriber_requires_publisher_valid.vsidl"
);
vsidl_file!(
    VSIDL_RPC_CLIENT_WITHOUT_SERVER,
    "tests_vsidl_v1/vsidl",
    "rpc_client_without_server.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER,
    "tests_vsidl_v1/vsidl",
    "subscriber_without_publisher.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER_FOR_TOPIC_ONE,
    "tests_vsidl_v1/vsidl",
    "subscriber_without_publisher_for_topic_one.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER_FOR_ONE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "subscriber_without_publisher_for_one_channel.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_WITH_UNIQUE_CHANNEL,
    "tests_vsidl_v1/vsidl",
    "subscriber_with_unique_channel.vsidl"
);
vsidl_file!(
    VSIDL_DUPLICATED_GENERATED_TARGET_NAME,
    "tests_vsidl_v1/vsidl",
    "duplicated_generated_target_name.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_TARGET_NAME_CLASHES_WITH_GENERATED_NAME,
    "tests_vsidl_v1/vsidl",
    "user_defined_target_name_clashes_with_generated_name.vsidl"
);
vsidl_file!(
    VSIDL_USER_DEFINED_TARGET_NAME_RESOLVES_CONFLICT,
    "tests_vsidl_v1/vsidl",
    "user_defined_target_name_resolves_conflict.vsidl"
);
vsidl_file!(
    VSIDL_UNNECESSARY_FULLY_QUALIFIED_NAME,
    "tests_vsidl_v1/vsidl",
    "unnecessary_fully_qualified_name.vsidl"
);
vsidl_file!(
    VSIDL_NECESSARY_FULLY_QUALIFIED_NAME,
    "tests_vsidl_v1/vsidl",
    "necessary_fully_qualified_name.vsidl"
);
vsidl_file!(
    VSIDL_UNNECESSARY_FULLY_QUALIFIED_NAME_SUBPACKAGE,
    "tests_vsidl_v1/vsidl",
    "unnecessary_fully_qualified_name_subpackage.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_CHANNEL_LISTING,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_channel_listing.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_CHANNEL_LISTING_SEPARATE,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_channel_listing_separate.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_TOPIC_LISTING,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_topic_listing.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_TOPIC_LISTING_SEPARATE,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_topic_listing_separate.vsidl"
);
vsidl_file!(
    VSIDL_IMPLICIT_CHANNEL_LISTING,
    "tests_vsidl_v1/vsidl",
    "pubsub_implicit_channel_listing.vsidl"
);
vsidl_file!(
    VSIDL_IMPLICIT_TOPIC_LISTING,
    "tests_vsidl_v1/vsidl",
    "pubsub_implicit_topic_listing.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_CHANNEL_LISTING_OK,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_channel_listing_ok.vsidl"
);
vsidl_file!(
    VSIDL_EXPLICIT_TOPIC_LISTING_OK,
    "tests_vsidl_v1/vsidl",
    "pubsub_explicit_topic_listing_ok.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_CHANNELS_REDECLARATION,
    "tests_vsidl_v1/vsidl",
    "subscriber_channels_redeclaration.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_CHANNELS_REDECLARATION_SEPARATE,
    "tests_vsidl_v1/vsidl",
    "subscriber_channels_redeclaration_separate.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_TOPICS_REDECLARATION,
    "tests_vsidl_v1/vsidl",
    "subscriber_topics_redeclaration.vsidl"
);
vsidl_file!(
    VSIDL_SUBSCRIBER_TOPICS_REDECLARATION_SEPARATE,
    "tests_vsidl_v1/vsidl",
    "subscriber_topics_redeclaration_separate.vsidl"
);
vsidl_file!(
    VSIDL_PUBLICATION_MULTIPUB_MISUSE,
    "tests_vsidl_v1/vsidl",
    "publication_multipub_misuse.vsidl"
);
vsidl_file!(VSIDL_PUBLICATION_MULTIPUB, "tests_vsidl_v1/vsidl", "publication_multipub.vsidl");
vsidl_file!(VSIDL_DEPRECATED_KEYWORDS, "tests_vsidl_v1/vsidl", "deprecated_keywords.vsidl");
vsidl_file!(VSIDL_TIRE_PRESSURE_PUBLISHER, "tests_vsidl_v1/vsidl", "tire_pressure_topic.vsidl");
vsidl_file!(VSIDL_TOPIC_NAME_CONFLICT, "tests_vsidl_v1/vsidl", "topic_name_conflict.vsidl");
vsidl_file!(VSIDL_CHANNEL_NAME_CONFLICT, "tests_vsidl_v1/vsidl", "channel_name_conflict.vsidl");
vsidl_file!(VSIDL_DESCRIPTOR_MESSAGE_LOOP, "tests_vsidl_v1/vsidl", "proto_message_loop.vsidl");
vsidl_file!(
    VSIDL_DESCRIPTOR_MESSAGE_WITH_ANNOTATION,
    "tests_vsidl_v1/vsidl",
    "proto_message_with_annotation.vsidl"
);
vsidl_file!(
    VSIDL_DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION,
    "tests_vsidl_v1/vsidl",
    "proto_message_without_annotation.vsidl"
);

/*
    PROTOBUF FILE DESCRIPTORS
    For the sake of tests all the used protobuf files are compiled with the library itself and then
    descriptors are retrieved from generated static objects.
*/

proto_file!(ALL_AST_NODE_TYPES_PROTO, sdv_vsidl_v1_test_protos_rs, all_ast_node_types);
proto_file!(DESCRIPTOR_MESSAGE_PROTO, sdv_vsidl_v1_test_protos_rs, messages);
proto_file!(
    DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS,
    sdv_vsidl_v1_test_protos_rs,
    messages_no_annotations
);
proto_file!(DESCRIPTOR_UNKNOWN_OPTIONS, sdv_vsidl_v1_test_protos_rs, unknown_options);
proto_file!(DESCRIPTOR_NAME_TOO_LONG_PROTO, sdv_vsidl_v1_test_protos_rs, name_too_long);
proto_file!(DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO, sdv_vsidl_v1_test_protos_rs, another_dummy_rpc);
proto_file!(DESCRIPTOR_DUMMY_RPC_PROTO, sdv_vsidl_v1_test_protos_rs, dummy_rpc);
proto_file!(
    DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS,
    sdv_vsidl_v1_test_protos_rs,
    dummy_rpc_no_annotations
);
proto_file!(
    DESCRIPTOR_DUMMY_RPC_WITH_DEPENDENCY_PROTO,
    sdv_vsidl_v1_test_protos_rs,
    dummy_rpc_with_dependency
);
proto_file!(
    DESCRIPTOR_DUMMY_RPC_DEPENDENCY_PROTO,
    sdv_vsidl_v1_test_protos_rs,
    dummy_rpc_dependency
);
proto_file!(DESCRIPTOR_DUMMY_RPC_MESSAGE_PROTO, sdv_vsidl_v1_test_protos_rs, dummy_rpc_message);
proto_file!(DESCRIPTOR_DIAGNOSTICS_PROTO, sdv_vsidl_v1_test_protos_rs, diagnostics_messages);
proto_file!(
    DESCRIPTOR_INVALID_SDV_PUBLICATION,
    sdv_vsidl_v1_test_protos_rs,
    invalid_sdv_publication
);
proto_file!(DESCRIPTOR_PUB_MESSAGE_TOO_LONG_PROTO, sdv_vsidl_v1_test_protos_rs, pubsub_too_long);
proto_file!(DESCRIPTOR_RPC_MESSAGE_TOO_LONG_PROTO, sdv_vsidl_v1_test_protos_rs, rpc_too_long);
proto_file!(DESCRIPTOR_RPC_CONFLICT_PROTO, sdv_vsidl_v1_test_protos_rs, rpc_name_conflict);
proto_file!(
    DESCRIPTOR_PUBSUB_PUBSUB_CHANNEL_CONFLICT_PROTO,
    sdv_vsidl_v1_test_protos_rs,
    pubsub_pubsub_channel_conflict
);
proto_file!(DESCRIPTOR_PUBSUB_CONFLICT_PROTO, sdv_vsidl_v1_test_protos_rs, pubsub_name_conflict);
proto_file!(
    DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO,
    sdv_vsidl_v1_test_protos_rs,
    rpc_pubsub_channel_conflict
);
proto_file!(
    DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_without_annotation
);
proto_file!(
    DESCRIPTOR_MESSAGE_WITH_ANNOTATION,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_with_annotation
);
proto_file!(
    DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_submessage_same_target
);
proto_file!(
    DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_submessage_diff_target
);
proto_file!(DESCRIPTOR_MESSAGE_LOOP, sdv_vsidl_v1_test_protos_rs, proto_message_loop);
proto_file!(
    DESCRIPTOR_UNNECESSARY_FULLY_QUALIFIED_NAME,
    sdv_vsidl_v1_test_protos_rs,
    unnecessary_fully_qualified_name
);
proto_file!(
    DESCRIPTOR_PROTO_MESSAGE_EXCEEDS_MAX_SIZE,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_exceeds_max_size
);
proto_file!(
    DESCRIPTOR_PROTO_MESSAGE_WITHIN_MAX_SIZE,
    sdv_vsidl_v1_test_protos_rs,
    proto_message_within_max_size
);
proto_file!(
    DESCRIPTOR_EXPLICIT_CHANNEL_LISTING,
    sdv_vsidl_v1_test_protos_rs,
    pubsub_explicit_channel_listing
);
proto_file!(
    DESCRIPTOR_PUBLICATION_MULTIPUB_MISUSE,
    sdv_vsidl_v1_test_protos_rs,
    publication_multipub_misuse
);
proto_file!(DESCRIPTOR_RPC_STREAMING_VALID, sdv_vsidl_v1_test_protos_rs, rpc_streaming_call_valid);
proto_file!(
    DESCRIPTOR_RPC_STREAMING_INVALID,
    sdv_vsidl_v1_test_protos_rs,
    rpc_streaming_call_invalid
);
proto_file!(DESCRIPTOR_RPC_STREAMING_CALL, sdv_vsidl_v1_test_protos_rs, rpc_streaming_call);

/*
    Android.bp Files
    // NOTE: to avoid conflict with Soong, those file are commented used /* */
*/

android_bp_file!(EMPTY_ANDROID_BP, "src/tests_vsidl_v1/android_bp/empty.bp");
android_bp_file!(MISSING_NAME, "src/tests_vsidl_v1/android_bp/missing_name.bp");
android_bp_file!(MISSING_CRATE_NAME, "src/tests_vsidl_v1/android_bp/missing_crate.bp");
android_bp_file!(MISSING_NAME_AND_CRATE, "src/tests_vsidl_v1/android_bp/missing_name_and_crate.bp");
android_bp_file!(DEFAULTS_DEFINED, "src/tests_vsidl_v1/android_bp/defaults_defined.bp");
android_bp_file!(
    DEFAULTS_DEFINED_WITH_ATTRIBUTES,
    "src/tests_vsidl_v1/android_bp/defaults_defined_with_attributes.bp"
);
android_bp_file!(MISSING_PROTOS, "src/tests_vsidl_v1/android_bp/missing_protos.bp");
android_bp_file!(ANDROID_BP_MESSAGES_PROTOS, "src/tests_vsidl_v1/messages_protos.bp");
android_bp_file!(ANDROID_BP_FILEGROUP_PROTOS, "src/tests_vsidl_v1/android_bp/filegroup.bp");
android_bp_file!(
    ANDROID_BP_DUPLICATED_PROTOS,
    "src/tests_vsidl_v1/duplicated_protos_definition.bp"
);
android_bp_file!(
    ANDROID_BP_UNDEFINED_FILEGROUP_PROTOS,
    "src/tests_vsidl_v1/android_bp/undefined_filegroup.bp"
);
android_bp_file!(
    ANDROID_BP_MISSING_FILEGROUP_NAME,
    "src/tests_vsidl_v1/android_bp/missing_filegroup_name.bp"
);
android_bp_file!(
    ANDROID_BP_MISSING_FILEGROUP_SRCS,
    "src/tests_vsidl_v1/android_bp/missing_filegroup_srcs.bp"
);
android_bp_file!(
    ANDROID_BP_DUPLICATED_FILEGROUP_SAME_FILE,
    "src/tests_vsidl_v1/android_bp/duplicated_filegroup_a.bp"
);
android_bp_file!(
    ANDROID_BP_DUPLICATED_FILEGROUP_DIFFERENT_FILE,
    "src/tests_vsidl_v1/android_bp/duplicated_filegroup_b.bp"
);
android_bp_file!(
    ANDROID_BP_FILEGROUP_REFERENCE_TO_ANOTHER,
    "src/tests_vsidl_v1/android_bp/filegroup_referencing_another.bp"
);
android_bp_file!(
    ANDROID_BP_USED_FILEGROUP_REFERENCE_TO_ANOTHER,
    "src/tests_vsidl_v1/android_bp/used_filegroup_referencing_another.bp"
);
android_bp_file!(
    ANDROID_BP_FILEGROUP_NOT_MATCHING_GLOB,
    "src/tests_vsidl_v1/android_bp/empty_filegroup.bp"
);
