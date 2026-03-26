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
//! This crate provides a mutable visitor infrastructure for VSIDL,
//! allowing traversal and **modification** (or "transformation")
//! of the Abstract Syntax Tree (AST).

use crate::android_bp::AndroidBpFile;
use crate::catalog::Catalog;
use crate::ProtoBuildTargetMetadata;
use mockall::automock;
use protobuf::MessageField;
use std::path::PathBuf;

use vsidl_v1::annotations::SdvPublication;
// Core VSIDL
use vsidl_v1::syntax::service_bundle::{Client, Publisher, Server, Subscriber};
use vsidl_v1::syntax::{BuildConfiguration, ServiceBundle, VsidlEntry};

// DIAGNOSTICS
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::routine::Method;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::{
    DataItem, Event, InputOutputControlData, Routine,
};
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;

// SOME/IP
use vsidl_v1::someip::some_ip_mapping::sdv_type_mapping::{
    publication_mapping::TopicMapping as SomeIpTopicMapping,
    rpc_interface::Method as SomeIpMappingRpcInterfaceMethod,
    rpc_interface_mapping::RpcChannelMapping as SomeIpRpcChannelMapping,
    topic::SimpleTopicMapping as SomeIpSimpleTopicMapping,
    PublicationMapping as SomeIpPublicationMapping, RpcInterface as SomeIpMappingRpcInterface,
    RpcInterfaceMapping as SomeIpRpcInterfaceMapping,
    SdvServiceUnitMapping as SomeIpSdvServiceUnitMapping, ServiceUnitId as SomeIpServiceUnitId,
    SomeIpFFMethod, Topic as SomeIpTopic,
};
use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::{
    event::GroupEvent as SomeIpGroupEvent, event::GroupEventPart as SomeIpGroupEventPart,
    event::SimpleEvent as SomeIpSimpleEvent,
    rpc_interface::RequestResponseMethod as SomeIpRequestResponseMethod, Event as SomeIpEvent,
    RpcInterface as SomeIpRpcInterface,
};
use vsidl_v1::someip::some_ip_mapping::some_ip_instance_mapping::SdvTopicParameters;
use vsidl_v1::someip::some_ip_mapping::{
    sdv_type_mapping::ServiceOrientedMapping as SomeIpServiceOrientedMapping,
    SdvTypeMapping as SomeIpSdvTypeMapping, SomeIpInstanceImplementation, SomeIpInstanceMapping,
    SomeIpServiceMapping, UnitType as SomeIpUnitType,
};
use vsidl_v1::someip::SomeIpMapping;
use vsidl_v1::someip_type::some_ip_type::{some_ip_struct::SomeIpStructField, SomeIpStruct};

// VHAL
use vsidl_v1::vhal_proxy::vhal_mapping::{
    AreaMapping as VhalAreaMapping, DataSource as VhalDataSource, DefaultValue as VhalDefaultValue,
    PropertyArea as VhalPropertyArea, VehicleProperty as VhalVehicleProperty,
};
use vsidl_v1::vhal_proxy::VhalMapping;
use vsidl_v1::VehicleAreaConfig::VehicleAreaConfig as VhalVehicleAreaConfig;
use vsidl_v1::VehiclePropConfig::VehiclePropConfig as VhalVehiclePropConfig;

// PROTOBUF
use protobuf::descriptor::descriptor_proto::ReservedRange as ProtoReservedRange;
use protobuf::descriptor::enum_descriptor_proto::EnumReservedRange as ProtoEnumReservedRange;
use protobuf::descriptor::{
    DescriptorProto as ProtoDescriptor, EnumDescriptorProto as ProtoEnumDescriptor,
    EnumOptions as ProtoEnumOptions, EnumValueDescriptorProto as ProtoEnumValueDescriptor,
    EnumValueOptions as ProtoEnumValueOptions, FieldDescriptorProto as ProtoFieldDescriptor,
    FieldOptions as ProtoFieldOptions, FileDescriptorProto as ProtoFileDescriptor,
    MessageOptions as ProtoMessageOptions, MethodDescriptorProto as ProtoMethodDescriptor,
    MethodOptions as ProtoMethodOptions, OneofDescriptorProto as ProtoOneofDescriptor,
    OneofOptions as ProtoOneofOptions, ServiceDescriptorProto as ProtoServiceDescriptor,
    ServiceOptions as ProtoServiceOptions,
};

/// Couples a mutable proto file metadata and the mutable file descriptor proto itself.
///
/// This allows mutable visitors to access and potentially **modify** both the content ([ProtoFileDescriptor])
/// and associated build metadata ([ProtoBuildTargetMetadata]) during traversal.
pub type ProtoFile<'a> = (&'a mut ProtoBuildTargetMetadata, &'a mut ProtoFileDescriptor);

/// Couples a mutable VSIDL file from the main catalog (not a dependency) with its mutable relative path.
///
/// This allows mutable visitors to access and potentially **modify** both the origin file path ([PathBuf])
/// and the VSIDL entry ([VsidlEntry]) within the catalog being processed.
pub type CatalogVsidlFile<'a> = (&'a mut PathBuf, &'a mut VsidlEntry);

/// Defines the contract for mutably visiting nodes in a VSIDL Abstract Syntax Tree (AST).
///
/// Implementors of this trait can traverse and potentially **modify** the AST structure
/// derived from `.vsidl`, `.proto`, and `Android.bp` files.
pub trait AstNodeMut {
    /// Accepts a mutable visitor, triggering the appropriate visit methods.
    ///
    /// The implementation should typically follow this pattern:
    /// 1. Call the corresponding `start_visit_*` method on the `visitor`.
    /// 2. Call `self.traverse(visitor)` to visit child nodes.
    /// 3. Call the corresponding `end_visit_*` method on the `visitor`.
    ///
    /// # Parameters
    ///
    /// * `visitor`: A mutable reference to an object implementing [VsidlVisitorMut].
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut);

    /// Traverses the child nodes of the current AST node mutably.
    ///
    /// This method defines the order in which child nodes are visited.
    /// It should call `accept` on each child node, passing the `visitor`.
    /// If a node has no children, this method should have an empty implementation.
    ///
    /// # Parameters
    ///
    /// * `visitor`: A mutable reference to an object implementing [VsidlVisitorMut].
    fn traverse(&'_ mut self, visitor: &mut dyn VsidlVisitorMut);
}

/// A helper trait to enable casting concrete visitor types to `&mut dyn VsidlVisitorMut`.
///
/// Rust's trait objects (`dyn Trait`) are unsized. This trait provides a bridge
/// to obtain a mutable trait object reference from a sized type `T` that implements [VsidlVisitorMut].
pub trait AsDynVisitorMut {
    /// Returns a mutable trait object reference to the [VsidlVisitorMut].
    fn as_mut_dyn_visitor(&mut self) -> &mut dyn VsidlVisitorMut;
}

/// Blanket implementation of [AsDynVisitorMut] for any type `T` that implements [VsidlVisitorMut].
impl<T: VsidlVisitorMut> AsDynVisitorMut for T {
    /// Returns `self` cast as a mutable trait object reference.
    fn as_mut_dyn_visitor(&mut self) -> &mut dyn VsidlVisitorMut {
        self
    }
}

/// Defines the mutable visitor pattern interface for traversing and **modifying** the VSIDL AST.
///
/// This trait provides `start_visit_*` and `end_visit_*` methods for each type of node
/// in the AST. Concrete visitors implement this trait, overriding methods for the
/// node types they need to modify or inspect. The default implementations do nothing.
///
/// The `#[automock]` attribute enables automatic mock generation for testing purposes.
#[automock]
pub trait VsidlVisitorMut: AsDynVisitorMut {
    /// Called before visiting the children of a mutable [Catalog] node.
    /// Allows modification of the [Catalog] before its children are visited.
    ///
    /// # Parameters
    /// * `_catalog`: A mutable reference to the [Catalog] being visited.
    fn start_visit_catalog(&mut self, _catalog: &mut Catalog) {}
    /// Called after visiting the children of a mutable [Catalog] node.
    /// Allows modification of the [Catalog] after its children have been visited.
    ///
    /// # Parameters
    /// * `_catalog`: A mutable reference to the [Catalog] being visited.
    fn end_visit_catalog(&mut self, _catalog: &mut Catalog) {}

    /// Called before visiting the children of a mutable [AndroidBpFile] node.
    /// Allows modification of the [AndroidBpFile] before its children are visited.
    ///
    /// # Parameters
    /// * `_android_bp_file`: A mutable reference to the [AndroidBpFile] being visited.
    fn start_visit_android_bp_file(&mut self, _android_bp_file: &mut AndroidBpFile) {}
    /// Called after visiting the children of a mutable [AndroidBpFile] node.
    /// Allows modification of the [AndroidBpFile] after its children have been visited.
    ///
    /// # Parameters
    /// * `_android_bp_file`: A mutable reference to the [AndroidBpFile] being visited.
    fn end_visit_android_bp_file(&mut self, _android_bp_file: &mut AndroidBpFile) {}

    /// Called before visiting the children of a mutable [ProtoFile] node.
    /// Allows modification of the [ProtoFile] tuple before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFile] tuple being visited.
    #[allow(clippy::needless_lifetimes)] // lifetimes are needed by automock
    fn start_visit_proto_file<'a>(&mut self, _node: &'_ mut ProtoFile<'a>) {}
    /// Called after visiting the children of a mutable [ProtoFile] node.
    /// Allows modification of the [ProtoFile] tuple after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFile] tuple being visited.
    #[allow(clippy::needless_lifetimes)] // lifetimes are needed by automock
    fn end_visit_proto_file<'a>(&mut self, _node: &'_ mut ProtoFile<'a>) {}

    /// Called before visiting the children of a mutable [ProtoFileDescriptor] node.
    /// Allows modification of the [ProtoFileDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFileDescriptor] being visited.
    fn start_visit_file_descriptor_proto(&mut self, _node: &mut ProtoFileDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoFileDescriptor] node.
    /// Allows modification of the [ProtoFileDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFileDescriptor] being visited.
    fn end_visit_file_descriptor_proto(&mut self, _node: &mut ProtoFileDescriptor) {}

    /// Called before visiting the children of a mutable [CatalogVsidlFile] node.
    /// Allows modification of the [CatalogVsidlFile] tuple before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [CatalogVsidlFile] tuple being visited.
    #[allow(clippy::needless_lifetimes)] // lifetimes are needed by automock
    fn start_visit_catalog_vsidl_file<'a>(&mut self, _node: &'_ mut CatalogVsidlFile<'a>) {}
    /// Called after visiting the children of a mutable [CatalogVsidlFile] node.
    /// Allows modification of the [CatalogVsidlFile] tuple after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [CatalogVsidlFile] tuple being visited.
    #[allow(clippy::needless_lifetimes)] // lifetimes are needed by automock
    fn end_visit_catalog_vsidl_file<'a>(&mut self, _node: &'_ mut CatalogVsidlFile<'a>) {}

    /// Called before visiting the children of a mutable [VsidlEntry] node.
    /// Allows modification of the [VsidlEntry] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VsidlEntry] being visited.
    fn start_visit_vsidl_entry(&mut self, _node: &mut VsidlEntry) {}
    /// Called after visiting the children of a mutable [VsidlEntry] node.
    /// Allows modification of the [VsidlEntry] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VsidlEntry] being visited.
    fn end_visit_vsidl_entry(&mut self, _node: &mut VsidlEntry) {}

    /// Called before visiting the children of a mutable [ServiceBundle] node.
    /// Allows modification of the [ServiceBundle] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ServiceBundle] being visited.
    fn start_visit_service_bundle(&mut self, _node: &mut ServiceBundle) {}
    /// Called after visiting the children of a mutable [ServiceBundle] node.
    /// Allows modification of the [ServiceBundle] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ServiceBundle] being visited.
    fn end_visit_service_bundle(&mut self, _node: &mut ServiceBundle) {}

    /// Called before visiting the children of a mutable [Publisher] node.
    /// Allows modification of the [Publisher] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Publisher] being visited.
    fn start_visit_publisher(&mut self, _node: &mut Publisher) {}
    /// Called after visiting the children of a mutable [Publisher] node.
    /// Allows modification of the [Publisher] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Publisher] being visited.
    fn end_visit_publisher(&mut self, _node: &mut Publisher) {}

    /// Called before visiting the children of a mutable [Subscriber] node.
    /// Allows modification of the [Subscriber] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Subscriber] being visited.
    fn start_visit_subscriber(&mut self, _node: &mut Subscriber) {}
    /// Called after visiting the children of a mutable [Subscriber] node.
    /// Allows modification of the [Subscriber] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Subscriber] being visited.
    fn end_visit_subscriber(&mut self, _node: &mut Subscriber) {}

