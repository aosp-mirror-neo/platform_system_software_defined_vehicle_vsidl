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
//! This crate provides a visitor infrastructure for VSIDL

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

/// Couples proto file metadata and the file descriptor proto itself.
///
/// This allows visitors to access both the content ([ProtoFileDescriptor]) and
/// associated build metadata ([ProtoBuildTargetMetadata]) during traversal.
pub type ProtoFile = (ProtoBuildTargetMetadata, ProtoFileDescriptor);

/// Couples a VSIDL file from the main catalog (not a dependency) with its relative path.
///
/// This allows visitors to know the origin file path ([PathBuf]) for a given
/// VSIDL entry ([VsidlEntry]) within the catalog being processed.
pub type CatalogVsidlFile = (PathBuf, VsidlEntry);

/// Defines the contract for visiting nodes in a VSIDL Abstract Syntax Tree (AST).
///
/// Implementors of this trait can traverse the AST structure derived from
/// `.vsidl`, `.proto`, and `Android.bp` files.
pub trait AstNode {
    /// Accepts a visitor, triggering the appropriate visit methods.
    ///
    /// The implementation should typically follow this pattern:
    /// 1. Call the corresponding `start_visit_*` method on the `visitor`.
    /// 2. Call `self.traverse(visitor)` to visit child nodes.
    /// 3. Call the corresponding `end_visit_*` method on the `visitor`.
    ///
    /// # Parameters
    ///
    /// * `visitor`: A mutable reference to an object implementing [VsidlVisitor].
    fn accept(&self, visitor: &mut dyn VsidlVisitor);

    /// Traverses the child nodes of the current AST node.
    ///
    /// This method defines the order in which child nodes are visited.
    /// It should call `accept` on each child node, passing the `visitor`.
    /// If a node has no children, this method should have an empty implementation.
    ///
    /// # Parameters
    ///
    /// * `visitor`: A mutable reference to an object implementing [VsidlVisitor].
    fn traverse(&self, visitor: &mut dyn VsidlVisitor);
}

/// A helper trait to enable casting concrete visitor types to `&mut dyn VsidlVisitor`.
///
/// Rust's trait objects (`dyn Trait`) are unsized, meaning their size isn't known at compile time.
/// This trait provides a bridge to obtain a trait object reference from a sized type `T`
/// that implements [VsidlVisitor].
pub trait AsDynVisitor {
    /// Returns a mutable trait object reference to the [VsidlVisitor].
    fn as_mut_dyn_visitor(&mut self) -> &mut dyn VsidlVisitor;
}

/// Blanket implementation of [AsDynVisitor] for any type `T` that implements [VsidlVisitor].
impl<T: VsidlVisitor> AsDynVisitor for T {
    /// Returns `self` cast as a mutable trait object reference.
    fn as_mut_dyn_visitor(&mut self) -> &mut dyn VsidlVisitor {
        self
    }
}

/// Defines the visitor pattern interface for traversing the VSIDL AST.
///
/// This trait provides `start_visit_*` and `end_visit_*` methods for each type of node
/// in the AST (derived from VSIDL, Protobuf definitions, and Android.bp files).
/// Concrete visitors implement this trait, overriding only the methods for the
/// node types they are interested in. The default implementations do nothing.
///
/// The `#[automock]` attribute enables automatic mock generation for testing purposes using the `mockall` crate.
#[automock]
pub trait VsidlVisitor: AsDynVisitor {
    /// Called before visiting the children of a [Catalog] node.
    ///
    /// # Parameters
    /// * `_catalog`: A reference to the [Catalog] being visited.
    fn start_visit_catalog(&mut self, _catalog: &Catalog) {}
    /// Called after visiting the children of a [Catalog] node.
    ///
    /// # Parameters
    /// * `_catalog`: A reference to the [Catalog] being visited.
    fn end_visit_catalog(&mut self, _catalog: &Catalog) {}

    /// Called before visiting the children of an [AndroidBpFile] node.
    ///
    /// # Parameters
    /// * `_android_bp_file`: A reference to the [AndroidBpFile] being visited.
    fn start_visit_android_bp_file(&mut self, _android_bp_file: &AndroidBpFile) {}
    /// Called after visiting the children of an [AndroidBpFile] node.
    ///
    /// # Parameters
    /// * `_android_bp_file`: A reference to the [AndroidBpFile] being visited.
    fn end_visit_android_bp_file(&mut self, _android_bp_file: &AndroidBpFile) {}

    /// Called before visiting the children of a [ProtoFile] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFile] tuple being visited.
    fn start_visit_proto_file(&mut self, _node: &ProtoFile) {}
    /// Called after visiting the children of a [ProtoFile] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFile] tuple being visited.
    fn end_visit_proto_file(&mut self, _node: &ProtoFile) {}

    /// Called before visiting the children of a [ProtoFileDescriptor] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFileDescriptor] being visited.
    fn start_visit_file_descriptor_proto(&mut self, _node: &ProtoFileDescriptor) {}
    /// Called after visiting the children of a [ProtoFileDescriptor] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFileDescriptor] being visited.
    fn end_visit_file_descriptor_proto(&mut self, _node: &ProtoFileDescriptor) {}

    /// Called before visiting the children of a [CatalogVsidlFile] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [CatalogVsidlFile] tuple being visited.
    fn start_visit_catalog_vsidl_file(&mut self, _node: &CatalogVsidlFile) {}
    /// Called after visiting the children of a [CatalogVsidlFile] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [CatalogVsidlFile] tuple being visited.
    fn end_visit_catalog_vsidl_file(&mut self, _node: &CatalogVsidlFile) {}

    /// Called before visiting the children of a [VsidlEntry] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VsidlEntry] being visited.
    fn start_visit_vsidl_entry(&mut self, _node: &VsidlEntry) {}
    /// Called after visiting the children of a [VsidlEntry] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VsidlEntry] being visited.
    fn end_visit_vsidl_entry(&mut self, _node: &VsidlEntry) {}

    /// Called before visiting the children of a [ServiceBundle] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ServiceBundle] being visited.
    fn start_visit_service_bundle(&mut self, _node: &ServiceBundle) {}
    /// Called after visiting the children of a [ServiceBundle] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ServiceBundle] being visited.
    fn end_visit_service_bundle(&mut self, _node: &ServiceBundle) {}

    /// Called before visiting the children of a [Publisher] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Publisher] being visited.
    fn start_visit_publisher(&mut self, _node: &Publisher) {}
    /// Called after visiting the children of a [Publisher] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Publisher] being visited.
    fn end_visit_publisher(&mut self, _node: &Publisher) {}

    /// Called before visiting the children of a [Subscriber] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Subscriber] being visited.
    fn start_visit_subscriber(&mut self, _node: &Subscriber) {}
    /// Called after visiting the children of a [Subscriber] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Subscriber] being visited.
    fn end_visit_subscriber(&mut self, _node: &Subscriber) {}

    /// Called before visiting the children of a [Server] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Server] being visited.
    fn start_visit_server(&mut self, _node: &Server) {}
    /// Called after visiting the children of a [Server] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Server] being visited.
    fn end_visit_server(&mut self, _node: &Server) {}

    /// Called before visiting the children of a [Client] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Client] being visited.
    fn start_visit_client(&mut self, _node: &Client) {}
    /// Called after visiting the children of a [Client] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Client] being visited.
    fn end_visit_client(&mut self, _node: &Client) {}

    /// Called before visiting the children of a [DiagnosticsDeclaration] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [DiagnosticsDeclaration] being visited.
    fn start_visit_diagnostics(&mut self, _node: &DiagnosticsDeclaration) {}
    /// Called after visiting the children of a [DiagnosticsDeclaration] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [DiagnosticsDeclaration] being visited.
    fn end_visit_diagnostics(&mut self, _node: &DiagnosticsDeclaration) {}

    /// Called before visiting the children of a [DataItem] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [DataItem] being visited.
    fn start_visit_diagnostics_data_item(&mut self, _node: &DataItem) {}
    /// Called after visiting the children of a [DataItem] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [DataItem] being visited.
    fn end_visit_diagnostics_data_item(&mut self, _node: &DataItem) {}

    /// Called before visiting the children of an [InputOutputControlData] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [InputOutputControlData] being visited.
    fn start_visit_diagnostics_io_control(&mut self, _node: &InputOutputControlData) {}
    /// Called after visiting the children of an [InputOutputControlData] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [InputOutputControlData] being visited.
    fn end_visit_diagnostics_io_control(&mut self, _node: &InputOutputControlData) {}

    /// Called before visiting the children of a [Routine] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Routine] being visited.
    fn start_visit_diagnostics_routine(&mut self, _node: &Routine) {}
    /// Called after visiting the children of a [Routine] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Routine] being visited.
    fn end_visit_diagnostics_routine(&mut self, _node: &Routine) {}

    /// Called before visiting the children of a [Method] node within a Diagnostics Routine.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Method] being visited.
    fn start_visit_diagnostics_routine_method(&mut self, _node: &Method) {}
    /// Called after visiting the children of a [Method] node within a Diagnostics Routine.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Method] being visited.
    fn end_visit_diagnostics_routine_method(&mut self, _node: &Method) {}

    /// Called before visiting the children of an [Event] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Event] being visited.
    fn start_visit_diagnostics_event(&mut self, _node: &Event) {}
    /// Called after visiting the children of an [Event] node within Diagnostics.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [Event] being visited.
    fn end_visit_diagnostics_event(&mut self, _node: &Event) {}

