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

//! Unique identifiers for context condition violations

/// Macro to define the [FindingId] enum and its associated implementations.
///
/// This macro takes another macro as input, which it then invokes with a list
/// of finding definitions. Each definition consists of a documentation comment,
/// an error code prefix (like `E100`), a hexadecimal value, and the enum channel name.
///
/// # Example Usage
///
/// ```ignore
/// macro_rules! define_my_enum {
///     ($(#[$doc:meta] $code:ident: ($value:literal, $name:ident)),*) => {
///         // ... implementation using the provided definitions ...
///     };
/// }
///
/// finding_ids!(define_my_enum);
/// ```
macro_rules! finding_ids {
    ($macro:path) => {
        $macro! {
            // --- WARNINGS ---
            /// Multiple RPC servers are defined for the same service in the same service bundle.
            E100: (0x100, MultipleRpcServersForSameInterface),
            // Deprecated: This finding ID is no longer in use. Use E508 instead.
            /// Filegroup are defined in rust_protobuf target of Android.bp
            E101: (0x101, AndroidBpMappingProtosFilegroup),
            /// rust_protobuf target of Android.bp has invalid proto
            E102: (0x102, AndroidBpMappingEmptyGlob),
            /// rust_protobuf target of Android.bp has multiple element in the protos pointing to the same file
            E103: (0x103, AndroidBpMappingDuplicateProtoFileSameLibrary),
            /// multiple rust_protobuf target of Android.bp points to the same file
            E104: (0x104, AndroidBpMappingDuplicateProtoFileMultipleLibraries),
            /// filegroup target srcs points to not existing files
            E105: (0x105, AndroidBpFilegroupEmptyGlob),
            /// VSIDL Redundant Unique channel declaration
            E106: (0x106, VsidlRedundantUniqueChannelDeclaration),
            /// unnecessary fully qualified name in VSIDL file
            E107: (0x107, VsidlUnnecessaryFullyQualifiedName),
            /// Redundant explicit channel listing specification for publisher or subscriber
            E108: (0x108, VsidlRedundantExplicitChannelListing),
            /// `MULTI_PUB` specification for publication is misuse
            E109: (0x109, PublicationMultipubMisuse),
            /// publication is unused
            E10A: (0x10A, PublicationUnused),
            /// File in catalog is not referenced in any filegroup
            E10B: (0x10B, AndroidBpDanglingFile),
            /// Catalog is empty or contains only empty .vsidl files
            E10C: (0x10C, EmptyCatalog),
            // --- ERRORS ---
            // -- Naming Conventions --
            /// User-defined service unit names must be in kebap case. First character is not lowercase letter.
            E201: (0x201, UserDefinedNameNamingConventionFirstChar),
            /// User-defined service unit names must be in kebap case. Contains invalid character.
            E202: (0x202, UserDefinedNameNamingConventionCharset),
            /// User-defined service unit names must be in kebap case. Contains consecutive hyphens.
            E203: (0x203, UserDefinedNameNamingConventionConsecutiveHyphen),
            /// User-defined service unit names must be in kebap case. Last letter is hyphen.
            E204: (0x204, UserDefinedNameNamingConventionLastChar),
            /// User-defined build_cfg target_name must be in snake case. Not all chars are in [a-z0-9_].
            E205: (0x205, UserDefinedTargetNameConventionCharset),
            /// User-defined build_cfg target_name must be in snake case. Contains consecutive underscores.
            E206: (0x206, UserDefinedTargetNameConventionConsecutiveUnderscore),
            /// User-defined build_cfg target_name must be in snake case. Last char must not be underscore.
            E207: (0x207, UserDefinedTargetNameConventionLastChar),
            /// User-defined build_cfg target_name must be in snake case. First char must not be underscore.
            E208: (0x208, UserDefinedTargetNameConventionFirstChar),
            /// Service bundle names must not be empty
            E209: (0x209, ServiceBundleNamingConventionEmptyName),
            /// Service bundle names must start with a unicode XID start character
            E20A: (0x20A, ServiceBundleNamingConventionFirstChar),
            /// Service bundle names must only contain with a unicode XID continue characters
            E20B: (0x20B, ServiceBundleNamingConventionContinueChar),
            /// Service bundle names must not use reserved names such as Rust, Java, or C++ keywords as their name
            E20C: (0x20C, ServiceBundleNamingConventionForbiddenName),
            /// Topics must be in snake case
            E20D: (0x20D, TopicNameKebabCase),
            /// RPC channels must be in lowercase kebab case
            E20E: (0x20E, RpcChannelNamingConvention),
            // -- Uniqueness / Name Conflicts --
            /// Multiple publishers are defined for the same multipub type in the same service bundle.
            E300: (0x300, MultiplePublishersSameMultipubTopic),
            /// User-defined target name in build_cfg must not conflict with generated names
            E301: (0x301, UniqueTargetName),
            /// User-defined service unit names must be unique per service bundle. Conflict in publisher names.
            E302: (0x302, UniqueUserDefinedServiceUnitNamesPerBundlePublisher),
            /// User-defined service unit names must be unique per service bundle. Conflict in RPC names.
            E303: (0x303, UniqueUserDefinedServiceUnitNamesPerBundleRpc),
            /// User-defined service unit names must be (globally) unique per unit type. Conflict in publisher names.
            E304: (0x304, UniqueUserDefinedServiceUnitNamesPerTypePublisher),
            /// User-defined service unit names must be (globally) unique per unit type. Conflict in RPC names.
            E305: (0x305, UniqueUserDefinedServiceUnitNamesPerTypeRpc),
            /// User-defined service unit names may only be used in publishers with one channel.
            E306: (0x306, UserDefinedNamesOnlyOneChannel),
            /// Single Pub publisher channel is used by multiple publishers
            E307: (0x307, PublisherChannelSinglePubUsedByMultiplePublishers),
            /// Service unit names may not occur multiple times in the same service bundle
            E308: (0x308, ServiceUnitNameDuplicate),
            /// Service bundles must have unique fully-qualified names
            E309: (0x309, ServiceBundleFqnDuplicate),
            /// filegroup target must be unique across all Android.bp
            E310: (0x310, AndroidBpFieldsFilegroupDuplicated),
            /// Subscriber channels should not redeclare in the same service bundle
            E311: (0x311, SubscriberChannelsRedeclaration),
            /// rpc method should not have duplicate name in the same service
            E312: (0x312, RpcDuplicateMethodName),
            /// Proto message or service has a duplicate fully qualified name
            E313: (0x313, ProtoFqnDuplicate),
            /// Topics must be globally unique across all publishers
            E314: (0x314, TopicNameUnique),
            // -- Structural Errors --
            /// SdvPublication annotation must include a positive message_count
            E401: (0x401, SdvPublicationMessageCountPositive),
            /// SdvPublication annotation must not set channels enum for MULTI_PUB publications
            E402: (0x402, SdvPublicationMultiPubWithChannels),
            /// A proto publication may only be owned by a single unit type definition
            E403: (0x403, OwnedUnitTypeProtoPublicationHasMultipleOwners),
            /// An RPC service may only be owned by a single unit type definition
            E404: (0x404, OwnedUnitTypeProtoRpcServiceHasMultipleOwners),
            /// A service bundle may not own unit types from a different package
            E405: (0x405, UnitTypeOwnershipFromDifferentPackage),
            /// Publisher capacity must be at least 2 and an even number
            E406: (0x406, PublisherCapacity),
            /// Publisher capacity is mandatory in latest VSIDL syntax
            E407: (0x407, PublisherCapacityMandatory),
            /// Topic is mandatory for publishers and subscribers in latest VSIDL syntax
            E408: (0x408, TopicMandatory),
            /// RPC channel is mandatory in latest VSIDL syntax
            E409: (0x409, RpcChannelMandatory),
            /// RPC client channel maps to an existing server-side channel definition
            E40A: (0x40A, RpcClientChannelRequiresServerChannel),
            /// RPC channel must only be used by one service
            E40B: (0x40B, RpcChannelUniquePerService),
            // -- Limitations --
            /// Publishers using user-defined names must specify a channel if the type has multiple channels
            E501: (0x501, NoUserDefinedNamesForMultiChannelPublishers),
            /// Service unit name may not exceed a length limit
            E502: (0x502, ServiceUnitNameTooLong),
            /// Having an RPC client requires having an RPC server with the requested service
            E503: (0x503, RpcClientRequiresRpcServer),
            /// Having a subscriber requires having a publisher with the requested type + channel
            E504: (0x504, SubscriberRequiresPublisher),
            /// Android.bp rust_protobuf without name
            E505: (0x505, AndroidBpFieldsRustProtobufNameNotExist),
            /// Android.bp rust_protobuf without crate_name
            E506: (0x506, AndroidBpFieldsRustProtobufCrateNameNotExist),
            /// Android.bp rust_protobuf defaults
            E507: (0x507, AndroidBpFieldsRustProtobufDefaultsDefined),
            /// Android.bp filegroup without name
            E508: (0x508, AndroidBpFieldsFilegroupNameDoesNotExist),
            /// Android.bp filegroup without name
            E509: (0x509, AndroidBpFieldsFilegroupSrcsDoesNotExist),
            /// Android.bp with recursive filegroup reference
            E510: (0x510, AndroidBpFilegroupsCompositionSrcsPointsToFilegroup),
            /// Certain fields in proto messages require annotations
            E511: (0x511, ProtoMessageRequireAnnotationType),
            /// proto message should not have self/cross reference field which would form a loop
            E512: (0x512, ProtoMessageLoop),
            /// proto message should not exceed max size
            E513: (0x513, ProtoMessageExceedsMaxSize),
            /// rpc method should not use bidirectional streaming
            E514: (0x514, RpcMethodBidirectionalStreaming),
            // -- Usage of unknown types --
            /// Types of publishers must exist
            E601: (0x601, PublicationTypeDoesNotExist),
            /// Types of publishers must be publications
            E602: (0x602, PublicationTypeIsNotPublication),
            /// Types of RPC servers must exist
            E603: (0x603, RpcServerTypeDoesNotExist),
            /// Unit types owned by a service bundle must be defined by proto messages
            E604: (0x604, OwnedUnitTypeDoesNotExist),
            /// Unit types owned by a service bundle must be defined by only one proto messages
            E605: (0x605, OwnedUnitTypeIsAmbiguous),
            /// Publisher uses channel that is not a value of the channels enum
            E606: (0x606, PublisherChannelDoesNotExist),
            /// Publisher uses channel for a Proto type that does not use channels
            E607: (0x607, PublisherChannelForChannellessProto),
            /// Subscriber uses a type that does not exist as a Proto publication
            E608: (0x608, SubscriberTypeDoesNotExist),
            /// Subscriber uses a type that does exist as a Proto publication, but does not have the channel requested by the subscriber
            E609: (0x609, SubscriberTypeDoesNotHaveRequestedChannel),
            /// RPC client uses a type that does not exist as a Proto RPC service
            E60A: (0x60A, RpcClientTypeDoesNotExist),
            /// RPC client uses a type that is implemented by more than one Proto RPC service
            E60B: (0x60B, RpcClientTypeIsAmbiguous),
            /// Channels enum of SDV Publication annotation must be defined
            E60C: (0x60C, SdvPublicationChannelsEnumUndefined),
            /// Publisher uses a type that does not exist as a unit type
            E60D: (0x60D, UnitTypeDoesNotExistForPublisher),
            /// Subscriber uses a type that does not exist as a unit type
            E60E: (0x60E, UnitTypeDoesNotExistForSubscriber),
            /// RPC server uses a type that does not exist as a unit type
            E60F: (0x60F, UnitTypeDoesNotExistForRpcServer),
            /// RPC client uses a type that does not exist as a unit type
            E610: (0x610, UnitTypeDoesNotExistForRpcClient),
            /// Proto library does not exist for a given proto message
            E611: (0x611, ProtoMetadataProtoLibraryDoesNotExist),
            /// Proto library points to a filegroup that does not exist
            E612: (0x612, AndroidBpMappingProtosFilegroupDoesNotExist),
            /// File is not parsable
            E700: (0x700, ParserError)
        }
    };
}