    /// Called before visiting the children of a mutable [Server] node.
    /// Allows modification of the [Server] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Server] being visited.
    fn start_visit_server(&mut self, _node: &mut Server) {}
    /// Called after visiting the children of a mutable [Server] node.
    /// Allows modification of the [Server] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Server] being visited.
    fn end_visit_server(&mut self, _node: &mut Server) {}

    /// Called before visiting the children of a mutable [Client] node.
    /// Allows modification of the [Client] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Client] being visited.
    fn start_visit_client(&mut self, _node: &mut Client) {}
    /// Called after visiting the children of a mutable [Client] node.
    /// Allows modification of the [Client] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Client] being visited.
    fn end_visit_client(&mut self, _node: &mut Client) {}

    /// Called before visiting the children of a mutable [DiagnosticsDeclaration] node.
    /// Allows modification of the [DiagnosticsDeclaration] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [DiagnosticsDeclaration] being visited.
    fn start_visit_diagnostics(&mut self, _node: &mut DiagnosticsDeclaration) {}
    /// Called after visiting the children of a mutable [DiagnosticsDeclaration] node.
    /// Allows modification of the [DiagnosticsDeclaration] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [DiagnosticsDeclaration] being visited.
    fn end_visit_diagnostics(&mut self, _node: &mut DiagnosticsDeclaration) {}

    /// Called before visiting the children of a mutable [DataItem] node within Diagnostics.
    /// Allows modification of the [DataItem] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [DataItem] being visited.
    fn start_visit_diagnostics_data_item(&mut self, _node: &mut DataItem) {}
    /// Called after visiting the children of a mutable [DataItem] node within Diagnostics.
    /// Allows modification of the [DataItem] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [DataItem] being visited.
    fn end_visit_diagnostics_data_item(&mut self, _node: &mut DataItem) {}

    /// Called before visiting the children of a mutable [InputOutputControlData] node within Diagnostics.
    /// Allows modification of the [InputOutputControlData] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [InputOutputControlData] being visited.
    fn start_visit_diagnostics_io_control(&mut self, _node: &mut InputOutputControlData) {}
    /// Called after visiting the children of a mutable [InputOutputControlData] node within Diagnostics.
    /// Allows modification of the [InputOutputControlData] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [InputOutputControlData] being visited.
    fn end_visit_diagnostics_io_control(&mut self, _node: &mut InputOutputControlData) {}

    /// Called before visiting the children of a mutable [Routine] node within Diagnostics.
    /// Allows modification of the [Routine] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Routine] being visited.
    fn start_visit_diagnostics_routine(&mut self, _node: &mut Routine) {}
    /// Called after visiting the children of a mutable [Routine] node within Diagnostics.
    /// Allows modification of the [Routine] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Routine] being visited.
    fn end_visit_diagnostics_routine(&mut self, _node: &mut Routine) {}

    /// Called before visiting the children of a mutable [Method] node within a Diagnostics Routine.
    /// Allows modification of the [Method] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Method] being visited.
    fn start_visit_diagnostics_routine_method(&mut self, _node: &mut Method) {}
    /// Called after visiting the children of a mutable [Method] node within a Diagnostics Routine.
    /// Allows modification of the [Method] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Method] being visited.
    fn end_visit_diagnostics_routine_method(&mut self, _node: &mut Method) {}

    /// Called before visiting the children of a mutable [Event] node within Diagnostics.
    /// Allows modification of the [Event] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Event] being visited.
    fn start_visit_diagnostics_event(&mut self, _node: &mut Event) {}
    /// Called after visiting the children of a mutable [Event] node within Diagnostics.
    /// Allows modification of the [Event] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [Event] being visited.
    fn end_visit_diagnostics_event(&mut self, _node: &mut Event) {}

    /// Called before visiting the children of a mutable [BuildConfiguration] node.
    /// Allows modification of the [BuildConfiguration] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [BuildConfiguration] being visited.
    fn start_visit_build_cfg(&mut self, _node: &mut BuildConfiguration) {}
    /// Called after visiting the children of a mutable [BuildConfiguration] node.
    /// Allows modification of the [BuildConfiguration] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [BuildConfiguration] being visited.
    fn end_visit_build_cfg(&mut self, _node: &mut BuildConfiguration) {}

    /// Called before visiting the children of a mutable [SomeIpMapping] node.
    /// Allows modification of the [SomeIpMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMapping] being visited.
    fn start_visit_someip_mapping(&mut self, _node: &mut SomeIpMapping) {}
    /// Called after visiting the children of a mutable [SomeIpMapping] node.
    /// Allows modification of the [SomeIpMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMapping] being visited.
    fn end_visit_someip_mapping(&mut self, _node: &mut SomeIpMapping) {}

    /// Called before visiting the children of a mutable [SomeIpServiceMapping] node.
    /// Allows modification of the [SomeIpServiceMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceMapping] being visited.
    fn start_visit_someip_service_mapping(&mut self, _node: &mut SomeIpServiceMapping) {}
    /// Called after visiting the children of a mutable [SomeIpServiceMapping] node.
    /// Allows modification of the [SomeIpServiceMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceMapping] being visited.
    fn end_visit_someip_service_mapping(&mut self, _node: &mut SomeIpServiceMapping) {}

    /// Called before visiting the children of a [SdvTopicParameters] node.
    /// Allows modification of the [SdvTopicParameters] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SdvTopicParameters] being visited.
    fn start_visit_sdv_topic_parameters(&mut self, _node: &mut SdvTopicParameters) {}
    /// Called after visiting the children of a [SdvTopicParameters] node.
    /// Allows modification of the [SdvTopicParameters] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SdvTopicParameters] being visited.
    fn end_visit_sdv_topic_parameters(&mut self, _node: &mut SdvTopicParameters) {}

    /// Called before visiting the children of a mutable [SomeIpInstanceMapping] node.
    /// Allows modification of the [SomeIpInstanceMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpInstanceMapping] being visited.
    fn start_visit_someip_instance_mapping(&mut self, _node: &mut SomeIpInstanceMapping) {}
    /// Called after visiting the children of a mutable [SomeIpInstanceMapping] node.
    /// Allows modification of the [SomeIpInstanceMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpInstanceMapping] being visited.
    fn end_visit_someip_instance_mapping(&mut self, _node: &mut SomeIpInstanceMapping) {}

    /// Called before visiting the children of a mutable [SomeIpInstanceImplementation] node.
    /// Allows modification of the [SomeIpInstanceImplementation] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpInstanceImplementation] being visited.
    fn start_visit_someip_instance_implementation(
        &mut self,
        _node: &mut SomeIpInstanceImplementation,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpInstanceImplementation] node.
    /// Allows modification of the [SomeIpInstanceImplementation] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpInstanceImplementation] being visited.
    fn end_visit_someip_instance_implementation(
        &mut self,
        _node: &mut SomeIpInstanceImplementation,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpRpcInterface] node (within InstanceImplementation).
    /// Allows modification of the [SomeIpRpcInterface] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcInterface] being visited.
    fn start_visit_someip_rpc_interface(&mut self, _node: &mut SomeIpRpcInterface) {}
    /// Called after visiting the children of a mutable [SomeIpRpcInterface] node (within InstanceImplementation).
    /// Allows modification of the [SomeIpRpcInterface] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcInterface] being visited.
    fn end_visit_someip_rpc_interface(&mut self, _node: &mut SomeIpRpcInterface) {}

    /// Called before visiting the children of a mutable [SomeIpMappingRpcInterface] node (within SdvTypeMapping).
    /// Allows modification of the [SomeIpMappingRpcInterface] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMappingRpcInterface] being visited.
    fn start_visit_someip_type_mapping_rpc_interface(
        &mut self,
        _node: &mut SomeIpMappingRpcInterface,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpMappingRpcInterface] node (within SdvTypeMapping).
    /// Allows modification of the [SomeIpMappingRpcInterface] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMappingRpcInterface] being visited.
    fn end_visit_someip_type_mapping_rpc_interface(
        &mut self,
        _node: &mut SomeIpMappingRpcInterface,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpMappingRpcInterfaceMethod] node.
    /// Allows modification of the [SomeIpMappingRpcInterfaceMethod] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMappingRpcInterfaceMethod] being visited.
    fn start_visit_someip_type_mapping_rpc_interface_method(
        &mut self,
        _node: &mut SomeIpMappingRpcInterfaceMethod,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpMappingRpcInterfaceMethod] node.
    /// Allows modification of the [SomeIpMappingRpcInterfaceMethod] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpMappingRpcInterfaceMethod] being visited.
    fn end_visit_someip_type_mapping_rpc_interface_method(
        &mut self,
        _node: &mut SomeIpMappingRpcInterfaceMethod,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpTopic] node.
    /// Allows modification of the [SomeIpTopic] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpTopic] being visited.
    fn start_visit_someip_topic(&mut self, _node: &mut SomeIpTopic) {}
    /// Called after visiting the children of a mutable [SomeIpTopic] node.
    /// Allows modification of the [SomeIpTopic] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpTopic] being visited.
    fn end_visit_someip_topic(&mut self, _node: &mut SomeIpTopic) {}

    /// Called before visiting the children of a mutable [SomeIpSimpleTopicMapping] node.
    /// Allows modification of the [SomeIpSimpleTopicMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSimpleTopicMapping] being visited.
    fn start_visit_someip_simple_topic_mapping(&mut self, _node: &mut SomeIpSimpleTopicMapping) {}
    /// Called after visiting the children of a mutable [SomeIpSimpleTopicMapping] node.
    /// Allows modification of the [SomeIpSimpleTopicMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSimpleTopicMapping] being visited.
    fn end_visit_someip_simple_topic_mapping(&mut self, _node: &mut SomeIpSimpleTopicMapping) {}

    /// Called before visiting the children of a mutable [SomeIpUnitType] node.
    /// Allows modification of the [SomeIpUnitType] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpUnitType] being visited.
    fn start_visit_someip_unit_type(&mut self, _node: &mut SomeIpUnitType) {}
    /// Called after visiting the children of a mutable [SomeIpUnitType] node.
    /// Allows modification of the [SomeIpUnitType] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpUnitType] being visited.
    fn end_visit_someip_unit_type(&mut self, _node: &mut SomeIpUnitType) {}

    /// Called before visiting the children of a mutable [SomeIpRequestResponseMethod] node.
    /// Allows modification of the [SomeIpRequestResponseMethod] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRequestResponseMethod] being visited.
    fn start_visit_someip_request_response_method(
        &mut self,
        _node: &mut SomeIpRequestResponseMethod,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpRequestResponseMethod] node.
    /// Allows modification of the [SomeIpRequestResponseMethod] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRequestResponseMethod] being visited.
    fn end_visit_someip_request_response_method(
        &mut self,
        _node: &mut SomeIpRequestResponseMethod,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpEvent] node.
    /// Allows modification of the [SomeIpEvent] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpEvent] being visited.
    fn start_visit_someip_event(&mut self, _node: &mut SomeIpEvent) {}
    /// Called after visiting the children of a mutable [SomeIpEvent] node.
    /// Allows modification of the [SomeIpEvent] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpEvent] being visited.
    fn end_visit_someip_event(&mut self, _node: &mut SomeIpEvent) {}

    /// Called before visiting the children of a mutable [SomeIpSimpleEvent] node.
    /// Allows modification of the [SomeIpSimpleEvent] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSimpleEvent] being visited.
    fn start_visit_someip_simple_event(&mut self, _node: &mut SomeIpSimpleEvent) {}
    /// Called after visiting the children of a mutable [SomeIpSimpleEvent] node.
    /// Allows modification of the [SomeIpSimpleEvent] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSimpleEvent] being visited.
    fn end_visit_someip_simple_event(&mut self, _node: &mut SomeIpSimpleEvent) {}

    /// Called before visiting the children of a mutable [SomeIpGroupEvent] node.
    /// Allows modification of the [SomeIpGroupEvent] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpGroupEvent] being visited.
    fn start_visit_someip_group_event(&mut self, _node: &mut SomeIpGroupEvent) {}
    /// Called after visiting the children of a mutable [SomeIpGroupEvent] node.
    /// Allows modification of the [SomeIpGroupEvent] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpGroupEvent] being visited.
    fn end_visit_someip_group_event(&mut self, _node: &mut SomeIpGroupEvent) {}