    /// Called before visiting the children of a [BuildConfiguration] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [BuildConfiguration] being visited.
    fn start_visit_build_cfg(&mut self, _node: &BuildConfiguration) {}
    /// Called after visiting the children of a [BuildConfiguration] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [BuildConfiguration] being visited.
    fn end_visit_build_cfg(&mut self, _node: &BuildConfiguration) {}

    /// Called before visiting the children of a [SomeIpMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMapping] being visited.
    fn start_visit_someip_mapping(&mut self, _node: &SomeIpMapping) {}
    /// Called after visiting the children of a [SomeIpMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMapping] being visited.
    fn end_visit_someip_mapping(&mut self, _node: &SomeIpMapping) {}

    /// Called before visiting the children of a [SomeIpServiceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceMapping] being visited.
    fn start_visit_someip_service_mapping(&mut self, _node: &SomeIpServiceMapping) {}
    /// Called after visiting the children of a [SomeIpServiceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceMapping] being visited.
    fn end_visit_someip_service_mapping(&mut self, _node: &SomeIpServiceMapping) {}

    /// Called before visiting the children of a [SdvTopicParameters] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SdvTopicParameters] being visited.
    fn start_visit_sdv_topic_parameters(&mut self, _node: &SdvTopicParameters) {}
    /// Called after visiting the children of a [SdvTopicParameters] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SdvTopicParameters] being visited.
    fn end_visit_sdv_topic_parameters(&mut self, _node: &SdvTopicParameters) {}

    /// Called before visiting the children of a [SomeIpInstanceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpInstanceMapping] being visited.
    fn start_visit_someip_instance_mapping(&mut self, _node: &SomeIpInstanceMapping) {}
    /// Called after visiting the children of a [SomeIpInstanceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpInstanceMapping] being visited.
    fn end_visit_someip_instance_mapping(&mut self, _node: &SomeIpInstanceMapping) {}

    /// Called before visiting the children of a [SomeIpInstanceImplementation] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpInstanceImplementation] being visited.
    fn start_visit_someip_instance_implementation(&mut self, _node: &SomeIpInstanceImplementation) {
    }
    /// Called after visiting the children of a [SomeIpInstanceImplementation] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpInstanceImplementation] being visited.
    fn end_visit_someip_instance_implementation(&mut self, _node: &SomeIpInstanceImplementation) {}

    /// Called before visiting the children of a [SomeIpRpcInterface] node (within InstanceImplementation).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcInterface] being visited.
    fn start_visit_someip_rpc_interface(&mut self, _node: &SomeIpRpcInterface) {}
    /// Called after visiting the children of a [SomeIpRpcInterface] node (within InstanceImplementation).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcInterface] being visited.
    fn end_visit_someip_rpc_interface(&mut self, _node: &SomeIpRpcInterface) {}

    /// Called before visiting the children of a [SomeIpMappingRpcInterface] node (within SdvTypeMapping).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMappingRpcInterface] being visited.
    fn start_visit_someip_type_mapping_rpc_interface(&mut self, _node: &SomeIpMappingRpcInterface) {
    }
    /// Called after visiting the children of a [SomeIpMappingRpcInterface] node (within SdvTypeMapping).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMappingRpcInterface] being visited.
    fn end_visit_someip_type_mapping_rpc_interface(&mut self, _node: &SomeIpMappingRpcInterface) {}

    /// Called before visiting the children of a [SomeIpMappingRpcInterfaceMethod] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMappingRpcInterfaceMethod] being visited.
    fn start_visit_someip_type_mapping_rpc_interface_method(
        &mut self,
        _node: &SomeIpMappingRpcInterfaceMethod,
    ) {
    }
    /// Called after visiting the children of a [SomeIpMappingRpcInterfaceMethod] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpMappingRpcInterfaceMethod] being visited.
    fn end_visit_someip_type_mapping_rpc_interface_method(
        &mut self,
        _node: &SomeIpMappingRpcInterfaceMethod,
    ) {
    }

    /// Called before visiting the children of a [SomeIpTopic] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpTopic] being visited.
    fn start_visit_someip_topic(&mut self, _node: &SomeIpTopic) {}
    /// Called after visiting the children of a [SomeIpTopic] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpTopic] being visited.
    fn end_visit_someip_topic(&mut self, _node: &SomeIpTopic) {}

    /// Called before visiting the children of a [SomeIpSimpleTopicMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSimpleTopicMapping] being visited.
    fn start_visit_someip_simple_topic_mapping(&mut self, _node: &SomeIpSimpleTopicMapping) {}
    /// Called after visiting the children of a [SomeIpSimpleTopicMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSimpleTopicMapping] being visited.
    fn end_visit_someip_simple_topic_mapping(&mut self, _node: &SomeIpSimpleTopicMapping) {}

    /// Called before visiting the children of a [SomeIpUnitType] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpUnitType] being visited.
    fn start_visit_someip_unit_type(&mut self, _node: &SomeIpUnitType) {}
    /// Called after visiting the children of a [SomeIpUnitType] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpUnitType] being visited.
    fn end_visit_someip_unit_type(&mut self, _node: &SomeIpUnitType) {}

    /// Called before visiting the children of a [SomeIpRequestResponseMethod] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRequestResponseMethod] being visited.
    fn start_visit_someip_request_response_method(&mut self, _node: &SomeIpRequestResponseMethod) {}
    /// Called after visiting the children of a [SomeIpRequestResponseMethod] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRequestResponseMethod] being visited.
    fn end_visit_someip_request_response_method(&mut self, _node: &SomeIpRequestResponseMethod) {}

    /// Called before visiting the children of a [SomeIpEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpEvent] being visited.
    fn start_visit_someip_event(&mut self, _node: &SomeIpEvent) {}
    /// Called after visiting the children of a [SomeIpEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpEvent] being visited.
    fn end_visit_someip_event(&mut self, _node: &SomeIpEvent) {}

    /// Called before visiting the children of a [SomeIpSimpleEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSimpleEvent] being visited.
    fn start_visit_someip_simple_event(&mut self, _node: &SomeIpSimpleEvent) {}
    /// Called after visiting the children of a [SomeIpSimpleEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSimpleEvent] being visited.
    fn end_visit_someip_simple_event(&mut self, _node: &SomeIpSimpleEvent) {}

    /// Called before visiting the children of a [SomeIpGroupEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpGroupEvent] being visited.
    fn start_visit_someip_group_event(&mut self, _node: &SomeIpGroupEvent) {}
    /// Called after visiting the children of a [SomeIpGroupEvent] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpGroupEvent] being visited.
    fn end_visit_someip_group_event(&mut self, _node: &SomeIpGroupEvent) {}

    /// Called before visiting the children of a [SomeIpGroupEventPart] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpGroupEventPart] being visited.
    fn start_visit_someip_group_event_part(&mut self, _node: &SomeIpGroupEventPart) {}
    /// Called after visiting the children of a [SomeIpGroupEventPart] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpGroupEventPart] being visited.
    fn end_visit_someip_group_event_part(&mut self, _node: &SomeIpGroupEventPart) {}

    /// Called before visiting the children of a [SomeIpStruct] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpStruct] being visited.
    fn start_visit_someip_struct(&mut self, _node: &SomeIpStruct) {}
    /// Called after visiting the children of a [SomeIpStruct] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpStruct] being visited.
    fn end_visit_someip_struct(&mut self, _node: &SomeIpStruct) {}

    /// Called before visiting the children of a [SomeIpStructField] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpStructField] being visited.
    fn start_visit_someip_struct_field(&mut self, _node: &SomeIpStructField) {}
    /// Called after visiting the children of a [SomeIpStructField] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpStructField] being visited.
    fn end_visit_someip_struct_field(&mut self, _node: &SomeIpStructField) {}

    /// Called before visiting the children of a [SomeIpSdvTypeMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSdvTypeMapping] being visited.
    fn start_visit_someip_sdv_type_mapping(&mut self, _node: &SomeIpSdvTypeMapping) {}
    /// Called after visiting the children of a [SomeIpSdvTypeMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSdvTypeMapping] being visited.
    fn end_visit_someip_sdv_type_mapping(&mut self, _node: &SomeIpSdvTypeMapping) {}

    /// Called before visiting the children of a [SomeIpSdvServiceUnitMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSdvServiceUnitMapping] being visited.
    fn start_visit_someip_sdv_service_unit_mapping(&mut self, _node: &SomeIpSdvServiceUnitMapping) {
    }
    /// Called after visiting the children of a [SomeIpSdvServiceUnitMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpSdvServiceUnitMapping] being visited.
    fn end_visit_someip_sdv_service_unit_mapping(&mut self, _node: &SomeIpSdvServiceUnitMapping) {}

    /// Called before visiting the children of a [SomeIpServiceUnitId] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceUnitId] being visited.
    fn start_visit_someip_service_unit_id(&mut self, _node: &SomeIpServiceUnitId) {}
    /// Called after visiting the children of a [SomeIpServiceUnitId] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceUnitId] being visited.
    fn end_visit_someip_service_unit_id(&mut self, _node: &SomeIpServiceUnitId) {}