/// Macro to define the [FindingId] enum and implement `TryFrom<&str>`, `TryFrom<u32>`, and `Display`.
///
/// This macro is invoked by the [finding_ids!] macro.
macro_rules! define_error_enum {
    ($(#[$doc:meta] $code:ident: ($value:literal, $name:ident)),*) => {
        /// Enum representing unique identifiers for findings (errors or warnings)
        /// detected by VSIDL context conditions.
        ///
        /// The hexadecimal value's leading digit often indicates the severity or category:
        /// - `0x1xx`: Warnings
        /// - `0x2xx`: Naming convention violations
        /// - `0x3xx`: Uniqueness/conflict errors
        /// - `0x4xx`: Structural errors in definitions/annotations
        /// - `0x5xx`: Limitations or unsupported features
        /// - `0x6xx`: Usage of unknown or mismatched types/channels
        ///
        /// Finding IDs are designed to be stable. If a context condition is removed,
        /// its corresponding ID should be marked as deprecated or reserved but not reused,
        /// ensuring that documentation links remain valid.
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        #[repr(u32)]
        pub enum FindingId {
            $(#[$doc] $name = $value,)*
            /// Represents an unknown or unspecified finding. Should generally not be used.
            Unknown = 0,
        }

        /// Allows converting a string representation (e.g., "E100") into a [FindingId].
        ///
        /// # Parameters
        ///
        /// * `code`: The string code (e.g., "E100", "E201") to convert.
        ///
        /// # Returns
        ///
        /// * `Ok(FindingId)` if the string is a valid code.
        /// * `Err(())` if the string does not match any known finding ID code.
        impl std::convert::TryFrom<&str> for FindingId {
            type Error = ();

            fn try_from(code: &str) -> Result<Self, Self::Error> {
                match code {
                    $(stringify!($code) => Ok(FindingId::$name),)*
                    "E000" => Ok(FindingId::Unknown),
                    _ => Err(()),
                }
            }
        }

        /// Allows converting a raw `u32` value into a [FindingId].
        ///
        /// # Parameters
        ///
        /// * `value`: The `u32` value (e.g., `0x100`, `0x201`) to convert.
        ///
        /// # Returns
        ///
        /// * `Ok(FindingId)` if the value corresponds to a valid finding ID.
        /// * `Err(())` if the value does not match any known finding ID.
        impl std::convert::TryFrom<u32> for FindingId {
            type Error = ();

            fn try_from(value: u32) -> Result<Self, Self::Error> {
                match value {
                    $($value => Ok(FindingId::$name),)*
                    0 => Ok(FindingId::Unknown),
                    _ => Err(()),
                }
            }
        }

        /// Formats the [FindingId] as a string code (e.g., "E100").
        ///
        /// # Parameters
        ///
        /// * `f`: The formatter.
        ///
        /// # Returns
        ///
        /// * `fmt::Result` indicating success or failure of formatting.
        impl std::fmt::Display for FindingId {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "E{:03x}", *self as u32)
            }
        }
    };
}