    /// Called before visiting the children of a mutable [SomeIpGroupEventPart] node.
    /// Allows modification of the [SomeIpGroupEventPart] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpGroupEventPart] being visited.
    fn start_visit_someip_group_event_part(&mut self, _node: &mut SomeIpGroupEventPart) {}
    /// Called after visiting the children of a mutable [SomeIpGroupEventPart] node.
    /// Allows modification of the [SomeIpGroupEventPart] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpGroupEventPart] being visited.
    fn end_visit_someip_group_event_part(&mut self, _node: &mut SomeIpGroupEventPart) {}

    /// Called before visiting the children of a mutable [SomeIpStruct] node.
    /// Allows modification of the [SomeIpStruct] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpStruct] being visited.
    fn start_visit_someip_struct(&mut self, _node: &mut SomeIpStruct) {}
    /// Called after visiting the children of a mutable [SomeIpStruct] node.
    /// Allows modification of the [SomeIpStruct] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpStruct] being visited.
    fn end_visit_someip_struct(&mut self, _node: &mut SomeIpStruct) {}

    /// Called before visiting the children of a mutable [SomeIpStructField] node.
    /// Allows modification of the [SomeIpStructField] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpStructField] being visited.
    fn start_visit_someip_struct_field(&mut self, _node: &mut SomeIpStructField) {}
    /// Called after visiting the children of a mutable [SomeIpStructField] node.
    /// Allows modification of the [SomeIpStructField] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpStructField] being visited.
    fn end_visit_someip_struct_field(&mut self, _node: &mut SomeIpStructField) {}

    /// Called before visiting the children of a mutable [SomeIpSdvTypeMapping] node.
    /// Allows modification of the [SomeIpSdvTypeMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSdvTypeMapping] being visited.
    fn start_visit_someip_sdv_type_mapping(&mut self, _node: &mut SomeIpSdvTypeMapping) {}
    /// Called after visiting the children of a mutable [SomeIpSdvTypeMapping] node.
    /// Allows modification of the [SomeIpSdvTypeMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSdvTypeMapping] being visited.
    fn end_visit_someip_sdv_type_mapping(&mut self, _node: &mut SomeIpSdvTypeMapping) {}

    /// Called before visiting the children of a mutable [SomeIpSdvServiceUnitMapping] node.
    /// Allows modification of the [SomeIpSdvServiceUnitMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSdvServiceUnitMapping] being visited.
    fn start_visit_someip_sdv_service_unit_mapping(
        &mut self,
        _node: &mut SomeIpSdvServiceUnitMapping,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpSdvServiceUnitMapping] node.
    /// Allows modification of the [SomeIpSdvServiceUnitMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpSdvServiceUnitMapping] being visited.
    fn end_visit_someip_sdv_service_unit_mapping(
        &mut self,
        _node: &mut SomeIpSdvServiceUnitMapping,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpServiceUnitId] node.
    /// Allows modification of the [SomeIpServiceUnitId] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceUnitId] being visited.
    fn start_visit_someip_service_unit_id(&mut self, _node: &mut SomeIpServiceUnitId) {}
    /// Called after visiting the children of a mutable [SomeIpServiceUnitId] node.
    /// Allows modification of the [SomeIpServiceUnitId] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceUnitId] being visited.
    fn end_visit_someip_service_unit_id(&mut self, _node: &mut SomeIpServiceUnitId) {}

    /// Called before visiting the children of a mutable [SomeIpFFMethod] node (Fire and Forget).
    /// Allows modification of the [SomeIpFFMethod] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpFFMethod] being visited.
    fn start_visit_someip_ff_method(&mut self, _node: &mut SomeIpFFMethod) {}
    /// Called after visiting the children of a mutable [SomeIpFFMethod] node (Fire and Forget).
    /// Allows modification of the [SomeIpFFMethod] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpFFMethod] being visited.
    fn end_visit_someip_ff_method(&mut self, _node: &mut SomeIpFFMethod) {}

    /// Called before visiting the children of a mutable [SomeIpServiceOrientedMapping] node.
    /// Allows modification of the [SomeIpServiceOrientedMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceOrientedMapping] being visited.
    fn start_visit_someip_service_oriented_mapping(
        &mut self,
        _node: &mut SomeIpServiceOrientedMapping,
    ) {
    }
    /// Called after visiting the children of a mutable [SomeIpServiceOrientedMapping] node.
    /// Allows modification of the [SomeIpServiceOrientedMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpServiceOrientedMapping] being visited.
    fn end_visit_someip_service_oriented_mapping(
        &mut self,
        _node: &mut SomeIpServiceOrientedMapping,
    ) {
    }

    /// Called before visiting the children of a mutable [SomeIpRpcInterfaceMapping] node.
    /// Allows modification of the [SomeIpRpcInterfaceMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcInterfaceMapping] being visited.
    fn start_visit_someip_rpc_interface_mapping(&mut self, _node: &mut SomeIpRpcInterfaceMapping) {}
    /// Called after visiting the children of a mutable [SomeIpRpcInterfaceMapping] node.
    /// Allows modification of the [SomeIpRpcInterfaceMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcInterfaceMapping] being visited.
    fn end_visit_someip_rpc_interface_mapping(&mut self, _node: &mut SomeIpRpcInterfaceMapping) {}

    /// Called before visiting the children of a mutable [SomeIpRpcChannelMapping] node.
    /// Allows modification of the [SomeIpRpcChannelMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcChannelMapping] being visited.
    fn start_visit_someip_rpc_channel_mapping(&mut self, _node: &mut SomeIpRpcChannelMapping) {}
    /// Called after visiting the children of a mutable [SomeIpRpcChannelMapping] node.
    /// Allows modification of the [SomeIpRpcChannelMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpRpcChannelMapping] being visited.
    fn end_visit_someip_rpc_channel_mapping(&mut self, _node: &mut SomeIpRpcChannelMapping) {}

    /// Called before visiting the children of a mutable [SomeIpPublicationMapping] node.
    /// Allows modification of the [SomeIpPublicationMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpPublicationMapping] being visited.
    fn start_visit_someip_publication_mapping(&mut self, _node: &mut SomeIpPublicationMapping) {}
    /// Called after visiting the children of a mutable [SomeIpPublicationMapping] node.
    /// Allows modification of the [SomeIpPublicationMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpPublicationMapping] being visited.
    fn end_visit_someip_publication_mapping(&mut self, _node: &mut SomeIpPublicationMapping) {}

    /// Called before visiting the children of a mutable [SomeIpTopicMapping] node.
    /// Allows modification of the [SomeIpTopicMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpTopicMapping] being visited.
    fn start_visit_someip_topic_mapping(&mut self, _node: &mut SomeIpTopicMapping) {}
    /// Called after visiting the children of a mutable [SomeIpTopicMapping] node.
    /// Allows modification of the [SomeIpTopicMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SomeIpTopicMapping] being visited.
    fn end_visit_someip_topic_mapping(&mut self, _node: &mut SomeIpTopicMapping) {}

    /// Called before visiting the children of a mutable [VhalMapping] node.
    /// Allows modification of the [VhalMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalMapping] being visited.
    fn start_visit_vhal_mapping(&mut self, _node: &mut VhalMapping) {}
    /// Called after visiting the children of a mutable [VhalMapping] node.
    /// Allows modification of the [VhalMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalMapping] being visited.
    fn end_visit_vhal_mapping(&mut self, _node: &mut VhalMapping) {}

    /// Called before visiting the children of a mutable [VhalVehicleProperty] node.
    /// Allows modification of the [VhalVehicleProperty] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehicleProperty] being visited.
    fn start_visit_vhal_vehicle_property(&mut self, _node: &mut VhalVehicleProperty) {}
    /// Called after visiting the children of a mutable [VhalVehicleProperty] node.
    /// Allows modification of the [VhalVehicleProperty] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehicleProperty] being visited.
    fn end_visit_vhal_vehicle_property(&mut self, _node: &mut VhalVehicleProperty) {}

    /// Called before visiting the children of a mutable [VhalVehiclePropConfig] node.
    /// Allows modification of the [VhalVehiclePropConfig] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehiclePropConfig] being visited.
    fn start_visit_vhal_vehicle_prop_config(&mut self, _node: &mut VhalVehiclePropConfig) {}
    /// Called after visiting the children of a mutable [VhalVehiclePropConfig] node.
    /// Allows modification of the [VhalVehiclePropConfig] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehiclePropConfig] being visited.
    fn end_visit_vhal_vehicle_prop_config(&mut self, _node: &mut VhalVehiclePropConfig) {}

    /// Called before visiting the children of a mutable [VhalDefaultValue] node.
    /// Allows modification of the [VhalDefaultValue] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalDefaultValue] being visited.
    fn start_visit_vhal_default_value(&mut self, _node: &mut VhalDefaultValue) {}
    /// Called after visiting the children of a mutable [VhalDefaultValue] node.
    /// Allows modification of the [VhalDefaultValue] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalDefaultValue] being visited.
    fn end_visit_vhal_default_value(&mut self, _node: &mut VhalDefaultValue) {}

    /// Called before visiting the children of a mutable [VhalVehicleAreaConfig] node.
    /// Allows modification of the [VhalVehicleAreaConfig] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehicleAreaConfig] being visited.
    fn start_visit_vhal_vehicle_area_config(&mut self, _node: &mut VhalVehicleAreaConfig) {}
    /// Called after visiting the children of a mutable [VhalVehicleAreaConfig] node.
    /// Allows modification of the [VhalVehicleAreaConfig] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalVehicleAreaConfig] being visited.
    fn end_visit_vhal_vehicle_area_config(&mut self, _node: &mut VhalVehicleAreaConfig) {}

    /// Called before visiting the children of a mutable [VhalDataSource] node.
    /// Allows modification of the [VhalDataSource] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalDataSource] being visited.
    fn start_visit_vhal_data_source(&mut self, _node: &mut VhalDataSource) {}
    /// Called after visiting the children of a mutable [VhalDataSource] node.
    /// Allows modification of the [VhalDataSource] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalDataSource] being visited.
    fn end_visit_vhal_data_source(&mut self, _node: &mut VhalDataSource) {}

    /// Called before visiting the children of a mutable [VhalAreaMapping] node.
    /// Allows modification of the [VhalAreaMapping] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalAreaMapping] being visited.
    fn start_visit_vhal_area_mapping(&mut self, _node: &mut VhalAreaMapping) {}
    /// Called after visiting the children of a mutable [VhalAreaMapping] node.
    /// Allows modification of the [VhalAreaMapping] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalAreaMapping] being visited.
    fn end_visit_vhal_area_mapping(&mut self, _node: &mut VhalAreaMapping) {}

    /// Called before visiting the children of a mutable [VhalPropertyArea] node.
    /// Allows modification of the [VhalPropertyArea] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalPropertyArea] being visited.
    fn start_visit_vhal_property_area(&mut self, _node: &mut VhalPropertyArea) {}
    /// Called after visiting the children of a mutable [VhalPropertyArea] node.
    /// Allows modification of the [VhalPropertyArea] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [VhalPropertyArea] being visited.
    fn end_visit_vhal_property_area(&mut self, _node: &mut VhalPropertyArea) {}

    /// Called before visiting the children of a mutable [ProtoDescriptor] (Protobuf Message) node.
    /// Allows modification of the [ProtoDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoDescriptor] being visited.
    fn start_visit_proto_descriptor(&mut self, _node: &mut ProtoDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoDescriptor] (Protobuf Message) node.
    /// Allows modification of the [ProtoDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoDescriptor] being visited.
    fn end_visit_proto_descriptor(&mut self, _node: &mut ProtoDescriptor) {}

    /// Called before visiting the children of a mutable [SdvPublication] annotation node.
    /// Allows modification of the [SdvPublication] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SdvPublication] being visited.
    fn start_visit_sdv_publication(&mut self, _node: &mut SdvPublication) {}
    /// Called after visiting the children of a mutable [SdvPublication] annotation node.
    /// Allows modification of the [SdvPublication] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [SdvPublication] being visited.
    fn end_visit_sdv_publication(&mut self, _node: &mut SdvPublication) {}