    /// Called before visiting the children of a [SomeIpFFMethod] node (Fire and Forget).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpFFMethod] being visited.
    fn start_visit_someip_ff_method(&mut self, _node: &SomeIpFFMethod) {}
    /// Called after visiting the children of a [SomeIpFFMethod] node (Fire and Forget).
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpFFMethod] being visited.
    fn end_visit_someip_ff_method(&mut self, _node: &SomeIpFFMethod) {}

    /// Called before visiting the children of a [SomeIpServiceOrientedMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceOrientedMapping] being visited.
    fn start_visit_someip_service_oriented_mapping(
        &mut self,
        _node: &SomeIpServiceOrientedMapping,
    ) {
    }
    /// Called after visiting the children of a [SomeIpServiceOrientedMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpServiceOrientedMapping] being visited.
    fn end_visit_someip_service_oriented_mapping(&mut self, _node: &SomeIpServiceOrientedMapping) {}

    /// Called before visiting the children of a [SomeIpRpcInterfaceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcInterfaceMapping] being visited.
    fn start_visit_someip_rpc_interface_mapping(&mut self, _node: &SomeIpRpcInterfaceMapping) {}
    /// Called after visiting the children of a [SomeIpRpcInterfaceMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcInterfaceMapping] being visited.
    fn end_visit_someip_rpc_interface_mapping(&mut self, _node: &SomeIpRpcInterfaceMapping) {}

    /// Called before visiting the children of a [SomeIpRpcChannelMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcChannelMapping] being visited.
    fn start_visit_someip_rpc_channel_mapping(&mut self, _node: &SomeIpRpcChannelMapping) {}
    /// Called after visiting the children of a [SomeIpRpcChannelMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpRpcChannelMapping] being visited.
    fn end_visit_someip_rpc_channel_mapping(&mut self, _node: &SomeIpRpcChannelMapping) {}

    /// Called before visiting the children of a [SomeIpPublicationMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpPublicationMapping] being visited.
    fn start_visit_someip_publication_mapping(&mut self, _node: &SomeIpPublicationMapping) {}
    /// Called after visiting the children of a [SomeIpPublicationMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpPublicationMapping] being visited.
    fn end_visit_someip_publication_mapping(&mut self, _node: &SomeIpPublicationMapping) {}

    /// Called before visiting the children of a [SomeIpTopicMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpTopicMapping] being visited.
    fn start_visit_someip_topic_mapping(&mut self, _node: &SomeIpTopicMapping) {}
    /// Called after visiting the children of a [SomeIpTopicMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SomeIpTopicMapping] being visited.
    fn end_visit_someip_topic_mapping(&mut self, _node: &SomeIpTopicMapping) {}

    /// Called before visiting the children of a [VhalMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalMapping] being visited.
    fn start_visit_vhal_mapping(&mut self, _node: &VhalMapping) {}
    /// Called after visiting the children of a [VhalMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalMapping] being visited.
    fn end_visit_vhal_mapping(&mut self, _node: &VhalMapping) {}

    /// Called before visiting the children of a [VhalVehicleProperty] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehicleProperty] being visited.
    fn start_visit_vhal_vehicle_property(&mut self, _node: &VhalVehicleProperty) {}
    /// Called after visiting the children of a [VhalVehicleProperty] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehicleProperty] being visited.
    fn end_visit_vhal_vehicle_property(&mut self, _node: &VhalVehicleProperty) {}

    /// Called before visiting the children of a [VhalVehiclePropConfig] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehiclePropConfig] being visited.
    fn start_visit_vhal_vehicle_prop_config(&mut self, _node: &VhalVehiclePropConfig) {}
    /// Called after visiting the children of a [VhalVehiclePropConfig] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehiclePropConfig] being visited.
    fn end_visit_vhal_vehicle_prop_config(&mut self, _node: &VhalVehiclePropConfig) {}

    /// Called before visiting the children of a [VhalDefaultValue] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalDefaultValue] being visited.
    fn start_visit_vhal_default_value(&mut self, _node: &VhalDefaultValue) {}
    /// Called after visiting the children of a [VhalDefaultValue] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalDefaultValue] being visited.
    fn end_visit_vhal_default_value(&mut self, _node: &VhalDefaultValue) {}

    /// Called before visiting the children of a [VhalVehicleAreaConfig] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehicleAreaConfig] being visited.
    fn start_visit_vhal_vehicle_area_config(&mut self, _node: &VhalVehicleAreaConfig) {}
    /// Called after visiting the children of a [VhalVehicleAreaConfig] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalVehicleAreaConfig] being visited.
    fn end_visit_vhal_vehicle_area_config(&mut self, _node: &VhalVehicleAreaConfig) {}

    /// Called before visiting the children of a [VhalDataSource] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalDataSource] being visited.
    fn start_visit_vhal_data_source(&mut self, _node: &VhalDataSource) {}
    /// Called after visiting the children of a [VhalDataSource] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalDataSource] being visited.
    fn end_visit_vhal_data_source(&mut self, _node: &VhalDataSource) {}

    /// Called before visiting the children of a [VhalAreaMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalAreaMapping] being visited.
    fn start_visit_vhal_area_mapping(&mut self, _node: &VhalAreaMapping) {}
    /// Called after visiting the children of a [VhalAreaMapping] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalAreaMapping] being visited.
    fn end_visit_vhal_area_mapping(&mut self, _node: &VhalAreaMapping) {}

    /// Called before visiting the children of a [VhalPropertyArea] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalPropertyArea] being visited.
    fn start_visit_vhal_property_area(&mut self, _node: &VhalPropertyArea) {}
    /// Called after visiting the children of a [VhalPropertyArea] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [VhalPropertyArea] being visited.
    fn end_visit_vhal_property_area(&mut self, _node: &VhalPropertyArea) {}

    /// Called before visiting the children of a [ProtoDescriptor] (Protobuf Message) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoDescriptor] being visited.
    fn start_visit_proto_descriptor(&mut self, _node: &ProtoDescriptor) {}
    /// Called after visiting the children of a [ProtoDescriptor] (Protobuf Message) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoDescriptor] being visited.
    fn end_visit_proto_descriptor(&mut self, _node: &ProtoDescriptor) {}

    /// Called before visiting the children of an [SdvPublication] annotation node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SdvPublication] being visited.
    fn start_visit_sdv_publication(&mut self, _node: &SdvPublication) {}
    /// Called after visiting the children of an [SdvPublication] annotation node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [SdvPublication] being visited.
    fn end_visit_sdv_publication(&mut self, _node: &SdvPublication) {}

    /// Convenience method called before visiting children of a Protobuf message
    /// identified as a SINGLE_PUB publication via the [SdvPublication] annotation.
    /// This is called *after* [start_visit_proto_descriptor].
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
    ///
    /// # Parameters
    /// * `_message`: The [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn start_visit_proto_multi_pub_message(
        &mut self,
        _message: &ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }
    /// Convenience method called after visiting children of a Protobuf message
    /// identified as a MULTI_PUB publication.
    /// This is called *before* [end_visit_proto_descriptor].
    ///
    /// # Parameters
    /// * `_message`: The [ProtoDescriptor] of the message.
    /// * `_publication`: The [SdvPublication] annotation data.
    fn end_visit_proto_multi_pub_message(
        &mut self,
        _message: &ProtoDescriptor,
        _publication: &SdvPublication,
    ) {
    }

    /// Called before visiting the children of a [ProtoFieldDescriptor] (Protobuf Field) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFieldDescriptor] being visited.
    fn start_visit_proto_field_descriptor(&mut self, _node: &ProtoFieldDescriptor) {}
    /// Called after visiting the children of a [ProtoFieldDescriptor] (Protobuf Field) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFieldDescriptor] being visited.
    fn end_visit_proto_field_descriptor(&mut self, _node: &ProtoFieldDescriptor) {}

    /// Called before visiting the children of a [ProtoFieldOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFieldOptions] being visited.
    fn start_visit_proto_field_options(&mut self, _node: &ProtoFieldOptions) {}
    /// Called after visiting the children of a [ProtoFieldOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoFieldOptions] being visited.
    fn end_visit_proto_field_options(&mut self, _node: &ProtoFieldOptions) {}

    /// Called before visiting the children of a [ProtoEnumDescriptor] (Protobuf Enum) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumDescriptor] being visited.
    fn start_visit_proto_enum_descriptor(&mut self, _node: &ProtoEnumDescriptor) {}
    /// Called after visiting the children of a [ProtoEnumDescriptor] (Protobuf Enum) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumDescriptor] being visited.
    fn end_visit_proto_enum_descriptor(&mut self, _node: &ProtoEnumDescriptor) {}

    /// Called before visiting the children of a [ProtoEnumOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumOptions] being visited.
    fn start_visit_proto_enum_options(&mut self, _node: &ProtoEnumOptions) {}
    /// Called after visiting the children of a [ProtoEnumOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumOptions] being visited.
    fn end_visit_proto_enum_options(&mut self, _node: &ProtoEnumOptions) {}

    /// Called before visiting the children of a [ProtoEnumReservedRange] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumReservedRange] being visited.
    fn start_visit_proto_enum_reserved_range(&mut self, _node: &ProtoEnumReservedRange) {}
    /// Called after visiting the children of a [ProtoEnumReservedRange] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumReservedRange] being visited.
    fn end_visit_proto_enum_reserved_range(&mut self, _node: &ProtoEnumReservedRange) {}

    /// Called before visiting the children of a [ProtoEnumValueDescriptor] (Protobuf Enum Value) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumValueDescriptor] being visited.
    fn start_visit_proto_enum_value_descriptor(&mut self, _node: &ProtoEnumValueDescriptor) {}
    /// Called after visiting the children of a [ProtoEnumValueDescriptor] (Protobuf Enum Value) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumValueDescriptor] being visited.
    fn end_visit_proto_enum_value_descriptor(&mut self, _node: &ProtoEnumValueDescriptor) {}

    /// Called before visiting the children of a [ProtoEnumValueOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumValueOptions] being visited.
    fn start_visit_proto_enum_value_options(&mut self, _node: &ProtoEnumValueOptions) {}
    /// Called after visiting the children of a [ProtoEnumValueOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoEnumValueOptions] being visited.
    fn end_visit_proto_enum_value_options(&mut self, _node: &ProtoEnumValueOptions) {}

    /// Called before visiting the children of a [ProtoOneofDescriptor] (Protobuf Oneof) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoOneofDescriptor] being visited.
    fn start_visit_proto_oneof_descriptor(&mut self, _node: &ProtoOneofDescriptor) {}
    /// Called after visiting the children of a [ProtoOneofDescriptor] (Protobuf Oneof) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoOneofDescriptor] being visited.
    fn end_visit_proto_oneof_descriptor(&mut self, _node: &ProtoOneofDescriptor) {}

    /// Called before visiting the children of a [ProtoOneofOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoOneofOptions] being visited.
    fn start_visit_proto_oneof_options(&mut self, _node: &ProtoOneofOptions) {}
    /// Called after visiting the children of a [ProtoOneofOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoOneofOptions] being visited.
    fn end_visit_proto_oneof_options(&mut self, _node: &ProtoOneofOptions) {}

    /// Called before visiting the children of a [ProtoMessageOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMessageOptions] being visited.
    fn start_visit_proto_message_options(&mut self, _node: &ProtoMessageOptions) {}
    /// Called after visiting the children of a [ProtoMessageOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMessageOptions] being visited.
    fn end_visit_proto_message_options(&mut self, _node: &ProtoMessageOptions) {}

    /// Called before visiting the children of a [ProtoReservedRange] node within a message.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoReservedRange] being visited.
    fn start_visit_proto_reserved_range(&mut self, _node: &ProtoReservedRange) {}
    /// Called after visiting the children of a [ProtoReservedRange] node within a message.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoReservedRange] being visited.
    fn end_visit_proto_reserved_range(&mut self, _node: &ProtoReservedRange) {}

    /// Called before visiting the children of a [ProtoServiceDescriptor] (Protobuf Service) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoServiceDescriptor] being visited.
    fn start_visit_proto_service_descriptor(&mut self, _node: &ProtoServiceDescriptor) {}
    /// Called after visiting the children of a [ProtoServiceDescriptor] (Protobuf Service) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoServiceDescriptor] being visited.
    fn end_visit_proto_service_descriptor(&mut self, _node: &ProtoServiceDescriptor) {}

    /// Called before visiting the children of a [ProtoMethodDescriptor] (Protobuf Method) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMethodDescriptor] being visited.
    fn start_visit_proto_method_descriptor(&mut self, _node: &ProtoMethodDescriptor) {}
    /// Called after visiting the children of a [ProtoMethodDescriptor] (Protobuf Method) node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMethodDescriptor] being visited.
    fn end_visit_proto_method_descriptor(&mut self, _node: &ProtoMethodDescriptor) {}

    /// Called before visiting the children of a [ProtoMethodOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMethodOptions] being visited.
    fn start_visit_proto_method_options(&mut self, _node: &ProtoMethodOptions) {}
    /// Called after visiting the children of a [ProtoMethodOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoMethodOptions] being visited.
    fn end_visit_proto_method_options(&mut self, _node: &ProtoMethodOptions) {}

    /// Called before visiting the children of a [ProtoServiceOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoServiceOptions] being visited.
    fn start_visit_proto_service_options(&mut self, _node: &ProtoServiceOptions) {}
    /// Called after visiting the children of a [ProtoServiceOptions] node.
    ///
    /// # Parameters
    /// * `_node`: A reference to the [ProtoServiceOptions] being visited.
    fn end_visit_proto_service_options(&mut self, _node: &ProtoServiceOptions) {}
}

