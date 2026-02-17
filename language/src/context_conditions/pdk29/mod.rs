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

// It is forbidden to have multiple publishers for the same multipub type in the same service bundle
mod multiple_publishers_same_multipub_type_per_bundle;
pub use multiple_publishers_same_multipub_type_per_bundle::MultiplePublishersSameMultipubTopicPerBundle;

/// Issues an error if the type of a publisher is either not defined at all or not a publication
mod publication_type;
pub use publication_type::PublicationType;

mod publisher_channel;
pub use publisher_channel::PublisherChannel;

/// Checks that the SdvPublication annotation is filled out correctly
mod sdv_publication;
pub use sdv_publication::SdvPublication;

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

// User-defined service unit names must be (globally) unique per unit type
mod unique_user_defined_service_unit_names_per_type;
pub use unique_user_defined_service_unit_names_per_type::UniqueUserDefinedServiceUnitNamesPerType;

// Proto message certain datatype require annotation
mod proto_message_datatype_require_annotation;
pub use proto_message_datatype_require_annotation::ProtoMessageRequireAnnotationType;

// VSIDL unnecessary fully qualified name
mod vsidl_unnecessary_fully_qualified_name;
pub use vsidl_unnecessary_fully_qualified_name::VsidlUnnecessaryFullyQualifiedName;

// Proto message exceed max size
mod proto_message_exceeds_max_size;
pub use proto_message_exceeds_max_size::ProtoMessageExceedsMaxSize;

// VSIDL redundant explicit channel listing
mod vsidl_redundant_explicit_channel_listing;
pub use vsidl_redundant_explicit_channel_listing::VsidlRedundantExplicitChannelListing;

// `MULTI_PUB` specification of publication is misuse
mod publication_multipub_misuse;
pub use publication_multipub_misuse::PublicationMultipubMisuse;

// publication is unused
mod publication_unused;
pub use publication_unused::PublicationUnused;