    /// Convenience method called before visiting children of a Protobuf message
    /// identified as a SINGLE_PUB publication via the [SdvPublication] annotation.
    /// This is called *after* [start_visit_proto_descriptor].
    /// Allows inspection of the message and publication data before children are visited.
    ///
    /// # Parameters
    /// * `_message`: The [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn start_visit_proto_single_pub_message(
        &mut self,
        _message: &ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }
    /// Convenience method called after visiting children of a Protobuf message
    /// identified as a SINGLE_PUB publication.
    /// This is called *before* [end_visit_proto_descriptor].
    /// Allows inspection of the message and publication data after children have been visited.
    ///
    /// # Parameters
    /// * `_message`: The [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn end_visit_proto_single_pub_message(
        &mut self,
        _message: &ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }

    /// Convenience method called before visiting children of a Protobuf message
    /// identified as a MULTI_PUB publication via the [SdvPublication] annotation.
    /// This is called *after* [start_visit_proto_descriptor].
    /// Allows modification of the message before its children are visited.
    ///
    /// # Parameters
    /// * `_message`: A mutable reference to the [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn start_visit_proto_multi_pub_message(
        &mut self,
        _message: &mut ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }
    /// Convenience method called after visiting children of a Protobuf message
    /// identified as a MULTI_PUB publication.
    /// This is called *before* [end_visit_proto_descriptor].
    /// Allows modification of the message after its children have been visited.
    ///
    /// # Parameters
    /// * `_message`: A mutable reference to the [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn end_visit_proto_multi_pub_message(
        &mut self,
        _message: &mut ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }

    /// Called before visiting the children of a mutable [ProtoFieldDescriptor] (Protobuf Field) node.
    /// Allows modification of the [ProtoFieldDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFieldDescriptor] being visited.
    fn start_visit_proto_field_descriptor(&mut self, _node: &mut ProtoFieldDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoFieldDescriptor] (Protobuf Field) node.
    /// Allows modification of the [ProtoFieldDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFieldDescriptor] being visited.
    fn end_visit_proto_field_descriptor(&mut self, _node: &mut ProtoFieldDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoFieldOptions] node.
    /// Allows modification of the [ProtoFieldOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFieldOptions] being visited.
    fn start_visit_proto_field_options(&mut self, _node: &mut ProtoFieldOptions) {}
    /// Called after visiting the children of a mutable [ProtoFieldOptions] node.
    /// Allows modification of the [ProtoFieldOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoFieldOptions] being visited.
    fn end_visit_proto_field_options(&mut self, _node: &mut ProtoFieldOptions) {}

    /// Called before visiting the children of a mutable [ProtoEnumDescriptor] (Protobuf Enum) node.
    /// Allows modification of the [ProtoEnumDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumDescriptor] being visited.
    fn start_visit_proto_enum_descriptor(&mut self, _node: &mut ProtoEnumDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoEnumDescriptor] (Protobuf Enum) node.
    /// Allows modification of the [ProtoEnumDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumDescriptor] being visited.
    fn end_visit_proto_enum_descriptor(&mut self, _node: &mut ProtoEnumDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoEnumOptions] node.
    /// Allows modification of the [ProtoEnumOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumOptions] being visited.
    fn start_visit_proto_enum_options(&mut self, _node: &mut ProtoEnumOptions) {}
    /// Called after visiting the children of a mutable [ProtoEnumOptions] node.
    /// Allows modification of the [ProtoEnumOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumOptions] being visited.
    fn end_visit_proto_enum_options(&mut self, _node: &mut ProtoEnumOptions) {}

    /// Called before visiting the children of a mutable [ProtoEnumReservedRange] node.
    /// Allows modification of the [ProtoEnumReservedRange] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumReservedRange] being visited.
    fn start_visit_proto_enum_reserved_range(&mut self, _node: &mut ProtoEnumReservedRange) {}
    /// Called after visiting the children of a mutable [ProtoEnumReservedRange] node.
    /// Allows modification of the [ProtoEnumReservedRange] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumReservedRange] being visited.
    fn end_visit_proto_enum_reserved_range(&mut self, _node: &mut ProtoEnumReservedRange) {}

    /// Called before visiting the children of a mutable [ProtoEnumValueDescriptor] (Protobuf Enum Value) node.
    /// Allows modification of the [ProtoEnumValueDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumValueDescriptor] being visited.
    fn start_visit_proto_enum_value_descriptor(&mut self, _node: &mut ProtoEnumValueDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoEnumValueDescriptor] (Protobuf Enum Value) node.
    /// Allows modification of the [ProtoEnumValueDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumValueDescriptor] being visited.
    fn end_visit_proto_enum_value_descriptor(&mut self, _node: &mut ProtoEnumValueDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoEnumValueOptions] node.
    /// Allows modification of the [ProtoEnumValueOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumValueOptions] being visited.
    fn start_visit_proto_enum_value_options(&mut self, _node: &mut ProtoEnumValueOptions) {}
    /// Called after visiting the children of a mutable [ProtoEnumValueOptions] node.
    /// Allows modification of the [ProtoEnumValueOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoEnumValueOptions] being visited.
    fn end_visit_proto_enum_value_options(&mut self, _node: &mut ProtoEnumValueOptions) {}

    /// Called before visiting the children of a mutable [ProtoOneofDescriptor] (Protobuf Oneof) node.
    /// Allows modification of the [ProtoOneofDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoOneofDescriptor] being visited.
    fn start_visit_proto_oneof_descriptor(&mut self, _node: &mut ProtoOneofDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoOneofDescriptor] (Protobuf Oneof) node.
    /// Allows modification of the [ProtoOneofDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoOneofDescriptor] being visited.
    fn end_visit_proto_oneof_descriptor(&mut self, _node: &mut ProtoOneofDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoOneofOptions] node.
    /// Allows modification of the [ProtoOneofOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoOneofOptions] being visited.
    fn start_visit_proto_oneof_options(&mut self, _node: &mut ProtoOneofOptions) {}
    /// Called after visiting the children of a mutable [ProtoOneofOptions] node.
    /// Allows modification of the [ProtoOneofOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoOneofOptions] being visited.
    fn end_visit_proto_oneof_options(&mut self, _node: &mut ProtoOneofOptions) {}

    /// Called before visiting the children of a mutable [ProtoMessageOptions] node.
    /// Allows modification of the [ProtoMessageOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMessageOptions] being visited.
    fn start_visit_proto_message_options(&mut self, _node: &mut ProtoMessageOptions) {}
    /// Called after visiting the children of a mutable [ProtoMessageOptions] node.
    /// Allows modification of the [ProtoMessageOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMessageOptions] being visited.
    fn end_visit_proto_message_options(&mut self, _node: &mut ProtoMessageOptions) {}

    /// Called before visiting the children of a mutable [ProtoReservedRange] node within a message.
    /// Allows modification of the [ProtoReservedRange] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoReservedRange] being visited.
    fn start_visit_proto_reserved_range(&mut self, _node: &mut ProtoReservedRange) {}
    /// Called after visiting the children of a mutable [ProtoReservedRange] node within a message.
    /// Allows modification of the [ProtoReservedRange] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoReservedRange] being visited.
    fn end_visit_proto_reserved_range(&mut self, _node: &mut ProtoReservedRange) {}

    /// Called before visiting the children of a mutable [ProtoServiceDescriptor] (Protobuf Service) node.
    /// Allows modification of the [ProtoServiceDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoServiceDescriptor] being visited.
    fn start_visit_proto_service_descriptor(&mut self, _node: &mut ProtoServiceDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoServiceDescriptor] (Protobuf Service) node.
    /// Allows modification of the [ProtoServiceDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoServiceDescriptor] being visited.
    fn end_visit_proto_service_descriptor(&mut self, _node: &mut ProtoServiceDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoMethodDescriptor] (Protobuf Method) node.
    /// Allows modification of the [ProtoMethodDescriptor] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMethodDescriptor] being visited.
    fn start_visit_proto_method_descriptor(&mut self, _node: &mut ProtoMethodDescriptor) {}
    /// Called after visiting the children of a mutable [ProtoMethodDescriptor] (Protobuf Method) node.
    /// Allows modification of the [ProtoMethodDescriptor] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMethodDescriptor] being visited.
    fn end_visit_proto_method_descriptor(&mut self, _node: &mut ProtoMethodDescriptor) {}

    /// Called before visiting the children of a mutable [ProtoMethodOptions] node.
    /// Allows modification of the [ProtoMethodOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMethodOptions] being visited.
    fn start_visit_proto_method_options(&mut self, _node: &mut ProtoMethodOptions) {}
    /// Called after visiting the children of a mutable [ProtoMethodOptions] node.
    /// Allows modification of the [ProtoMethodOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoMethodOptions] being visited.
    fn end_visit_proto_method_options(&mut self, _node: &mut ProtoMethodOptions) {}

    /// Called before visiting the children of a mutable [ProtoServiceOptions] node.
    /// Allows modification of the [ProtoServiceOptions] before its children are visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoServiceOptions] being visited.
    fn start_visit_proto_service_options(&mut self, _node: &mut ProtoServiceOptions) {}
    /// Called after visiting the children of a mutable [ProtoServiceOptions] node.
    /// Allows modification of the [ProtoServiceOptions] after its children have been visited.
    ///
    /// # Parameters
    /// * `_node`: A mutable reference to the [ProtoServiceOptions] being visited.
    fn end_visit_proto_service_options(&mut self, _node: &mut ProtoServiceOptions) {}
}

/// Adds a `traverse` function to a visitor struct that implements [VsidlVisitorMut] and [Default].
///
/// This macro simplifies the process of creating a mutable visitor that processes an entire [Catalog].
/// It generates an associated function `traverse` for the specified type `$type_name`.
/// This function creates a default instance of the visitor, traverses the provided mutable `catalog`,
/// and returns the visitor instance (which may have collected information or **modified** the AST).
///
/// # Example
///
/// ```ignore
/// use vsidl_v1_parser::impl_traverse_catalog_mut;
/// use vsidl_v1_parser::VsidlVisitorMut;
/// use vsidl_v1_parser::Catalog;
///
/// #[derive(Default)]
/// struct MyMutVisitor { /* ... fields ... */ }
/// impl VsidlVisitorMut for MyMutVisitor { /* ... override visit methods ... */ }
/// impl_traverse_catalog_mut!(MyMutVisitor);
///
/// fn process_catalog_mut(catalog: &mut Catalog) {
///     let visitor_result = MyMutVisitor::traverse(catalog);
///     // Use visitor_result or inspect the modified catalog
/// }
/// ```
#[macro_export]
macro_rules! impl_traverse_catalog_mut {
    ($type_name:ident) => {
        impl $type_name {
            /// Creates a default visitor, traverses the entire catalog mutably, and returns the visitor.
            ///
            /// This function provides a convenient way to run a mutable visitor over the whole AST
            /// starting from the [Catalog] root. The visitor **can modify** the AST during traversal.
            ///
            /// # Parameters
            /// * `catalog`: A mutable reference to the [Catalog] to traverse.
            ///
            /// # Returns
            /// An instance of `$type_name` after it has visited the entire `catalog`.
            pub fn traverse(catalog: &mut $crate::Catalog) -> Self
            where
                Self: $crate::VsidlVisitorMut + Default,
            {
                let mut visitor = <$type_name>::default();
                catalog.accept(&mut visitor);
                visitor
            }
        }
    };
}

/// Implements the [AstNodeMut] trait for the [Catalog] struct.
impl AstNodeMut for Catalog {
    /// Accepts a [VsidlVisitorMut] and initiates the mutable traversal of the entire catalog.
    ///
    /// Calls `start_visit_catalog`, `traverse`, and `end_visit_catalog` on the visitor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_catalog(self);
        self.traverse(visitor);
        visitor.end_visit_catalog(self);
    }

    /// Defines the mutable traversal order for the children of a [Catalog].
    ///
    /// Visits [AndroidBpFile]s, then [ProtoFile]s, then [CatalogVsidlFile]s (main catalog VSIDL),
    /// and finally dependency [VsidlEntry]s. Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for android_bp in &mut self.android_bp_files {
            android_bp.accept(visitor);
        }

        for (metadata, file_descriptor) in &mut self.proto_descriptors {
            let mut proto_file: ProtoFile = (metadata, file_descriptor);
            proto_file.accept(visitor);
        }
        for (path, vsidl_entry) in &mut self.catalog_vsidl_files {
            let mut vsidl_file: CatalogVsidlFile = (path, vsidl_entry);
            vsidl_file.accept(visitor);
        }
        for vsidl_entry in &mut self.dependencies_vsidl_files {
            vsidl_entry.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [AndroidBpFile] struct.
impl AstNodeMut for AndroidBpFile {
    /// Accepts a [VsidlVisitorMut] for a mutable Android.bp file node.
    ///
    /// Calls `start_visit_android_bp_file`, `traverse`, and `end_visit_android_bp_file`.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_android_bp_file(self);
        self.traverse(visitor);
        visitor.end_visit_android_bp_file(self);
    }

    /// Traverses the children of a mutable [AndroidBpFile] node.
    ///
    /// Currently, [AndroidBpFile] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoFile] type alias.
impl AstNodeMut for ProtoFile<'_> {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf file node.
    ///
    /// Calls `start_visit_proto_file`, `traverse`, and `end_visit_proto_file`.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_file(self);
        self.traverse(visitor);
        visitor.end_visit_proto_file(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoFile].
    ///
    /// Visits the contained mutable [ProtoFileDescriptor]. Allows modification of the descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        self.1.accept(visitor);
    }
}