/// Adds a `traverse` function to a visitor struct that implements [VsidlVisitor] and [Default].
///
/// This macro simplifies the process of creating a visitor that processes an entire [Catalog].
/// It generates an associated function `traverse` for the specified type `$type_name`.
/// This function creates a default instance of the visitor, traverses the provided `catalog`,
/// and returns the visitor instance (which may have collected information during traversal).
///
/// # Example
///
/// ```ignore
/// use vsidl_v1_parser::impl_traverse_catalog;
/// use vsidl_v1_parser::VsidlVisitor;
/// use vsidl_v1_parser::Catalog;
///
/// #[derive(Default)]
/// struct MyVisitor { /* ... fields to collect data ... */ }
/// impl VsidlVisitor for MyVisitor { /* ... override visit methods ... */ }
/// impl_traverse_catalog!(MyVisitor);
///
/// fn process_catalog(catalog: &Catalog) {
///     let visitor_result = MyVisitor::traverse(catalog);
///     // Use the data collected in visitor_result
/// }
/// ```
#[macro_export]
macro_rules! impl_traverse_catalog {
    ($type_name:ident) => {
        impl $type_name {
            /// Creates a default visitor, traverses the entire catalog, and returns the visitor.
            ///
            /// This function provides a convenient way to run a visitor over the whole AST
            /// starting from the [Catalog] root.
            ///
            /// # Parameters
            /// * `catalog`: A reference to the [Catalog] to traverse.
            ///
            /// # Returns
            /// An instance of `$type_name` after it has visited the entire `catalog`.
            pub fn traverse(catalog: &$crate::Catalog) -> Self
            where
                Self: $crate::VsidlVisitor + Default,
            {
                let mut visitor = <$type_name>::default();
                catalog.accept(&mut visitor);
                visitor
            }
        }
    };
}

/// Implements the [AstNode] trait for the [Catalog] struct.
impl AstNode for Catalog {
    /// Accepts a [VsidlVisitor] and initiates the traversal of the entire catalog.
    ///
    /// Calls `start_visit_catalog`, `traverse`, and `end_visit_catalog` on the visitor.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_catalog(self);
        self.traverse(visitor);
        visitor.end_visit_catalog(self);
    }

    /// Defines the traversal order for the children of a [Catalog].
    ///
    /// Visits [AndroidBpFile]s, then [ProtoFile]s, then [CatalogVsidlFile]s (main catalog VSIDL),
    /// and finally dependency [VsidlEntry]s.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for android_bp in &self.android_bp_files {
            android_bp.accept(visitor);
        }

        for (metadata, file_descriptor) in &self.proto_descriptors {
            let proto_file: ProtoFile = (metadata.clone(), file_descriptor.clone());
            proto_file.accept(visitor);
        }
        for (path, vsidl_entry) in &self.catalog_vsidl_files {
            let vsidl_file: CatalogVsidlFile = (path.clone(), vsidl_entry.clone());
            vsidl_file.accept(visitor);
        }
        for vsidl_entry in &self.dependencies_vsidl_files {
            vsidl_entry.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [AndroidBpFile] struct.
impl AstNode for AndroidBpFile {
    /// Accepts a [VsidlVisitor] for an Android.bp file node.
    ///
    /// Calls `start_visit_android_bp_file`, `traverse`, and `end_visit_android_bp_file`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_android_bp_file(self);
        self.traverse(visitor);
        visitor.end_visit_android_bp_file(self);
    }

    /// Traverses the children of an [AndroidBpFile] node.
    ///
    /// Currently, [AndroidBpFile] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [ProtoFile] type alias.
impl AstNode for ProtoFile {
    /// Accepts a [VsidlVisitor] for a Protobuf file node.
    ///
    /// Calls `start_visit_proto_file`, `traverse`, and `end_visit_proto_file`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_file(self);
        self.traverse(visitor);
        visitor.end_visit_proto_file(self);
    }

    /// Defines the traversal order for the children of a [ProtoFile].
    ///
    /// Visits the contained [ProtoFileDescriptor].
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        self.1.accept(visitor);
    }
}

/// Implements the [AstNode] trait for the [CatalogVsidlFile] type alias.
impl AstNode for CatalogVsidlFile {
    /// Accepts a [VsidlVisitor] for a VSIDL file node from the main catalog.
    ///
    /// Calls `start_visit_catalog_vsidl_file`, `traverse`, and `end_visit_catalog_vsidl_file`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_catalog_vsidl_file(self);
        self.traverse(visitor);
        visitor.end_visit_catalog_vsidl_file(self);
    }

    /// Defines the traversal order for the children of a [CatalogVsidlFile].
    ///
    /// Visits the contained [VsidlEntry].
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        // Continue with VsidlEntry
        self.1.accept(visitor);
    }
}