// Generate the FindingId enum and its implementations using the macros.
finding_ids!(define_error_enum);

macro_rules! define_get_explanation_doc {
    ($($(#[$doc:meta])* $code:ident: ($value:literal, $name:ident)),*) => {
        /// Returns the markdown documentation for a given finding ID code.
        pub fn get_explanation_doc(code: &str) -> Option<&'static str> {
            match code {
                $(
                    stringify!($code) => Some(include_str!(concat!("docs/", stringify!($code), ".md"))),
                )*
                &_ => None,
            }
        }
    };
}
finding_ids!(define_get_explanation_doc);

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_try_from_str() {
        assert_eq!(
            FindingId::try_from("E100").unwrap(),
            FindingId::MultipleRpcServersForSameInterface
        );
        assert_eq!(FindingId::try_from("E000").unwrap(), FindingId::Unknown);
        assert!(FindingId::try_from("E999").is_err()); // Non-existent code
        assert!(FindingId::try_from("100").is_err()); // Missing 'E' prefix
        assert!(FindingId::try_from("E10G").is_err()); // Invalid hex
        assert!(FindingId::try_from("").is_err()); // Empty string
    }

    #[test]
    fn test_try_from_u32() {
        assert_eq!(
            FindingId::try_from(0x100).unwrap(),
            FindingId::MultipleRpcServersForSameInterface
        );
        assert_eq!(FindingId::try_from(0).unwrap(), FindingId::Unknown);
        assert!(FindingId::try_from(999).is_err()); // Non-existent value
        assert!(FindingId::try_from(0xFFFF).is_err()); // Large non-existent value
    }

    #[test]
    fn test_display() {
        assert_eq!(FindingId::MultipleRpcServersForSameInterface.to_string(), "E100");
        assert_eq!(FindingId::Unknown.to_string(), "E000");
    }

    #[test]
    fn test_get_explanation_doc_known_error_code() {
        let doc = get_explanation_doc("E101");
        assert!(doc.is_some());
        assert!(doc.unwrap().contains("E101 is deprecated."));
    }

    #[test]
    fn test_get_explanation_doc_unknown_error_code() {
        let doc = get_explanation_doc("UNKNOWN_ERROR_CODE");
        assert!(doc.is_none());
    }
}