/// Implements the [AstNodeMut] trait for the [CatalogVsidlFile] type alias.
impl AstNodeMut for CatalogVsidlFile<'_> {
    /// Accepts a [VsidlVisitorMut] for a mutable VSIDL file node from the main catalog.
    ///
    /// Calls `start_visit_catalog_vsidl_file`, `traverse`, and `end_visit_catalog_vsidl_file`.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_catalog_vsidl_file(self);
        self.traverse(visitor);
        visitor.end_visit_catalog_vsidl_file(self);
    }

    /// Defines the mutable traversal order for the children of a [CatalogVsidlFile].
    ///
    /// Visits the contained mutable [VsidlEntry]. Allows modification of the entry.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        // Continue with VsidlEntry
        self.1.accept(visitor);
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoFileDescriptor] struct.
impl AstNodeMut for ProtoFileDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf file descriptor node.
    ///
    /// Calls `start_visit_file_descriptor_proto`, `traverse`, and `end_visit_file_descriptor_proto`.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_file_descriptor_proto(self);
        self.traverse(visitor);
        visitor.end_visit_file_descriptor_proto(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoFileDescriptor].
    ///
    /// Visits mutable messages, enums, services, and extensions defined within the file.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.message_type {
            child_node.accept(visitor);
        }
        for child_node in &mut self.enum_type {
            child_node.accept(visitor);
        }
        for child_node in &mut self.service {
            child_node.accept(visitor);
        }
        // options is not used here as it does not apply to Rust
        for child_node in &mut self.extension {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoDescriptor] (Protobuf Message) struct.
impl AstNodeMut for ProtoDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf message descriptor node.
    ///
    /// Calls `start_visit_proto_descriptor`. If the message has an [SdvPublication]
    /// annotation, it calls the appropriate `start_visit_proto_*_pub_message` convenience method.
    /// Then calls `traverse`. Finally, calls the corresponding `end_visit_proto_*_pub_message`
    /// (if applicable) and `end_visit_proto_descriptor`. Allows modification of the node.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        // In VSIDL, we mostly care about different types of publications.
        // Therefore, we add convenience functions here to make publications
        // easier to access in visitors.

        visitor.start_visit_proto_descriptor(self);

        if let Some(publication) = vsidl_v1::annotations::exts::publication.get(&self.options) {
            match publication.model.enum_value() {
                Ok(vsidl_v1::annotations::sdv_publication::PublisherModel::SINGLE_PUB) => {
                    visitor.start_visit_proto_single_pub_message(self, &publication);
                }
                Ok(vsidl_v1::annotations::sdv_publication::PublisherModel::MULTI_PUB) => {
                    visitor.start_visit_proto_multi_pub_message(self, &publication);
                }
                _ => {} // No PublisherModel
            }
        }

        self.traverse(visitor);

        if let Some(publication) = vsidl_v1::annotations::exts::publication.get(&self.options) {
            match publication.model.enum_value() {
                Ok(vsidl_v1::annotations::sdv_publication::PublisherModel::SINGLE_PUB) => {
                    visitor.end_visit_proto_single_pub_message(self, &publication);
                }
                Ok(vsidl_v1::annotations::sdv_publication::PublisherModel::MULTI_PUB) => {
                    visitor.end_visit_proto_multi_pub_message(self, &publication);
                }
                _ => {} // No PublisherModel
            }
        }

        visitor.end_visit_proto_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoDescriptor].
    ///
    /// Visits the mutable [SdvPublication] annotation (if present), fields, extensions,
    /// nested messages, nested enums, oneof declarations, message options, and reserved ranges.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let Some(mut sdv_publication) =
            vsidl_v1::annotations::exts::publication.get(&self.options)
        {
            sdv_publication.accept(visitor);
        }
        for child_node in &mut self.field {
            child_node.accept(visitor);
        }
        for child_node in &mut self.extension {
            child_node.accept(visitor);
        }
        for child_node in &mut self.nested_type {
            child_node.accept(visitor);
        }
        for child_node in &mut self.enum_type {
            child_node.accept(visitor);
        }
        // Extensions are not covered here, since they are not supported by Proto3
        for child_node in &mut self.oneof_decl {
            child_node.accept(visitor);
        }
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
        for child_node in &mut self.reserved_range {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SdvPublication] annotation struct.
impl AstNodeMut for SdvPublication {
    /// Accepts a [VsidlVisitorMut] for a mutable [SdvPublication] annotation node.
    ///
    /// Calls `start_visit_sdv_publication`, `traverse`, and `end_visit_sdv_publication`.
    /// Allows modification of the annotation.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_sdv_publication(self);
        self.traverse(visitor);
        visitor.end_visit_sdv_publication(self);
    }

    /// Traverses the children of a mutable [SdvPublication] node.
    ///
    /// Currently, [SdvPublication] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoFieldDescriptor] (Protobuf Field) struct.
impl AstNodeMut for ProtoFieldDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf field descriptor node.
    ///
    /// Calls `start_visit_proto_field_descriptor`, `traverse`, and `end_visit_proto_field_descriptor`.
    /// Allows modification of the field descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_field_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_field_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoFieldDescriptor].
    ///
    /// Visits the mutable field's options, if present. Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoFieldOptions] struct.
impl AstNodeMut for ProtoFieldOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf field options node.
    ///
    /// Calls `start_visit_proto_field_options`, `traverse`, and `end_visit_proto_field_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_field_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_field_options(self);
    }

    /// Traverses the children of a mutable [ProtoFieldOptions] node.
    ///
    /// Currently, [ProtoFieldOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoEnumDescriptor] (Protobuf Enum) struct.
impl AstNodeMut for ProtoEnumDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf enum descriptor node.
    ///
    /// Calls `start_visit_proto_enum_descriptor`, `traverse`, and `end_visit_proto_enum_descriptor`.
    /// Allows modification of the enum descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_enum_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoEnumDescriptor].
    ///
    /// Visits the mutable enum values, options (if present), and reserved ranges.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.value {
            child_node.accept(visitor);
        }
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
        for child_node in &mut self.reserved_range {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoEnumOptions] struct.
impl AstNodeMut for ProtoEnumOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf enum options node.
    ///
    /// Calls `start_visit_proto_enum_options`, `traverse`, and `end_visit_proto_enum_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_enum_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_options(self);
    }

    /// Traverses the children of a mutable [ProtoEnumOptions] node.
    ///
    /// Currently, [ProtoEnumOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoEnumReservedRange] struct.
impl AstNodeMut for ProtoEnumReservedRange {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf enum reserved range node.
    ///
    /// Calls `start_visit_proto_enum_reserved_range`, `traverse`, and `end_visit_proto_enum_reserved_range`.
    /// Allows modification of the range.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_enum_reserved_range(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_reserved_range(self);
    }

    /// Traverses the children of a mutable [ProtoEnumReservedRange] node.
    ///
    /// Currently, [ProtoEnumReservedRange] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoEnumValueDescriptor] (Protobuf Enum Value) struct.
impl AstNodeMut for ProtoEnumValueDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf enum value descriptor node.
    ///
    /// Calls `start_visit_proto_enum_value_descriptor`, `traverse`, and `end_visit_proto_enum_value_descriptor`.
    /// Allows modification of the enum value descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_enum_value_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_value_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoEnumValueDescriptor].
    ///
    /// Visits the mutable enum value's options, if present. Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoEnumValueOptions] struct.
impl AstNodeMut for ProtoEnumValueOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf enum value options node.
    ///
    /// Calls `start_visit_proto_enum_value_options`, `traverse`, and `end_visit_proto_enum_value_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_enum_value_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_value_options(self);
    }

    /// Traverses the children of a mutable [ProtoEnumValueOptions] node.
    ///
    /// Currently, [ProtoEnumValueOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoOneofDescriptor] (Protobuf Oneof) struct.
impl AstNodeMut for ProtoOneofDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf oneof descriptor node.
    ///
    /// Calls `start_visit_proto_oneof_descriptor`, `traverse`, and `end_visit_proto_oneof_descriptor`.
    /// Allows modification of the oneof descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_oneof_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_oneof_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoOneofDescriptor].
    ///
    /// Visits the mutable oneof's options, if present. Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoOneofOptions] struct.
impl AstNodeMut for ProtoOneofOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf oneof options node.
    ///
    /// Calls `start_visit_proto_oneof_options`, `traverse`, and `end_visit_proto_oneof_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_oneof_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_oneof_options(self);
    }

    /// Traverses the children of a mutable [ProtoOneofOptions] node.
    ///
    /// Currently, [ProtoOneofOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoMessageOptions] struct.
impl AstNodeMut for ProtoMessageOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf message options node.
    ///
    /// Calls `start_visit_proto_message_options`, `traverse`, and `end_visit_proto_message_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_message_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_message_options(self);
    }

    /// Traverses the children of a mutable [ProtoMessageOptions] node.
    ///
    /// Currently, [ProtoMessageOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoReservedRange] struct within a message.
impl AstNodeMut for ProtoReservedRange {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf message reserved range node.
    ///
    /// Calls `start_visit_proto_reserved_range`, `traverse`, and `end_visit_proto_reserved_range`.
    /// Allows modification of the range.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_reserved_range(self);
        self.traverse(visitor);
        visitor.end_visit_proto_reserved_range(self);
    }

    /// Traverses the children of a mutable [ProtoReservedRange] node.
    ///
    /// Currently, [ProtoReservedRange] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoServiceDescriptor] (Protobuf Service) struct.
impl AstNodeMut for ProtoServiceDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf service descriptor node.
    ///
    /// Calls `start_visit_proto_service_descriptor`, `traverse`, and `end_visit_proto_service_descriptor`.
    /// Allows modification of the service descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_service_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_service_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoServiceDescriptor].
    ///
    /// Visits the mutable methods and options (if present). Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.method {
            child_node.accept(visitor);
        }
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoMethodDescriptor] (Protobuf Method) struct.
impl AstNodeMut for ProtoMethodDescriptor {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf method descriptor node.
    ///
    /// Calls `start_visit_proto_method_descriptor`, `traverse`, and `end_visit_proto_method_descriptor`.
    /// Allows modification of the method descriptor.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_method_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_method_descriptor(self);
    }

    /// Defines the mutable traversal order for the children of a [ProtoMethodDescriptor].
    ///
    /// Visits the mutable method's options, if present. Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(options)) = &mut self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoMethodOptions] struct.
impl AstNodeMut for ProtoMethodOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf method options node.
    ///
    /// Calls `start_visit_proto_method_options`, `traverse`, and `end_visit_proto_method_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_method_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_method_options(self);
    }

    /// Traverses the children of a mutable [ProtoMethodOptions] node.
    ///
    /// Currently, [ProtoMethodOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [ProtoServiceOptions] struct.
impl AstNodeMut for ProtoServiceOptions {
    /// Accepts a [VsidlVisitorMut] for a mutable Protobuf service options node.
    ///
    /// Calls `start_visit_proto_service_options`, `traverse`, and `end_visit_proto_service_options`.
    /// Allows modification of the options.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_proto_service_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_service_options(self);
    }

    /// Traverses the children of a mutable [ProtoServiceOptions] node.
    ///
    /// Currently, [ProtoServiceOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNodeMut] trait for the [VsidlEntry] struct.
impl AstNodeMut for VsidlEntry {
    /// Accepts a [VsidlVisitorMut] for a mutable VSIDL entry node (root of a .vsidl file).
    ///
    /// Calls `start_visit_vsidl_entry`, `traverse`, and `end_visit_vsidl_entry`.
    /// Allows modification of the entry.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vsidl_entry(self);
        self.traverse(visitor);
        visitor.end_visit_vsidl_entry(self);
    }

    /// Defines the mutable traversal order for the children of a [VsidlEntry].
    ///
    /// Visits mutable service bundles, SOME/IP mappings, and VHAL mappings defined in the file.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.service_bundle {
            child_node.accept(visitor);
        }
        for child_node in &mut self.some_ip_mapping {
            child_node.accept(visitor);
        }
        for child_node in &mut self.vhal_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [ServiceBundle] struct.