/// Implements the [AstNode] trait for the [ProtoFileDescriptor] struct.
impl AstNode for ProtoFileDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf file descriptor node.
    ///
    /// Calls `start_visit_file_descriptor_proto`, `traverse`, and `end_visit_file_descriptor_proto`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_file_descriptor_proto(self);
        self.traverse(visitor);
        visitor.end_visit_file_descriptor_proto(self);
    }

    /// Defines the traversal order for the children of a [ProtoFileDescriptor].
    ///
    /// Visits messages, enums, services, and extensions defined within the file.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.message_type {
            child_node.accept(visitor);
        }
        for child_node in &self.enum_type {
            child_node.accept(visitor);
        }
        for child_node in &self.service {
            child_node.accept(visitor);
        }
        // options is not used here as it does not apply to Rust
        for child_node in &self.extension {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoDescriptor] (Protobuf Message) struct.
impl AstNode for ProtoDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf message descriptor node.
    ///
    /// Calls `start_visit_proto_descriptor`. If the message has an [SdvPublication]
    /// annotation, it calls the appropriate `start_visit_proto_*_pub_message` convenience method.
    /// Then calls `traverse`. Finally, calls the corresponding `end_visit_proto_*_pub_message`
    /// (if applicable) and `end_visit_proto_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
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

    /// Defines the traversal order for the children of a [ProtoDescriptor].
    ///
    /// Visits the [SdvPublication] annotation (if present), fields, extensions,
    /// nested messages, nested enums, oneof declarations, message options, and reserved ranges.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let Some(sdv_publication) = vsidl_v1::annotations::exts::publication.get(&self.options) {
            sdv_publication.accept(visitor);
        }
        for child_node in &self.field {
            child_node.accept(visitor);
        }
        for child_node in &self.extension {
            child_node.accept(visitor);
        }
        for child_node in &self.nested_type {
            child_node.accept(visitor);
        }
        for child_node in &self.enum_type {
            child_node.accept(visitor);
        }
        // Extensions are not covered here, since they are not supported by Proto3
        for child_node in &self.oneof_decl {
            child_node.accept(visitor);
        }
        for child_node in &self.options {
            child_node.accept(visitor);
        }
        for child_node in &self.reserved_range {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SdvPublication] annotation struct.
impl AstNode for SdvPublication {
    /// Accepts a [VsidlVisitor] for an [SdvPublication] annotation node.
    ///
    /// Calls `start_visit_sdv_publication`, `traverse`, and `end_visit_sdv_publication`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_sdv_publication(self);
        self.traverse(visitor);
        visitor.end_visit_sdv_publication(self);
    }

    /// Traverses the children of an [SdvPublication] node.
    ///
    /// Currently, [SdvPublication] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [ProtoFieldDescriptor] (Protobuf Field) struct.
impl AstNode for ProtoFieldDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf field descriptor node.
    ///
    /// Calls `start_visit_proto_field_descriptor`, `traverse`, and `end_visit_proto_field_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_field_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_field_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoFieldDescriptor].
    ///
    /// Visits the field's options, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoFieldOptions] struct.
impl AstNode for ProtoFieldOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf field options node.
    ///
    /// Calls `start_visit_proto_field_options`, `traverse`, and `end_visit_proto_field_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_field_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_field_options(self);
    }

    /// Traverses the children of a [ProtoFieldOptions] node.
    ///
    /// Currently, [ProtoFieldOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoEnumDescriptor] (Protobuf Enum) struct.
impl AstNode for ProtoEnumDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf enum descriptor node.
    ///
    /// Calls `start_visit_proto_enum_descriptor`, `traverse`, and `end_visit_proto_enum_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_enum_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoEnumDescriptor].
    ///
    /// Visits the enum values, options (if present), and reserved ranges.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.value {
            child_node.accept(visitor);
        }
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
        for child_node in &self.reserved_range {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoEnumOptions] struct.
impl AstNode for ProtoEnumOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf enum options node.
    ///
    /// Calls `start_visit_proto_enum_options`, `traverse`, and `end_visit_proto_enum_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_enum_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_options(self);
    }

    /// Traverses the children of a [ProtoEnumOptions] node.
    ///
    /// Currently, [ProtoEnumOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoEnumReservedRange] struct.
impl AstNode for ProtoEnumReservedRange {
    /// Accepts a [VsidlVisitor] for a Protobuf enum reserved range node.
    ///
    /// Calls `start_visit_proto_enum_reserved_range`, `traverse`, and `end_visit_proto_enum_reserved_range`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_enum_reserved_range(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_reserved_range(self);
    }

    /// Traverses the children of a [ProtoEnumReservedRange] node.
    ///
    /// Currently, [ProtoEnumReservedRange] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [ProtoEnumValueDescriptor] (Protobuf Enum Value) struct.
impl AstNode for ProtoEnumValueDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf enum value descriptor node.
    ///
    /// Calls `start_visit_proto_enum_value_descriptor`, `traverse`, and `end_visit_proto_enum_value_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_enum_value_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_value_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoEnumValueDescriptor].
    ///
    /// Visits the enum value's options, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoEnumValueOptions] struct.
impl AstNode for ProtoEnumValueOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf enum value options node.
    ///
    /// Calls `start_visit_proto_enum_value_options`, `traverse`, and `end_visit_proto_enum_value_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_enum_value_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_enum_value_options(self);
    }

    /// Traverses the children of a [ProtoEnumValueOptions] node.
    ///
    /// Currently, [ProtoEnumValueOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoOneofDescriptor] (Protobuf Oneof) struct.
impl AstNode for ProtoOneofDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf oneof descriptor node.
    ///
    /// Calls `start_visit_proto_oneof_descriptor`, `traverse`, and `end_visit_proto_oneof_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_oneof_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_oneof_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoOneofDescriptor].
    ///
    /// Visits the oneof's options, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoOneofOptions] struct.
impl AstNode for ProtoOneofOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf oneof options node.
    ///
    /// Calls `start_visit_proto_oneof_options`, `traverse`, and `end_visit_proto_oneof_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_oneof_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_oneof_options(self);
    }

    /// Traverses the children of a [ProtoOneofOptions] node.
    ///
    /// Currently, [ProtoOneofOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoMessageOptions] struct.
impl AstNode for ProtoMessageOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf message options node.
    ///
    /// Calls `start_visit_proto_message_options`, `traverse`, and `end_visit_proto_message_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_message_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_message_options(self);
    }

    /// Traverses the children of a [ProtoMessageOptions] node.
    ///
    /// Currently, [ProtoMessageOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoReservedRange] struct within a message.
impl AstNode for ProtoReservedRange {
    /// Accepts a [VsidlVisitor] for a Protobuf message reserved range node.
    ///
    /// Calls `start_visit_proto_reserved_range`, `traverse`, and `end_visit_proto_reserved_range`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_reserved_range(self);
        self.traverse(visitor);
        visitor.end_visit_proto_reserved_range(self);
    }

    /// Traverses the children of a [ProtoReservedRange] node.
    ///
    /// Currently, [ProtoReservedRange] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [ProtoServiceDescriptor] (Protobuf Service) struct.
impl AstNode for ProtoServiceDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf service descriptor node.
    ///
    /// Calls `start_visit_proto_service_descriptor`, `traverse`, and `end_visit_proto_service_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_service_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_service_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoServiceDescriptor].
    ///
    /// Visits the methods and options (if present).
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.method {
            child_node.accept(visitor);
        }
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoMethodDescriptor] (Protobuf Method) struct.
impl AstNode for ProtoMethodDescriptor {
    /// Accepts a [VsidlVisitor] for a Protobuf method descriptor node.
    ///
    /// Calls `start_visit_proto_method_descriptor`, `traverse`, and `end_visit_proto_method_descriptor`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_method_descriptor(self);
        self.traverse(visitor);
        visitor.end_visit_proto_method_descriptor(self);
    }

    /// Defines the traversal order for the children of a [ProtoMethodDescriptor].
    ///
    /// Visits the method's options, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(options)) = &self.options {
            options.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ProtoMethodOptions] struct.
impl AstNode for ProtoMethodOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf method options node.
    ///
    /// Calls `start_visit_proto_method_options`, `traverse`, and `end_visit_proto_method_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_method_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_method_options(self);
    }

    /// Traverses the children of a [ProtoMethodOptions] node.
    ///
    /// Currently, [ProtoMethodOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [ProtoServiceOptions] struct.
impl AstNode for ProtoServiceOptions {
    /// Accepts a [VsidlVisitor] for a Protobuf service options node.
    ///
    /// Calls `start_visit_proto_service_options`, `traverse`, and `end_visit_proto_service_options`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_proto_service_options(self);
        self.traverse(visitor);
        visitor.end_visit_proto_service_options(self);
    }

    /// Traverses the children of a [ProtoServiceOptions] node.
    ///
    /// Currently, [ProtoServiceOptions] has no defined children in the AST traversal logic
    /// (uninterpreted options are Proto2 only).
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // uninterpreted options are not part of Proto3, no other children
    }
}

/// Implements the [AstNode] trait for the [VsidlEntry] struct.
impl AstNode for VsidlEntry {
    /// Accepts a [VsidlVisitor] for a VSIDL entry node (root of a .vsidl file).
    ///
    /// Calls `start_visit_vsidl_entry`, `traverse`, and `end_visit_vsidl_entry`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vsidl_entry(self);
        self.traverse(visitor);
        visitor.end_visit_vsidl_entry(self);
    }

    /// Defines the traversal order for the children of a [VsidlEntry].
    ///
    /// Visits service bundles, SOME/IP mappings, and VHAL mappings defined in the file.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.service_bundle {
            child_node.accept(visitor);
        }
        for child_node in &self.some_ip_mapping {
            child_node.accept(visitor);
        }
        for child_node in &self.vhal_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [ServiceBundle] struct.