impl AstNodeMut for ServiceBundle {
    /// Accepts a [VsidlVisitorMut] for a mutable service bundle node.
    ///
    /// Calls `start_visit_service_bundle`, `traverse`, and `end_visit_service_bundle`.
    /// Allows modification of the service bundle.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_service_bundle(self);
        self.traverse(visitor);
        visitor.end_visit_service_bundle(self);
    }

    /// Defines the mutable traversal order for the children of a [ServiceBundle].
    ///
    /// Visits mutable publishers, subscribers, servers, clients, owned unit types,
    /// diagnostics declarations (if present), and build configurations (if present).
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.publisher {
            child_node.accept(visitor);
        }
        for child_node in &mut self.subscriber {
            child_node.accept(visitor);
        }
        for child_node in &mut self.server {
            child_node.accept(visitor);
        }
        for child_node in &mut self.client {
            child_node.accept(visitor);
        }
        if let MessageField(Some(diagnostics_declaration)) = &mut self.diagnostics_declaration {
            diagnostics_declaration.accept(visitor);
        }
        if let MessageField(Some(build_cfg)) = &mut self.build_cfg {
            build_cfg.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [Publisher] struct.
impl AstNodeMut for Publisher {
    /// Accepts a [VsidlVisitorMut] for a mutable publisher node.
    ///
    /// Calls `start_visit_publisher`, `traverse`, and `end_visit_publisher`.
    /// Allows modification of the publisher.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_publisher(self);
        self.traverse(visitor);
        visitor.end_visit_publisher(self);
    }

    /// Traverses the children of a mutable [Publisher] node.
    ///
    /// Currently, [Publisher] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [Subscriber] struct.
impl AstNodeMut for Subscriber {
    /// Accepts a [VsidlVisitorMut] for a mutable subscriber node.
    ///
    /// Calls `start_visit_subscriber`, `traverse`, and `end_visit_subscriber`.
    /// Allows modification of the subscriber.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_subscriber(self);
        self.traverse(visitor);
        visitor.end_visit_subscriber(self);
    }

    /// Traverses the children of a mutable [Subscriber] node.
    ///
    /// Currently, [Subscriber] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [Server] struct.
impl AstNodeMut for Server {
    /// Accepts a [VsidlVisitorMut] for a mutable server node.
    ///
    /// Calls `start_visit_server`, `traverse`, and `end_visit_server`.
    /// Allows modification of the server.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_server(self);
        self.traverse(visitor);
        visitor.end_visit_server(self);
    }

    /// Traverses the children of a mutable [Server] node.
    ///
    /// Currently, [Server] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [Client] struct.
impl AstNodeMut for Client {
    /// Accepts a [VsidlVisitorMut] for a mutable client node.
    ///
    /// Calls `start_visit_client`, `traverse`, and `end_visit_client`.
    /// Allows modification of the client.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_client(self);
        self.traverse(visitor);
        visitor.end_visit_client(self);
    }

    /// Traverses the children of a mutable [Client] node.
    ///
    /// Currently, [Client] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [DiagnosticsDeclaration] struct.
impl AstNodeMut for DiagnosticsDeclaration {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics declaration node.
    ///
    /// Calls `start_visit_diagnostics`, `traverse`, and `end_visit_diagnostics`.
    /// Allows modification of the diagnostics declaration.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics(self);
    }

    /// Defines the mutable traversal order for the children of a [DiagnosticsDeclaration].
    ///
    /// Visits mutable data items, I/O controls, routines, and events.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.data_item {
            child_node.accept(visitor);
        }
        for child_node in &mut self.io_control {
            child_node.accept(visitor);
        }
        for child_node in &mut self.routine {
            child_node.accept(visitor);
        }
        for child_node in &mut self.event {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [DataItem] struct within Diagnostics.
impl AstNodeMut for DataItem {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics data item node.
    ///
    /// Calls `start_visit_diagnostics_data_item`, `traverse`, and `end_visit_diagnostics_data_item`.
    /// Allows modification of the data item.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics_data_item(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_data_item(self);
    }

    /// Traverses the children of a mutable [DataItem] node.
    ///
    /// Currently, [DataItem] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [InputOutputControlData] struct within Diagnostics.
impl AstNodeMut for InputOutputControlData {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics I/O control node.
    ///
    /// Calls `start_visit_diagnostics_io_control`, `traverse`, and `end_visit_diagnostics_io_control`.
    /// Allows modification of the I/O control data.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics_io_control(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_io_control(self);
    }

    /// Traverses the children of a mutable [InputOutputControlData] node.
    ///
    /// Currently, [InputOutputControlData] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [Routine] struct within Diagnostics.
impl AstNodeMut for Routine {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics routine node.
    ///
    /// Calls `start_visit_diagnostics_routine`, `traverse`, and `end_visit_diagnostics_routine`.
    /// Allows modification of the routine.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics_routine(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_routine(self);
    }

    /// Defines the mutable traversal order for the children of a [Routine].
    ///
    /// Visits the mutable start, stop, and result methods, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(start)) = &mut self.start {
            start.accept(visitor);
        }
        if let MessageField(Some(stop)) = &mut self.stop {
            stop.accept(visitor);
        }
        if let MessageField(Some(result)) = &mut self.result {
            result.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [Method] struct within a Diagnostics Routine.
impl AstNodeMut for Method {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics routine method node.
    ///
    /// Calls `start_visit_diagnostics_routine_method`, `traverse`, and `end_visit_diagnostics_routine_method`.
    /// Allows modification of the method.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics_routine_method(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_routine_method(self);
    }

    /// Traverses the children of a mutable [Method] node.
    ///
    /// Currently, [Method] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [Event] struct within Diagnostics.
impl AstNodeMut for Event {
    /// Accepts a [VsidlVisitorMut] for a mutable diagnostics event node.
    ///
    /// Calls `start_visit_diagnostics_event`, `traverse`, and `end_visit_diagnostics_event`.
    /// Allows modification of the event.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_diagnostics_event(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_event(self);
    }

    /// Traverses the children of a mutable [Event] node.
    ///
    /// Currently, [Event] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [BuildConfiguration] struct.
impl AstNodeMut for BuildConfiguration {
    /// Accepts a [VsidlVisitorMut] for a mutable build configuration node.
    ///
    /// Calls `start_visit_build_cfg`, `traverse`, and `end_visit_build_cfg`.
    /// Allows modification of the build configuration.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_build_cfg(self);
        self.traverse(visitor);
        visitor.end_visit_build_cfg(self);
    }

    /// Traverses the children of a mutable [BuildConfiguration] node.
    ///
    /// Currently, [BuildConfiguration] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpMapping] struct.
impl AstNodeMut for SomeIpMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP mapping node.
    ///
    /// Calls `start_visit_someip_mapping`, `traverse`, and `end_visit_someip_mapping`.
    /// Allows modification of the mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpMapping].
    ///
    /// Visits either the mutable [SomeIpServiceMapping] or the mutable [SomeIpSdvTypeMapping],
    /// depending on which one is present. Allows modification of the child node.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if self.has_someip_service_mapping() {
            self.mut_someip_service_mapping().accept(visitor);
        } else if self.has_sdv_type_mapping() {
            self.mut_sdv_type_mapping().accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpServiceMapping] struct.
impl AstNodeMut for SomeIpServiceMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP service mapping node.
    ///
    /// Calls `start_visit_someip_service_mapping`, `traverse`, and `end_visit_someip_service_mapping`.
    /// Allows modification of the service mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_service_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpServiceMapping].
    ///
    /// Visits mutable instance mappings and instance implementations.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.instance {
            child_node.accept(visitor);
        }
        for child_node in &mut self.instance_impl {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpInstanceMapping] struct.
impl AstNodeMut for SomeIpInstanceMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP instance mapping node.
    ///
    /// Calls `start_visit_someip_instance_mapping`, `traverse`, and `end_visit_someip_instance_mapping`.
    /// Allows modification of the instance mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_instance_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_instance_mapping(self);
    }

    /// Traverses the children of a mutable [SomeIpInstanceMapping] node.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for sdv_topic_parameter in self.sdv_topic_parameterization.values_mut() {
            sdv_topic_parameter.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SdvTopicParameters] struct.
impl AstNodeMut for SdvTopicParameters {
    /// Accepts a [VsidlVisitor] for a SOME/IP SdvTopicParameters node.
    ///
    /// Calls `start_visit_sdv_topic_parameters`, `traverse`, and `end_visit_sdv_topic_parameters`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_sdv_topic_parameters(self);
        self.traverse(visitor);
        visitor.end_visit_sdv_topic_parameters(self);
    }

    /// Traverses the children of a [SdvTopicParameters] node.
    ///
    /// Currently, [SdvTopicParameters] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpInstanceImplementation] struct.
impl AstNodeMut for SomeIpInstanceImplementation {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP instance implementation node.
    ///
    /// Calls `start_visit_someip_instance_implementation`, `traverse`, and `end_visit_someip_instance_implementation`.
    /// Allows modification of the instance implementation.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_instance_implementation(self);
        self.traverse(visitor);
        visitor.end_visit_someip_instance_implementation(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpInstanceImplementation].
    ///
    /// Visits the mutable offered RPC interface and offered event, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if self.has_offered_interface() {
            self.mut_offered_interface().accept(visitor);
        }
        if self.has_offered_event() {
            self.mut_offered_event().accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpRpcInterface] struct (within InstanceImplementation).
impl AstNodeMut for SomeIpRpcInterface {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP RPC interface node (within InstanceImplementation).
    ///
    /// Calls `start_visit_someip_rpc_interface`, `traverse`, and `end_visit_someip_rpc_interface`.
    /// Allows modification of the RPC interface.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_rpc_interface(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_interface(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpRpcInterface].
    ///
    /// Visits the mutable associated SDV unit type (if present) and methods.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(unit_type)) = &mut self.sdv_unit_type {
            unit_type.accept(visitor);
        }
        for child_node in &mut self.method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpTopic] struct.
impl AstNodeMut for SomeIpTopic {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP topic node.
    ///
    /// Calls `start_visit_someip_topic`, `traverse`, and `end_visit_someip_topic`.
    /// Allows modification of the topic.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_topic(self);
        self.traverse(visitor);
        visitor.end_visit_someip_topic(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpTopic].
    ///
    /// Visits the mutable type definition (if present) and the simple mapping (if present).
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(type_definition)) = &mut self.type_definition {
            type_definition.accept(visitor);
        }
        if self.has_simple_mapping() {
            self.mut_simple_mapping().accept(visitor);
        }
        // SomeipFfMethods is only a wrapper for bool
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpSimpleTopicMapping] struct.
impl AstNodeMut for SomeIpSimpleTopicMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP simple topic mapping node.
    ///
    /// Calls `start_visit_someip_simple_topic_mapping`, `traverse`, and `end_visit_someip_simple_topic_mapping`.
    /// Allows modification of the mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_simple_topic_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_simple_topic_mapping(self);
    }

    /// Traverses the children of a mutable [SomeIpSimpleTopicMapping] node.
    ///
    /// Currently, [SomeIpSimpleTopicMapping] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpUnitType] struct.
impl AstNodeMut for SomeIpUnitType {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP unit type reference node.
    ///
    /// Calls `start_visit_someip_unit_type`, `traverse`, and `end_visit_someip_unit_type`.
    /// Allows modification of the unit type reference.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_unit_type(self);
        self.traverse(visitor);
        visitor.end_visit_someip_unit_type(self);
    }

    /// Traverses the children of a mutable [SomeIpUnitType] node.
    ///
    /// Currently, [SomeIpUnitType] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpRequestResponseMethod] struct.
impl AstNodeMut for SomeIpRequestResponseMethod {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP request-response method node.
    ///
    /// Calls `start_visit_someip_request_response_method`, `traverse`, and `end_visit_someip_request_response_method`.
    /// Allows modification of the method.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_request_response_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_request_response_method(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpRequestResponseMethod].
    ///
    /// Visits the mutable request and response payload mappings, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(request)) = &mut self.request_payload_mapping {
            request.accept(visitor);
        }
        if let MessageField(Some(response)) = &mut self.response_payload_mapping {
            response.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpEvent] struct.
impl AstNodeMut for SomeIpEvent {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP event node.
    ///
    /// Calls `start_visit_someip_event`, `traverse`, and `end_visit_someip_event`.
    /// Allows modification of the event.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_event(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpEvent].
    ///
    /// Visits either the mutable simple event or the mutable group event, depending on which one is present.
    /// Allows modification of the child node.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if self.has_simple_event() {
            self.mut_simple_event().accept(visitor);
        } else if self.has_group_event() {
            self.mut_group_event().accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpSimpleEvent] struct.
impl AstNodeMut for SomeIpSimpleEvent {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP simple event node.
    ///
    /// Calls `start_visit_someip_simple_event`, `traverse`, and `end_visit_someip_simple_event`.
    /// Allows modification of the simple event.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_simple_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_simple_event(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpSimpleEvent].
    ///
    /// Visits the mutable associated SDV unit type and the type definition, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(sdv_unit_type)) = &mut self.sdv_unit_type {
            sdv_unit_type.accept(visitor);
        }
        if let MessageField(Some(type_definition)) = &mut self.type_definition {
            type_definition.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpGroupEvent] struct.
impl AstNodeMut for SomeIpGroupEvent {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP group event node.
    ///
    /// Calls `start_visit_someip_group_event`, `traverse`, and `end_visit_someip_group_event`.
    /// Allows modification of the group event.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_group_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_group_event(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpGroupEvent].
    ///
    /// Visits the mutable type definition (if present) and the event parts.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(type_definition)) = &mut self.type_definition {
            type_definition.accept(visitor);
        }
        for child_node in &mut self.event_part {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpGroupEventPart] struct.
impl AstNodeMut for SomeIpGroupEventPart {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP group event part node.
    ///
    /// Calls `start_visit_someip_group_event_part`, `traverse`, and `end_visit_someip_group_event_part`.
    /// Allows modification of the event part.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_group_event_part(self);
        self.traverse(visitor);
        visitor.end_visit_someip_group_event_part(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpGroupEventPart].
    ///
    /// Visits the mutable associated SDV unit type, if present. Allows modification of the unit type.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(sdv_unit_type)) = &mut self.sdv_unit_type {
            sdv_unit_type.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpStruct] struct.
impl AstNodeMut for SomeIpStruct {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP struct definition node.
    ///
    /// Calls `start_visit_someip_struct`, `traverse`, and `end_visit_someip_struct`.
    /// Allows modification of the struct definition.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_struct(self);
        self.traverse(visitor);
        visitor.end_visit_someip_struct(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpStruct].
    ///
    /// Visits the mutable struct fields. Allows modification of the fields.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.field {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpStructField] struct.
impl AstNodeMut for SomeIpStructField {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP struct field node.
    ///
    /// Calls `start_visit_someip_struct_field`, `traverse`, and `end_visit_someip_struct_field`.
    /// Allows modification of the struct field.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_struct_field(self);
        self.traverse(visitor);
        visitor.end_visit_someip_struct_field(self);
    }

    /// Traverses the children of a mutable [SomeIpStructField] node.
    ///
    /// Currently, [SomeIpStructField] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpSdvTypeMapping] struct.
impl AstNodeMut for SomeIpSdvTypeMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP SDV type mapping node.
    ///
    /// Calls `start_visit_someip_sdv_type_mapping`, `traverse`, and `end_visit_someip_sdv_type_mapping`.
    /// Allows modification of the type mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_sdv_type_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_sdv_type_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpSdvTypeMapping].
    ///
    /// Visits the mutable associated unit type (if present), service unit mappings,
    /// offered RPC interface (if present), and offered topic (if present).
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(unit_type)) = &mut self.unit_type {
            unit_type.accept(visitor);
        }
        for child_node in &mut self.service_unit_mapping {
            child_node.accept(visitor);
        }
        if self.has_offered_interface() {
            self.mut_offered_interface().accept(visitor);
        }
        if self.has_offered_topic() {
            self.mut_offered_topic().accept(visitor);
        }
        if let MessageField(Some(service_oriented_mapping)) = &mut self.service_oriented_mapping {
            service_oriented_mapping.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpServiceOrientedMapping] struct.
impl AstNodeMut for SomeIpServiceOrientedMapping {
    /// Accepts a [VsidlVisitorMut] for a SOME/IP service-oriented mapping node.
    ///
    /// Calls `start_visit_someip_service_oriented_mapping`, `traverse`, and `end_visit_someip_service_oriented_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_service_oriented_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_oriented_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpServiceOrientedMapping] mutably.
    ///
    /// Visits either the RPC interface mapping or the publication mapping, depending on which one is present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        if self.has_rpc_interface_mapping() {
            self.mut_rpc_interface_mapping().accept(visitor);
        } else if self.has_publication_mapping() {
            self.mut_publication_mapping().accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpRpcInterfaceMapping] struct.
impl AstNodeMut for SomeIpRpcInterfaceMapping {
    /// Accepts a [VsidlVisitorMut] for a SOME/IP RPC interface mapping node.
    ///
    /// Calls `start_visit_someip_rpc_interface_mapping`, `traverse`, and `end_visit_someip_rpc_interface_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_rpc_interface_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_interface_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpRpcInterfaceMapping] mutably.
    ///
    /// Visits the offered interface and the RPC channel mappings.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(offered_interface)) = &mut self.offered_interface {
            offered_interface.accept(visitor);
        }
        for child_node in &mut self.rpc_channel_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpRpcChannelMapping] struct.
impl AstNodeMut for SomeIpRpcChannelMapping {
    /// Accepts a [VsidlVisitorMut] for a SOME/IP RPC channel mapping node.
    ///
    /// Calls `start_visit_someip_rpc_channel_mapping`, `traverse`, and `end_visit_someip_rpc_channel_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_rpc_channel_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_channel_mapping(self);
    }

    /// Traverses the children of a [SomeIpRpcChannelMapping] node mutably.
    ///
    /// Currently, [SomeIpRpcChannelMapping] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&'_ mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpPublicationMapping] struct.
impl AstNodeMut for SomeIpPublicationMapping {
    /// Accepts a [VsidlVisitorMut] for a SOME/IP publication mapping node.
    ///
    /// Calls `start_visit_someip_publication_mapping`, `traverse`, and `end_visit_someip_publication_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_publication_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_publication_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpPublicationMapping] mutably.
    ///
    /// Visits the offered topic and the topic mappings.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(offered_topic)) = &mut self.offered_topic {
            offered_topic.accept(visitor);
        }
        for child_node in &mut self.topic_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpTopicMapping] struct.
impl AstNodeMut for SomeIpTopicMapping {
    /// Accepts a [VsidlVisitorMut] for a SOME/IP topic mapping node.
    ///
    /// Calls `start_visit_someip_topic_mapping`, `traverse`, and `end_visit_someip_topic_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_topic_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_topic_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpTopicMapping] mutably.
    ///
    /// Visits the fire and forget methods, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&'_ mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.fire_and_forget_method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpMappingRpcInterface] struct (within SdvTypeMapping).
impl AstNodeMut for SomeIpMappingRpcInterface {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP RPC interface node (within SdvTypeMapping).
    ///
    /// Calls `start_visit_someip_type_mapping_rpc_interface`, `traverse`, and `end_visit_someip_type_mapping_rpc_interface`.
    /// Allows modification of the RPC interface.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_type_mapping_rpc_interface(self);
        self.traverse(visitor);
        visitor.end_visit_someip_type_mapping_rpc_interface(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpMappingRpcInterface].
    ///
    /// Visits the mutable methods defined in the interface. Allows modification of the methods.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpMappingRpcInterfaceMethod] struct.
impl AstNodeMut for SomeIpMappingRpcInterfaceMethod {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP RPC interface method node (within SdvTypeMapping).
    ///
    /// Calls `start_visit_someip_type_mapping_rpc_interface_method`, `traverse`, and `end_visit_someip_type_mapping_rpc_interface_method`.
    /// Allows modification of the method.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_type_mapping_rpc_interface_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_type_mapping_rpc_interface_method(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpMappingRpcInterfaceMethod].
    ///
    /// Visits the mutable request and response payload mappings, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(request)) = &mut self.request_payload_mapping {
            request.accept(visitor);
        }
        if let MessageField(Some(response)) = &mut self.response_payload_mapping {
            response.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpSdvServiceUnitMapping] struct.
impl AstNodeMut for SomeIpSdvServiceUnitMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP SDV service unit mapping node.
    ///
    /// Calls `start_visit_someip_sdv_service_unit_mapping`, `traverse`, and `end_visit_someip_sdv_service_unit_mapping`.
    /// Allows modification of the service unit mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_sdv_service_unit_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_sdv_service_unit_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [SomeIpSdvServiceUnitMapping].
    ///
    /// Visits the mutable service unit ID (if present) and any fire-and-forget methods.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(service_unit_id)) = &mut self.service_unit_id {
            service_unit_id.accept(visitor);
        }
        for child_node in &mut self.fire_and_forget_method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpServiceUnitId] struct.
impl AstNodeMut for SomeIpServiceUnitId {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP service unit ID node.
    ///
    /// Calls `start_visit_someip_service_unit_id`, `traverse`, and `end_visit_someip_service_unit_id`.
    /// Allows modification of the service unit ID.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_service_unit_id(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_unit_id(self);
    }

    /// Traverses the children of a mutable [SomeIpServiceUnitId] node.
    ///
    /// Currently, [SomeIpServiceUnitId] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [SomeIpFFMethod] (Fire and Forget) struct.
impl AstNodeMut for SomeIpFFMethod {
    /// Accepts a [VsidlVisitorMut] for a mutable SOME/IP fire-and-forget method node.
    ///
    /// Calls `start_visit_someip_ff_method`, `traverse`, and `end_visit_someip_ff_method`.
    /// Allows modification of the method.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_someip_ff_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_ff_method(self);
    }

    /// Traverses the children of a mutable [SomeIpFFMethod] node.
    ///
    /// Currently, [SomeIpFFMethod] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // No child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [VhalMapping] struct.
impl AstNodeMut for VhalMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL mapping node.
    ///
    /// Calls `start_visit_vhal_mapping`, `traverse`, and `end_visit_vhal_mapping`.
    /// Allows modification of the mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [VhalMapping].
    ///
    /// Visits the mutable vehicle property (if present), the data source (if present), and area mappings.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(property)) = &mut self.property {
            property.accept(visitor);
        }
        if let MessageField(Some(source)) = &mut self.source {
            source.accept(visitor);
        }
        for child_node in &mut self.area_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [VhalVehicleProperty] struct.
impl AstNodeMut for VhalVehicleProperty {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL vehicle property node.
    ///
    /// Calls `start_visit_vhal_vehicle_property`, `traverse`, and `end_visit_vhal_vehicle_property`.
    /// Allows modification of the vehicle property.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_vehicle_property(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_property(self);
    }

    /// Defines the mutable traversal order for the children of a [VhalVehicleProperty].
    ///
    /// Visits the mutable property config, default value, and default area config, if present.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(prop_config)) = &mut self.prop_config {
            prop_config.accept(visitor);
        }
        if let MessageField(Some(default_value)) = &mut self.default_value {
            default_value.accept(visitor);
        }
        if let MessageField(Some(default_area_config)) = &mut self.default_area_config {
            default_area_config.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [VhalVehiclePropConfig] struct.
impl AstNodeMut for VhalVehiclePropConfig {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL vehicle property config node.
    ///
    /// Calls `start_visit_vhal_vehicle_prop_config`, `traverse`, and `end_visit_vhal_vehicle_prop_config`.
    /// Allows modification of the property config.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_vehicle_prop_config(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_prop_config(self);
    }

    /// Defines the mutable traversal order for the children of a [VhalVehiclePropConfig].
    ///
    /// Visits the mutable area configs defined within the property config.
    /// Allows modification of child nodes.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to child nodes.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        for child_node in &mut self.area_configs {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [VhalDefaultValue] struct.
impl AstNodeMut for VhalDefaultValue {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL default value node.
    ///
    /// Calls `start_visit_vhal_default_value`, `traverse`, and `end_visit_vhal_default_value`.
    /// Allows modification of the default value.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_default_value(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_default_value(self);
    }

    /// Traverses the children of a mutable [VhalDefaultValue] node.
    ///
    /// Currently, [VhalDefaultValue] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [VhalVehicleAreaConfig] struct.
impl AstNodeMut for VhalVehicleAreaConfig {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL vehicle area config node.
    ///
    /// Calls `start_visit_vhal_vehicle_area_config`, `traverse`, and `end_visit_vhal_vehicle_area_config`.
    /// Allows modification of the area config.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_vehicle_area_config(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_area_config(self);
    }

    /// Traverses the children of a mutable [VhalVehicleAreaConfig] node.
    ///
    /// Currently, [VhalVehicleAreaConfig] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [VhalDataSource] struct.
impl AstNodeMut for VhalDataSource {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL data source node.
    ///
    /// Calls `start_visit_vhal_data_source`, `traverse`, and `end_visit_vhal_data_source`.
    /// Allows modification of the data source.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_data_source(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_data_source(self);
    }

    /// Traverses the children of a mutable [VhalDataSource] node.
    ///
    /// Currently, [VhalDataSource] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The mutable visitor (unused).
    fn traverse(&mut self, _visitor: &mut dyn VsidlVisitorMut) {
        // no child nodes
    }
}