impl AstNode for ServiceBundle {
    /// Accepts a [VsidlVisitor] for a service bundle node.
    ///
    /// Calls `start_visit_service_bundle`, `traverse`, and `end_visit_service_bundle`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_service_bundle(self);
        self.traverse(visitor);
        visitor.end_visit_service_bundle(self);
    }

    /// Defines the traversal order for the children of a [ServiceBundle].
    ///
    /// Visits publishers, subscribers, servers, clients, owned unit types,
    /// diagnostics declarations (if present), and build configurations (if present).
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.publisher {
            child_node.accept(visitor);
        }
        for child_node in &self.subscriber {
            child_node.accept(visitor);
        }
        for child_node in &self.server {
            child_node.accept(visitor);
        }
        for child_node in &self.client {
            child_node.accept(visitor);
        }
        if let MessageField(Some(diagnostics_declaration)) = &self.diagnostics_declaration {
            diagnostics_declaration.accept(visitor);
        }
        if let MessageField(Some(build_cfg)) = &self.build_cfg {
            build_cfg.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [Publisher] struct.
impl AstNode for Publisher {
    /// Accepts a [VsidlVisitor] for a publisher node.
    ///
    /// Calls `start_visit_publisher`, `traverse`, and `end_visit_publisher`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_publisher(self);
        self.traverse(visitor);
        visitor.end_visit_publisher(self);
    }

    /// Traverses the children of a [Publisher] node.
    ///
    /// Currently, [Publisher] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [Subscriber] struct.
impl AstNode for Subscriber {
    /// Accepts a [VsidlVisitor] for a subscriber node.
    ///
    /// Calls `start_visit_subscriber`, `traverse`, and `end_visit_subscriber`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_subscriber(self);
        self.traverse(visitor);
        visitor.end_visit_subscriber(self);
    }

    /// Traverses the children of a [Subscriber] node.
    ///
    /// Currently, [Subscriber] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [Server] struct.
impl AstNode for Server {
    /// Accepts a [VsidlVisitor] for a server node.
    ///
    /// Calls `start_visit_server`, `traverse`, and `end_visit_server`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_server(self);
        self.traverse(visitor);
        visitor.end_visit_server(self);
    }

    /// Traverses the children of a [Server] node.
    ///
    /// Currently, [Server] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [Client] struct.
impl AstNode for Client {
    /// Accepts a [VsidlVisitor] for a client node.
    ///
    /// Calls `start_visit_client`, `traverse`, and `end_visit_client`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_client(self);
        self.traverse(visitor);
        visitor.end_visit_client(self);
    }

    /// Traverses the children of a [Client] node.
    ///
    /// Currently, [Client] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [DiagnosticsDeclaration] struct.
impl AstNode for DiagnosticsDeclaration {
    /// Accepts a [VsidlVisitor] for a diagnostics declaration node.
    ///
    /// Calls `start_visit_diagnostics`, `traverse`, and `end_visit_diagnostics`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics(self);
    }

    /// Defines the traversal order for the children of a [DiagnosticsDeclaration].
    ///
    /// Visits data items, I/O controls, routines, and events.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.data_item {
            child_node.accept(visitor);
        }
        for child_node in &self.io_control {
            child_node.accept(visitor);
        }
        for child_node in &self.routine {
            child_node.accept(visitor);
        }
        for child_node in &self.event {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [DataItem] struct within Diagnostics.
impl AstNode for DataItem {
    /// Accepts a [VsidlVisitor] for a diagnostics data item node.
    ///
    /// Calls `start_visit_diagnostics_data_item`, `traverse`, and `end_visit_diagnostics_data_item`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics_data_item(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_data_item(self);
    }

    /// Traverses the children of a [DataItem] node.
    ///
    /// Currently, [DataItem] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [InputOutputControlData] struct within Diagnostics.
impl AstNode for InputOutputControlData {
    /// Accepts a [VsidlVisitor] for a diagnostics I/O control node.
    ///
    /// Calls `start_visit_diagnostics_io_control`, `traverse`, and `end_visit_diagnostics_io_control`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics_io_control(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_io_control(self);
    }

    /// Traverses the children of an [InputOutputControlData] node.
    ///
    /// Currently, [InputOutputControlData] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [Routine] struct within Diagnostics.
impl AstNode for Routine {
    /// Accepts a [VsidlVisitor] for a diagnostics routine node.
    ///
    /// Calls `start_visit_diagnostics_routine`, `traverse`, and `end_visit_diagnostics_routine`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics_routine(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_routine(self);
    }

    /// Defines the traversal order for the children of a [Routine].
    ///
    /// Visits the start, stop, and result methods, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(start)) = &self.start {
            start.accept(visitor);
        }
        if let MessageField(Some(stop)) = &self.stop {
            stop.accept(visitor);
        }
        if let MessageField(Some(result)) = &self.result {
            result.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [Method] struct within a Diagnostics Routine.
impl AstNode for Method {
    /// Accepts a [VsidlVisitor] for a diagnostics routine method node.
    ///
    /// Calls `start_visit_diagnostics_routine_method`, `traverse`, and `end_visit_diagnostics_routine_method`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics_routine_method(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_routine_method(self);
    }

    /// Traverses the children of a [Method] node.
    ///
    /// Currently, [Method] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [Event] struct within Diagnostics.
impl AstNode for Event {
    /// Accepts a [VsidlVisitor] for a diagnostics event node.
    ///
    /// Calls `start_visit_diagnostics_event`, `traverse`, and `end_visit_diagnostics_event`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_diagnostics_event(self);
        self.traverse(visitor);
        visitor.end_visit_diagnostics_event(self);
    }

    /// Traverses the children of an [Event] node.
    ///
    /// Currently, [Event] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [BuildConfiguration] struct.
impl AstNode for BuildConfiguration {
    /// Accepts a [VsidlVisitor] for a build configuration node.
    ///
    /// Calls `start_visit_build_cfg`, `traverse`, and `end_visit_build_cfg`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_build_cfg(self);
        self.traverse(visitor);
        visitor.end_visit_build_cfg(self);
    }

    /// Traverses the children of a [BuildConfiguration] node.
    ///
    /// Currently, [BuildConfiguration] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpMapping] struct.
impl AstNode for SomeIpMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP mapping node.
    ///
    /// Calls `start_visit_someip_mapping`, `traverse`, and `end_visit_someip_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpMapping].
    ///
    /// Visits either the [SomeIpServiceMapping] or the [SomeIpSdvTypeMapping], depending on which one is present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if self.has_someip_service_mapping() {
            self.someip_service_mapping().accept(visitor);
        } else if self.has_sdv_type_mapping() {
            self.sdv_type_mapping().accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpServiceMapping] struct.
impl AstNode for SomeIpServiceMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP service mapping node.
    ///
    /// Calls `start_visit_someip_service_mapping`, `traverse`, and `end_visit_someip_service_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_service_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpServiceMapping].
    ///
    /// Visits instance mappings and instance implementations.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.instance {
            child_node.accept(visitor);
        }
        for child_node in &self.instance_impl {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpInstanceMapping] struct.
impl AstNode for SomeIpInstanceMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP instance mapping node.
    ///
    /// Calls `start_visit_someip_instance_mapping`, `traverse`, and `end_visit_someip_instance_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_instance_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_instance_mapping(self);
    }

    /// Traverses the children of a [SomeIpInstanceMapping] node.
    ///
    /// # Parameters
    /// * `visitor`: The visitor.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for sdv_topic_parameter in self.sdv_topic_parameterization.values() {
            sdv_topic_parameter.accept(visitor);
        }
    }
}

impl AstNode for SdvTopicParameters {
    /// Accepts a [VsidlVisitor] for a SOME/IP SdvTopicParameters node.
    ///
    /// Calls `start_visit_sdv_topic_parameters`, `traverse`, and `end_visit_sdv_topic_parameters`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
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
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpInstanceImplementation] struct.
impl AstNode for SomeIpInstanceImplementation {
    /// Accepts a [VsidlVisitor] for a SOME/IP instance implementation node.
    ///
    /// Calls `start_visit_someip_instance_implementation`, `traverse`, and `end_visit_someip_instance_implementation`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_instance_implementation(self);
        self.traverse(visitor);
        visitor.end_visit_someip_instance_implementation(self);
    }

    /// Defines the traversal order for the children of a [SomeIpInstanceImplementation].
    ///
    /// Visits the offered RPC interface and offered event, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if self.has_offered_interface() {
            self.offered_interface().accept(visitor);
        }
        if self.has_offered_event() {
            self.offered_event().accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpRpcInterface] struct (within InstanceImplementation).
impl AstNode for SomeIpRpcInterface {
    /// Accepts a [VsidlVisitor] for a SOME/IP RPC interface node (within InstanceImplementation).
    ///
    /// Calls `start_visit_someip_rpc_interface`, `traverse`, and `end_visit_someip_rpc_interface`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_rpc_interface(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_interface(self);
    }

    /// Defines the traversal order for the children of a [SomeIpRpcInterface].
    ///
    /// Visits the associated SDV unit type (if present) and methods.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(unit_type)) = &self.sdv_unit_type {
            unit_type.accept(visitor);
        }
        for child_node in &self.method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpTopic] struct.