/// Implements the [AstNodeMut] trait for the [VhalAreaMapping] struct.
impl AstNodeMut for VhalAreaMapping {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL area mapping node.
    ///
    /// Calls `start_visit_vhal_area_mapping`, `traverse`, and `end_visit_vhal_area_mapping`.
    /// Allows modification of the area mapping.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_area_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_area_mapping(self);
    }

    /// Defines the mutable traversal order for the children of a [VhalAreaMapping].
    ///
    /// Visits the mutable property area definition, if present. Allows modification of the property area.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(area)) = &mut self.area {
            area.accept(visitor);
        }
    }
}

/// Implements the [AstNodeMut] trait for the [VhalPropertyArea] struct.
impl AstNodeMut for VhalPropertyArea {
    /// Accepts a [VsidlVisitorMut] for a mutable VHAL property area node.
    ///
    /// Calls `start_visit_vhal_property_area`, `traverse`, and `end_visit_vhal_property_area`.
    /// Allows modification of the property area.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to accept.
    fn accept(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        visitor.start_visit_vhal_property_area(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_property_area(self);
    }

    /// Defines the mutable traversal order for the children of a [VhalPropertyArea].
    ///
    /// Visits the mutable area config, if present. Allows modification of the area config.
    ///
    /// # Parameters
    /// * `visitor`: The mutable visitor to pass to the child node.
    fn traverse(&mut self, visitor: &mut dyn VsidlVisitorMut) {
        if let MessageField(Some(area_config)) = &mut self.area_config {
            area_config.accept(visitor);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_data;
    use crate::visitors::visitor_mut::AstNodeMut;
    use crate::visitors::visitor_mut::MockVsidlVisitorMut;
    use crate::Catalog;
    use crate::VsidlVisitorMut;

    /// Empty visitor. Only exists because Automock is not detected by coverage reports.
    pub struct ManualMock;
    impl VsidlVisitorMut for ManualMock {}

    #[test]
    /// Verifies that the mutable visitor infrastructure correctly traverses all defined AST node types.
    ///
    /// This test uses a mock visitor ([MockVsidlVisitorMut]) and a specially crafted catalog
    /// ([test_data::VSIDL_ALL_AST_NODE_TYPES], [test_data::ALL_AST_NODE_TYPES_PROTO])
    /// that includes instances of every possible node type defined in the visitor trait.
    /// It sets expectations on the mock that each `start_visit_*` and `end_visit_*` method
    /// is called at least once. If the traversal logic in the `AstNodeMut` implementations
    /// is incomplete or incorrect, the mock will panic, failing the test.
    fn test_visitor_visits_every_ast_node() {
        // GIVEN a mock that expects every visit function to be called at least once
        let mut mock = MockVsidlVisitorMut::new();
        mock.expect_start_visit_catalog().times(1..).return_const(());
        mock.expect_end_visit_catalog().times(1..).return_const(());
        mock.expect_start_visit_android_bp_file().times(1..).return_const(());
        mock.expect_end_visit_android_bp_file().times(1..).return_const(());
        mock.expect_start_visit_catalog_vsidl_file().times(1..).return_const(());
        mock.expect_end_visit_catalog_vsidl_file().times(1..).return_const(());
        mock.expect_start_visit_vsidl_entry().times(1..).return_const(());
        mock.expect_end_visit_vsidl_entry().times(1..).return_const(());
        mock.expect_start_visit_service_bundle().times(1..).return_const(());
        mock.expect_end_visit_service_bundle().times(1..).return_const(());
        mock.expect_start_visit_publisher().times(1..).return_const(());
        mock.expect_end_visit_publisher().times(1..).return_const(());
        mock.expect_start_visit_subscriber().times(1..).return_const(());
        mock.expect_end_visit_subscriber().times(1..).return_const(());
        mock.expect_start_visit_server().times(1..).return_const(());
        mock.expect_end_visit_server().times(1..).return_const(());
        mock.expect_start_visit_client().times(1..).return_const(());
        mock.expect_end_visit_client().times(1..).return_const(());
        mock.expect_start_visit_diagnostics().times(1..).return_const(());
        mock.expect_end_visit_diagnostics().times(1..).return_const(());
        mock.expect_start_visit_diagnostics_data_item().times(1..).return_const(());
        mock.expect_end_visit_diagnostics_data_item().times(1..).return_const(());
        mock.expect_start_visit_diagnostics_io_control().times(1..).return_const(());
        mock.expect_end_visit_diagnostics_io_control().times(1..).return_const(());
        mock.expect_start_visit_diagnostics_routine().times(1..).return_const(());
        mock.expect_end_visit_diagnostics_routine().times(1..).return_const(());
        mock.expect_start_visit_diagnostics_routine_method().times(1..).return_const(());
        mock.expect_end_visit_diagnostics_routine_method().times(1..).return_const(());
        mock.expect_start_visit_diagnostics_event().times(1..).return_const(());
        mock.expect_end_visit_diagnostics_event().times(1..).return_const(());
        mock.expect_start_visit_build_cfg().times(1..).return_const(());
        mock.expect_end_visit_build_cfg().times(1..).return_const(());

        // SOME IP Methods
        mock.expect_start_visit_someip_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_service_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_service_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_instance_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_instance_mapping().times(1..).return_const(());
        mock.expect_start_visit_sdv_topic_parameters().times(1..).return_const(());
        mock.expect_end_visit_sdv_topic_parameters().times(1..).return_const(());
        mock.expect_start_visit_someip_instance_implementation().times(1..).return_const(());
        mock.expect_end_visit_someip_instance_implementation().times(1..).return_const(());
        mock.expect_start_visit_someip_rpc_interface().times(1..).return_const(());
        mock.expect_end_visit_someip_rpc_interface().times(1..).return_const(());
        mock.expect_start_visit_someip_type_mapping_rpc_interface().times(1..).return_const(());
        mock.expect_end_visit_someip_type_mapping_rpc_interface().times(1..).return_const(());
        mock.expect_start_visit_someip_type_mapping_rpc_interface_method()
            .times(1..)
            .return_const(());
        mock.expect_end_visit_someip_type_mapping_rpc_interface_method()
            .times(1..)
            .return_const(());
        mock.expect_start_visit_someip_topic().times(1..).return_const(());
        mock.expect_end_visit_someip_topic().times(1..).return_const(());
        mock.expect_start_visit_someip_simple_topic_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_simple_topic_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_unit_type().times(1..).return_const(());
        mock.expect_end_visit_someip_unit_type().times(1..).return_const(());
        mock.expect_start_visit_someip_request_response_method().times(1..).return_const(());
        mock.expect_end_visit_someip_request_response_method().times(1..).return_const(());
        mock.expect_start_visit_someip_event().times(1..).return_const(());
        mock.expect_end_visit_someip_event().times(1..).return_const(());
        mock.expect_start_visit_someip_simple_event().times(1..).return_const(());
        mock.expect_end_visit_someip_simple_event().times(1..).return_const(());
        mock.expect_start_visit_someip_group_event().times(1..).return_const(());
        mock.expect_end_visit_someip_group_event().times(1..).return_const(());
        mock.expect_start_visit_someip_group_event_part().times(1..).return_const(());
        mock.expect_end_visit_someip_group_event_part().times(1..).return_const(());
        mock.expect_start_visit_someip_struct().times(1..).return_const(());
        mock.expect_end_visit_someip_struct().times(1..).return_const(());
        mock.expect_start_visit_someip_struct_field().times(1..).return_const(());
        mock.expect_end_visit_someip_struct_field().times(1..).return_const(());
        mock.expect_start_visit_someip_sdv_type_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_sdv_type_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_sdv_service_unit_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_sdv_service_unit_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_service_unit_id().times(1..).return_const(());
        mock.expect_end_visit_someip_service_unit_id().times(1..).return_const(());
        mock.expect_start_visit_someip_ff_method().times(1..).return_const(());
        mock.expect_end_visit_someip_ff_method().times(1..).return_const(());
        mock.expect_start_visit_someip_service_oriented_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_service_oriented_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_rpc_interface_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_rpc_interface_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_rpc_channel_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_rpc_channel_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_publication_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_publication_mapping().times(1..).return_const(());
        mock.expect_start_visit_someip_topic_mapping().times(1..).return_const(());
        mock.expect_end_visit_someip_topic_mapping().times(1..).return_const(());

        // VHAL methods
        mock.expect_start_visit_vhal_mapping().times(1..).return_const(());
        mock.expect_end_visit_vhal_mapping().times(1..).return_const(());
        mock.expect_start_visit_vhal_vehicle_property().times(1..).return_const(());
        mock.expect_end_visit_vhal_vehicle_property().times(1..).return_const(());
        mock.expect_start_visit_vhal_vehicle_prop_config().times(1..).return_const(());
        mock.expect_end_visit_vhal_vehicle_prop_config().times(1..).return_const(());
        mock.expect_start_visit_vhal_default_value().times(1..).return_const(());
        mock.expect_end_visit_vhal_default_value().times(1..).return_const(());
        mock.expect_start_visit_vhal_vehicle_area_config().times(1..).return_const(());
        mock.expect_end_visit_vhal_vehicle_area_config().times(1..).return_const(());
        mock.expect_start_visit_vhal_data_source().times(1..).return_const(());
        mock.expect_end_visit_vhal_data_source().times(1..).return_const(());
        mock.expect_start_visit_vhal_area_mapping().times(1..).return_const(());
        mock.expect_end_visit_vhal_area_mapping().times(1..).return_const(());
        mock.expect_start_visit_vhal_property_area().times(1..).return_const(());
        mock.expect_end_visit_vhal_property_area().times(1..).return_const(());

        // PROTO methods
        mock.expect_start_visit_proto_file().times(1..).return_const(());
        mock.expect_end_visit_proto_file().times(1..).return_const(());
        mock.expect_start_visit_file_descriptor_proto().times(1..).return_const(());
        mock.expect_end_visit_file_descriptor_proto().times(1..).return_const(());
        mock.expect_start_visit_proto_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_descriptor().times(1..).return_const(());
        mock.expect_start_visit_sdv_publication().times(1..).return_const(());
        mock.expect_end_visit_sdv_publication().times(1..).return_const(());
        mock.expect_start_visit_proto_single_pub_message().times(1..).return_const(());
        mock.expect_end_visit_proto_single_pub_message().times(1..).return_const(());
        mock.expect_start_visit_proto_multi_pub_message().times(1..).return_const(());
        mock.expect_end_visit_proto_multi_pub_message().times(1..).return_const(());
        mock.expect_start_visit_proto_field_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_field_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_field_options().times(1..).return_const(());
        mock.expect_end_visit_proto_field_options().times(1..).return_const(());
        mock.expect_start_visit_proto_enum_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_enum_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_enum_options().times(1..).return_const(());
        mock.expect_end_visit_proto_enum_options().times(1..).return_const(());
        mock.expect_start_visit_proto_enum_reserved_range().times(1..).return_const(());
        mock.expect_end_visit_proto_enum_reserved_range().times(1..).return_const(());
        mock.expect_start_visit_proto_enum_value_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_enum_value_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_enum_value_options().times(1..).return_const(());
        mock.expect_end_visit_proto_enum_value_options().times(1..).return_const(());
        mock.expect_start_visit_proto_oneof_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_oneof_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_oneof_options().times(1..).return_const(());
        mock.expect_end_visit_proto_oneof_options().times(1..).return_const(());
        mock.expect_start_visit_proto_message_options().times(1..).return_const(());
        mock.expect_end_visit_proto_message_options().times(1..).return_const(());
        mock.expect_start_visit_proto_reserved_range().times(1..).return_const(());
        mock.expect_end_visit_proto_reserved_range().times(1..).return_const(());
        mock.expect_start_visit_proto_service_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_service_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_method_descriptor().times(1..).return_const(());
        mock.expect_end_visit_proto_method_descriptor().times(1..).return_const(());
        mock.expect_start_visit_proto_method_options().times(1..).return_const(());
        mock.expect_end_visit_proto_method_options().times(1..).return_const(());
        mock.expect_start_visit_proto_service_options().times(1..).return_const(());
        mock.expect_end_visit_proto_service_options().times(1..).return_const(());

        // GIVEN a catalog that parses into an AST that contains all possible types of AST nodes
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::ALL_AST_NODE_TYPES_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(test_data::VSIDL_ALL_AST_NODE_TYPES.clone());
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_ALL_AST_NODE_TYPES.1.clone());
        catalog.android_bp_files.push(test_data::EMPTY_ANDROID_BP.clone());

        // GIVEN another mock of the visitor
        let mut manual_mock = ManualMock;

        // WHEN visiting the catalog's AST with the Mock
        catalog.accept(&mut mock);
        catalog.accept(&mut manual_mock);

        // THEN all methods should have been called
        // AutoMock will panic if this is not the case
    }
}