impl AstNode for SomeIpTopic {
    /// Accepts a [VsidlVisitor] for a SOME/IP topic node.
    ///
    /// Calls `start_visit_someip_topic`, `traverse`, and `end_visit_someip_topic`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_topic(self);
        self.traverse(visitor);
        visitor.end_visit_someip_topic(self);
    }

    /// Defines the traversal order for the children of a [SomeIpTopic].
    ///
    /// Visits the type definition (if present) and the simple mapping (if present).
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(type_definition)) = &self.type_definition {
            type_definition.accept(visitor);
        }
        if self.has_simple_mapping() {
            self.simple_mapping().accept(visitor);
        }
        // SomeipFfMethods is only a wrapper for bool
    }
}

/// Implements the [AstNode] trait for the [SomeIpSimpleTopicMapping] struct.
impl AstNode for SomeIpSimpleTopicMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP simple topic mapping node.
    ///
    /// Calls `start_visit_someip_simple_topic_mapping`, `traverse`, and `end_visit_someip_simple_topic_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_simple_topic_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_simple_topic_mapping(self);
    }

    /// Traverses the children of a [SomeIpSimpleTopicMapping] node.
    ///
    /// Currently, [SomeIpSimpleTopicMapping] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpUnitType] struct.
impl AstNode for SomeIpUnitType {
    /// Accepts a [VsidlVisitor] for a SOME/IP unit type reference node.
    ///
    /// Calls `start_visit_someip_unit_type`, `traverse`, and `end_visit_someip_unit_type`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_unit_type(self);
        self.traverse(visitor);
        visitor.end_visit_someip_unit_type(self);
    }

    /// Traverses the children of a [SomeIpUnitType] node.
    ///
    /// Currently, [SomeIpUnitType] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpRequestResponseMethod] struct.
impl AstNode for SomeIpRequestResponseMethod {
    /// Accepts a [VsidlVisitor] for a SOME/IP request-response method node.
    ///
    /// Calls `start_visit_someip_request_response_method`, `traverse`, and `end_visit_someip_request_response_method`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_request_response_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_request_response_method(self);
    }

    /// Defines the traversal order for the children of a [SomeIpRequestResponseMethod].
    ///
    /// Visits the request and response payload mappings, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(request)) = &self.request_payload_mapping {
            request.accept(visitor);
        }
        if let MessageField(Some(response)) = &self.response_payload_mapping {
            response.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpEvent] struct.
impl AstNode for SomeIpEvent {
    /// Accepts a [VsidlVisitor] for a SOME/IP event node.
    ///
    /// Calls `start_visit_someip_event`, `traverse`, and `end_visit_someip_event`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_event(self);
    }

    /// Defines the traversal order for the children of a [SomeIpEvent].
    ///
    /// Visits either the simple event or the group event, depending on which one is present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if self.has_simple_event() {
            self.simple_event().accept(visitor);
        } else if self.has_group_event() {
            self.group_event().accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpSimpleEvent] struct.
impl AstNode for SomeIpSimpleEvent {
    /// Accepts a [VsidlVisitor] for a SOME/IP simple event node.
    ///
    /// Calls `start_visit_someip_simple_event`, `traverse`, and `end_visit_someip_simple_event`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_simple_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_simple_event(self);
    }

    /// Defines the traversal order for the children of a [SomeIpSimpleEvent].
    ///
    /// Visits the associated SDV unit type and the type definition, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(sdv_unit_type)) = &self.sdv_unit_type {
            sdv_unit_type.accept(visitor);
        }
        if let MessageField(Some(type_definition)) = &self.type_definition {
            type_definition.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpGroupEvent] struct.
impl AstNode for SomeIpGroupEvent {
    /// Accepts a [VsidlVisitor] for a SOME/IP group event node.
    ///
    /// Calls `start_visit_someip_group_event`, `traverse`, and `end_visit_someip_group_event`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_group_event(self);
        self.traverse(visitor);
        visitor.end_visit_someip_group_event(self);
    }

    /// Defines the traversal order for the children of a [SomeIpGroupEvent].
    ///
    /// Visits the type definition (if present) and the event parts.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(type_definition)) = &self.type_definition {
            type_definition.accept(visitor);
        }
        for child_node in &self.event_part {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpGroupEventPart] struct.
impl AstNode for SomeIpGroupEventPart {
    /// Accepts a [VsidlVisitor] for a SOME/IP group event part node.
    ///
    /// Calls `start_visit_someip_group_event_part`, `traverse`, and `end_visit_someip_group_event_part`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_group_event_part(self);
        self.traverse(visitor);
        visitor.end_visit_someip_group_event_part(self);
    }

    /// Defines the traversal order for the children of a [SomeIpGroupEventPart].
    ///
    /// Visits the associated SDV unit type, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(sdv_unit_type)) = &self.sdv_unit_type {
            sdv_unit_type.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpStruct] struct.
impl AstNode for SomeIpStruct {
    /// Accepts a [VsidlVisitor] for a SOME/IP struct definition node.
    ///
    /// Calls `start_visit_someip_struct`, `traverse`, and `end_visit_someip_struct`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_struct(self);
        self.traverse(visitor);
        visitor.end_visit_someip_struct(self);
    }

    /// Defines the traversal order for the children of a [SomeIpStruct].
    ///
    /// Visits the struct fields.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.field {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpStructField] struct.
impl AstNode for SomeIpStructField {
    /// Accepts a [VsidlVisitor] for a SOME/IP struct field node.
    ///
    /// Calls `start_visit_someip_struct_field`, `traverse`, and `end_visit_someip_struct_field`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_struct_field(self);
        self.traverse(visitor);
        visitor.end_visit_someip_struct_field(self);
    }

    /// Traverses the children of a [SomeIpStructField] node.
    ///
    /// Currently, [SomeIpStructField] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpSdvTypeMapping] struct.
impl AstNode for SomeIpSdvTypeMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP SDV type mapping node.
    ///
    /// Calls `start_visit_someip_sdv_type_mapping`, `traverse`, and `end_visit_someip_sdv_type_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_sdv_type_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_sdv_type_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpSdvTypeMapping].
    ///
    /// Visits the associated unit type (if present), service unit mappings,
    /// offered RPC interface (if present), and offered topic (if present).
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(unit_type)) = &self.unit_type {
            unit_type.accept(visitor);
        }
        for child_node in &self.service_unit_mapping {
            child_node.accept(visitor);
        }
        if self.has_offered_interface() {
            self.offered_interface().accept(visitor);
        }
        if self.has_offered_topic() {
            self.offered_topic().accept(visitor);
        }
        if let MessageField(Some(service_oriented_mapping)) = &self.service_oriented_mapping {
            service_oriented_mapping.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpServiceOrientedMapping] struct.
impl AstNode for SomeIpServiceOrientedMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP service-oriented mapping node.
    ///
    /// Calls `start_visit_someip_service_oriented_mapping`, `traverse`, and `end_visit_someip_service_oriented_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_service_oriented_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_oriented_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpServiceOrientedMapping].
    ///
    /// Visits either the RPC interface mapping or the publication mapping, depending on which one is present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if self.has_rpc_interface_mapping() {
            self.rpc_interface_mapping().accept(visitor);
        } else if self.has_publication_mapping() {
            self.publication_mapping().accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpRpcInterfaceMapping] struct.
impl AstNode for SomeIpRpcInterfaceMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP RPC interface mapping node.
    ///
    /// Calls `start_visit_someip_rpc_interface_mapping`, `traverse`, and `end_visit_someip_rpc_interface_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_rpc_interface_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_interface_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpRpcInterfaceMapping].
    ///
    /// Visits the offered interface and the RPC channel mappings.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(offered_interface)) = &self.offered_interface {
            offered_interface.accept(visitor);
        }
        for child_node in &self.rpc_channel_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpRpcChannelMapping] struct.
impl AstNode for SomeIpRpcChannelMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP RPC channel mapping node.
    ///
    /// Calls `start_visit_someip_rpc_channel_mapping`, `traverse`, and `end_visit_someip_rpc_channel_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_rpc_channel_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_rpc_channel_mapping(self);
    }

    /// Traverses the children of a [SomeIpRpcChannelMapping] node.
    ///
    /// Currently, [SomeIpRpcChannelMapping] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpPublicationMapping] struct.
impl AstNode for SomeIpPublicationMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP publication mapping node.
    ///
    /// Calls `start_visit_someip_publication_mapping`, `traverse`, and `end_visit_someip_publication_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_publication_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_publication_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpPublicationMapping].
    ///
    /// Visits the offered topic and the topic mappings.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(offered_topic)) = &self.offered_topic {
            offered_topic.accept(visitor);
        }
        for child_node in &self.topic_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpTopicMapping] struct.
impl AstNode for SomeIpTopicMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP topic mapping node.
    ///
    /// Calls `start_visit_someip_topic_mapping`, `traverse`, and `end_visit_someip_topic_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_topic_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_topic_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpTopicMapping].
    ///
    /// Visits the fire and forget methods, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.fire_and_forget_method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpMappingRpcInterface] struct (within SdvTypeMapping).
impl AstNode for SomeIpMappingRpcInterface {
    /// Accepts a [VsidlVisitor] for a SOME/IP RPC interface node (within SdvTypeMapping).
    ///
    /// Calls `start_visit_someip_type_mapping_rpc_interface`, `traverse`, and `end_visit_someip_type_mapping_rpc_interface`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_type_mapping_rpc_interface(self);
        self.traverse(visitor);
        visitor.end_visit_someip_type_mapping_rpc_interface(self);
    }

    /// Defines the traversal order for the children of a [SomeIpMappingRpcInterface].
    ///
    /// Visits the methods defined in the interface.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpMappingRpcInterfaceMethod] struct.
impl AstNode for SomeIpMappingRpcInterfaceMethod {
    /// Accepts a [VsidlVisitor] for a SOME/IP RPC interface method node (within SdvTypeMapping).
    ///
    /// Calls `start_visit_someip_type_mapping_rpc_interface_method`, `traverse`, and `end_visit_someip_type_mapping_rpc_interface_method`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_type_mapping_rpc_interface_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_type_mapping_rpc_interface_method(self);
    }

    /// Defines the traversal order for the children of a [SomeIpMappingRpcInterfaceMethod].
    ///
    /// Visits the request and response payload mappings, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(request)) = &self.request_payload_mapping {
            request.accept(visitor);
        }
        if let MessageField(Some(response)) = &self.response_payload_mapping {
            response.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpSdvServiceUnitMapping] struct.
impl AstNode for SomeIpSdvServiceUnitMapping {
    /// Accepts a [VsidlVisitor] for a SOME/IP SDV service unit mapping node.
    ///
    /// Calls `start_visit_someip_sdv_service_unit_mapping`, `traverse`, and `end_visit_someip_sdv_service_unit_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_sdv_service_unit_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_someip_sdv_service_unit_mapping(self);
    }

    /// Defines the traversal order for the children of a [SomeIpSdvServiceUnitMapping].
    ///
    /// Visits the service unit ID (if present) and any fire-and-forget methods.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(service_unit_id)) = &self.service_unit_id {
            service_unit_id.accept(visitor);
        }
        for child_node in &self.fire_and_forget_method {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [SomeIpServiceUnitId] struct.
impl AstNode for SomeIpServiceUnitId {
    /// Accepts a [VsidlVisitor] for a SOME/IP service unit ID node.
    ///
    /// Calls `start_visit_someip_service_unit_id`, `traverse`, and `end_visit_someip_service_unit_id`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_service_unit_id(self);
        self.traverse(visitor);
        visitor.end_visit_someip_service_unit_id(self);
    }

    /// Traverses the children of a [SomeIpServiceUnitId] node.
    ///
    /// Currently, [SomeIpServiceUnitId] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [SomeIpFFMethod] (Fire and Forget) struct.
impl AstNode for SomeIpFFMethod {
    /// Accepts a [VsidlVisitor] for a SOME/IP fire-and-forget method node.
    ///
    /// Calls `start_visit_someip_ff_method`, `traverse`, and `end_visit_someip_ff_method`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_someip_ff_method(self);
        self.traverse(visitor);
        visitor.end_visit_someip_ff_method(self);
    }

    /// Traverses the children of a [SomeIpFFMethod] node.
    ///
    /// Currently, [SomeIpFFMethod] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // No child nodes
    }
}

/// Implements the [AstNode] trait for the [VhalMapping] struct.
impl AstNode for VhalMapping {
    /// Accepts a [VsidlVisitor] for a VHAL mapping node.
    ///
    /// Calls `start_visit_vhal_mapping`, `traverse`, and `end_visit_vhal_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_mapping(self);
    }

    /// Defines the traversal order for the children of a [VhalMapping].
    ///
    /// Visits the vehicle property (if present), the data source (if present), and area mappings.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(property)) = &self.property {
            property.accept(visitor);
        }
        if let MessageField(Some(source)) = &self.source {
            source.accept(visitor);
        }
        for child_node in &self.area_mapping {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [VhalVehicleProperty] struct.
impl AstNode for VhalVehicleProperty {
    /// Accepts a [VsidlVisitor] for a VHAL vehicle property node.
    ///
    /// Calls `start_visit_vhal_vehicle_property`, `traverse`, and `end_visit_vhal_vehicle_property`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_vehicle_property(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_property(self);
    }

    /// Defines the traversal order for the children of a [VhalVehicleProperty].
    ///
    /// Visits the property config, default value, and default area config, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(prop_config)) = &self.prop_config {
            prop_config.accept(visitor);
        }
        if let MessageField(Some(default_value)) = &self.default_value {
            default_value.accept(visitor);
        }
        if let MessageField(Some(default_area_config)) = &self.default_area_config {
            default_area_config.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [VhalVehiclePropConfig] struct.
impl AstNode for VhalVehiclePropConfig {
    /// Accepts a [VsidlVisitor] for a VHAL vehicle property config node.
    ///
    /// Calls `start_visit_vhal_vehicle_prop_config`, `traverse`, and `end_visit_vhal_vehicle_prop_config`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_vehicle_prop_config(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_prop_config(self);
    }

    /// Defines the traversal order for the children of a [VhalVehiclePropConfig].
    ///
    /// Visits the area configs defined within the property config.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to child nodes.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        for child_node in &self.area_configs {
            child_node.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [VhalDefaultValue] struct.
impl AstNode for VhalDefaultValue {
    /// Accepts a [VsidlVisitor] for a VHAL default value node.
    ///
    /// Calls `start_visit_vhal_default_value`, `traverse`, and `end_visit_vhal_default_value`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_default_value(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_default_value(self);
    }

    /// Traverses the children of a [VhalDefaultValue] node.
    ///
    /// Currently, [VhalDefaultValue] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [VhalVehicleAreaConfig] struct.
impl AstNode for VhalVehicleAreaConfig {
    /// Accepts a [VsidlVisitor] for a VHAL vehicle area config node.
    ///
    /// Calls `start_visit_vhal_vehicle_area_config`, `traverse`, and `end_visit_vhal_vehicle_area_config`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_vehicle_area_config(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_vehicle_area_config(self);
    }

    /// Traverses the children of a [VhalVehicleAreaConfig] node.
    ///
    /// Currently, [VhalVehicleAreaConfig] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [VhalDataSource] struct.
impl AstNode for VhalDataSource {
    /// Accepts a [VsidlVisitor] for a VHAL data source node.
    ///
    /// Calls `start_visit_vhal_data_source`, `traverse`, and `end_visit_vhal_data_source`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_data_source(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_data_source(self);
    }

    /// Traverses the children of a [VhalDataSource] node.
    ///
    /// Currently, [VhalDataSource] has no defined children in the AST traversal logic.
    ///
    /// # Parameters
    /// * `_visitor`: The visitor (unused).
    fn traverse(&self, _visitor: &mut dyn VsidlVisitor) {
        // no child nodes
    }
}

/// Implements the [AstNode] trait for the [VhalAreaMapping] struct.
impl AstNode for VhalAreaMapping {
    /// Accepts a [VsidlVisitor] for a VHAL area mapping node.
    ///
    /// Calls `start_visit_vhal_area_mapping`, `traverse`, and `end_visit_vhal_area_mapping`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_area_mapping(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_area_mapping(self);
    }

    /// Defines the traversal order for the children of a [VhalAreaMapping].
    ///
    /// Visits the property area definition, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(area)) = &self.area {
            area.accept(visitor);
        }
    }
}

/// Implements the [AstNode] trait for the [VhalPropertyArea] struct.
impl AstNode for VhalPropertyArea {
    /// Accepts a [VsidlVisitor] for a VHAL property area node.
    ///
    /// Calls `start_visit_vhal_property_area`, `traverse`, and `end_visit_vhal_property_area`.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to accept.
    fn accept(&self, visitor: &mut dyn VsidlVisitor) {
        visitor.start_visit_vhal_property_area(self);
        self.traverse(visitor);
        visitor.end_visit_vhal_property_area(self);
    }

    /// Defines the traversal order for the children of a [VhalPropertyArea].
    ///
    /// Visits the area config, if present.
    ///
    /// # Parameters
    /// * `visitor`: The visitor to pass to the child node.
    fn traverse(&self, visitor: &mut dyn VsidlVisitor) {
        if let MessageField(Some(area_config)) = &self.area_config {
            area_config.accept(visitor);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_data;
    use crate::visitors::visitor::AstNode;
    use crate::visitors::visitor::MockVsidlVisitor;
    use crate::Catalog;
    use crate::VsidlVisitor;

    /// Empty visitor. Only exists because Automock is not detected by coverage reports.
    pub struct ManualMock;
    impl VsidlVisitor for ManualMock {}

    #[test]
    /// Verifies that the visitor infrastructure correctly traverses all defined AST node types.
    ///
    /// This test uses a mock visitor ([MockVsidlVisitor]) and a specially crafted catalog
    /// ([test_data::VSIDL_ALL_AST_NODE_TYPES], [test_data::ALL_AST_NODE_TYPES_PROTO])
    /// that includes instances of every possible node type defined in the visitor trait.
    /// It sets expectations on the mock that each `start_visit_*` and `end_visit_*` method
    /// is called at least once. If the traversal logic in the `AstNode` implementations
    /// is incomplete or incorrect, the mock will panic, failing the test.
    fn test_visitor_visits_every_ast_node() {
        // GIVEN a mock that expects every visit function to be called at least once
        let mut mock = MockVsidlVisitor::new();
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
