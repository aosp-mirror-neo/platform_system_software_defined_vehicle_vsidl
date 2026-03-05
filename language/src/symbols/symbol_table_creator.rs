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

use indexmap::IndexMap;
use std::path::PathBuf;
use vsidl_v1::annotations::{sdv_publication::PublisherModel, SdvPublication};
use vsidl_v1::syntax::service_bundle::Client;
use vsidl_v1::syntax::{
    service_bundle::{Publisher, Server, Subscriber},
    ServiceBundle, Span, VsidlEntry,
};

use super::specific_symbols::rpc_client_symbol::RpcClientSymbol;
use super::symbol_table::GLOBAL_SCOPE_ID;
use super::symbol_table::{SymbolTableState, TargetApi};
use super::{
    scope::ScopeId,
    specific_symbols::{
        proto_enum_symbol::ProtoEnumSymbol, proto_message_symbol::ProtoMessageSymbol,
        proto_publication_symbol::ProtoPublicationSymbol,
        proto_rpc_service_symbol::ProtoRpcServiceSymbol, publisher_symbol::PublisherSymbol,
        rpc_channel_symbol::RpcChannelSymbol, rpc_server_symbol::RpcServerSymbol,
        service_bundle_symbol::ServiceBundleSymbol, subscriber_symbol::SubscriberSymbol,
        topic_symbol::TopicSymbol,
    },
    symbol::{SymbolId, SymbolKind},
    symbol_table::SymbolTable,
};
use crate::proto_rpc_service_symbol::ProtoRpcServiceSource;
use crate::service_bundle_symbol::ServiceBundleSource;
use pest::iterators::Pair;
use pest::Parser;
use protobuf::descriptor::{DescriptorProto, EnumDescriptorProto, ServiceDescriptorProto};
use std::collections::{HashMap, HashSet};
use vsidl_v1_common::message_size::{MessageSize, MessageSizeMetadataSet};
use vsidl_v1_common::{
    get_proto_dependencies, message_size, DiagnosticsContext, DiagnosticsInfo, DiskFileProvider,
    FileProvider, ProgressPrinter, ProtoMetadata, VsidlVisitor,
};
use vsidl_v1_common::{visitors::visitor::ProtoFile, Catalog};
use vsidl_v1_pest_parser::proto_parser::{ProtoParser, Rule as ProtoRule};

/// Creates a symbol table with scopes and symbols from a [Catalog].
///
/// This struct implements the [VsidlVisitor] trait to traverse the AST
/// representation of the input `.proto` and `.vsidl` files. During traversal,
/// it builds a [SymbolTable] containing symbols for various language elements
/// (like service bundles, publishers, subscribers, Protobuf messages, etc.)
/// and organizes them into scopes representing the package structure.
///
/// Because of dependencies between symbols that are not resolvable
/// during the initial creation phase, only containment relationships are
/// fully established by the [SymbolTableCreator] (e.g., "a ServiceBundle contains a Publisher").
///
/// After the creator finishes, a [SymbolTableLinker] should be used to resolve
/// and link cross-tree relationships (e.g., "an RpcServer serves a ProtoRpcService").
pub struct SymbolTableCreator<F: FileProvider = DiskFileProvider> {
    /// The input catalog containing parsed VSIDL and Proto files.
    catalog: Catalog,
    /// The current package name being processed.
    package: String,
    /// The symbol table being built.
    symbol_table: SymbolTable,
    /// The ID of the global (root) scope.
    global_scope: Option<ScopeId>,
    /// The ID of the current scope being processed (e.g., package, service bundle).
    current_scope_id: Option<ScopeId>,
    /// Temporarily stores the scope ID of a message when processing a publication derived from it.
    message_scope_id: Option<ScopeId>,
    /// The ID of the service bundle symbol currently being processed.
    current_service_bundle_symbol_id: Option<SymbolId>,
    /// Metadata of the current `.proto` file being processed.
    proto_metadata: ProtoMetadata,
    /// The Spans of the currently processed proto file. Maps type + name to Span.
    current_proto_spans: HashMap<(String, String), Span>,
    /// The doc strings of the currently processed proto file. Maps type + name to doc string.
    current_proto_doc_strings: HashMap<(String, String), String>,
    /// Precomputed metadata needed for calculating message sizes.
    message_size_metadata: MessageSizeMetadataSet,
    /// Cache for computed message sizes to avoid redundant calculations.
    messages_computed_sizes: IndexMap<String, MessageSize>,
    /// Map storing the transitive dependencies for each `.proto` file.
    proto_dependencies: HashMap<String, Vec<ProtoMetadata>>,
    /// Flag indicating if the current service bundle comes from the main catalog or a dependency.
    bundle_comes_from_catalog: bool,
    /// Counter for the number of files processed so far.
    processed_files: usize,
    /// Total number of files to be processed.
    total_files: usize,
    /// Utility for printing progress updates to the console.
    printer: ProgressPrinter,
    /// The path of the `.vsidl` file currently being processed.
    current_vsidl_file: PathBuf,
    /// Flag indicating if the current proto file comes from the main catalog or a dependency.
    proto_comes_from_catalog: bool,
    /// Cached context for diagnostics to avoid expensive re-initialization per bundle.
    diagnostics_context: Option<DiagnosticsContext>,
    /// Flag indicating if the current file should be processed (for incremental updates).
    should_process_current_file: bool,
    /// Already created channels in the symbol table.
    created_channels: HashSet<String>,
    /// Already created RPC channels in the symbol table.
    created_rpc_channels: HashSet<String>,
    /// Optional path to a specific file to process. If set, only this file is processed.
    target_file: Option<PathBuf>,
    /// The target API level for code generation.
    target_api: TargetApi,
    /// The file provider for reading files.
    file_provider: F,
}

impl SymbolTableCreator<DiskFileProvider> {
    /// Creates a new, empty [SymbolTableCreator].
    pub fn new() -> Self {
        Self::with_file_provider(DiskFileProvider)
    }

    /// Creates a [SymbolTableCreator] initialized with an existing [SymbolTable].
    /// This allows incremental updates to the table.
    pub fn from_symbol_table(symbol_table: SymbolTable) -> Self {
        let mut creator = Self::new();
        creator.symbol_table = symbol_table;
        creator
    }
}

impl<F: FileProvider> SymbolTableCreator<F> {
    /// Creates a new, empty [SymbolTableCreator] with a specific file provider.
    pub fn with_file_provider(file_provider: F) -> Self {
        SymbolTableCreator {
            catalog: Catalog::new(),
            package: "".to_string(),
            symbol_table: SymbolTable::default(),
            global_scope: None,
            current_scope_id: None,
            message_scope_id: None,
            current_service_bundle_symbol_id: None,
            proto_metadata: ProtoMetadata::default(),
            current_proto_spans: HashMap::new(),
            current_proto_doc_strings: HashMap::new(),
            message_size_metadata: MessageSizeMetadataSet::default(),
            messages_computed_sizes: IndexMap::new(),
            proto_dependencies: HashMap::new(),
            bundle_comes_from_catalog: false,
            processed_files: 0,
            total_files: 0,
            printer: ProgressPrinter::new(0),
            current_vsidl_file: PathBuf::new(),
            proto_comes_from_catalog: false,
            diagnostics_context: None,
            should_process_current_file: true,
            created_channels: HashSet::new(),
            created_rpc_channels: HashSet::new(),
            target_file: None,
            target_api: TargetApi::default(),
            file_provider,
        }
    }

    /// Creates a [SymbolTableCreator] initialized with an existing [SymbolTable] and a specific file provider.
    pub fn from_symbol_table_with_provider(symbol_table: SymbolTable, file_provider: F) -> Self {
        let mut creator = Self::with_file_provider(file_provider);
        creator.symbol_table = symbol_table;
        creator
    }

    /// Sets a file to restrict processing to.
    /// Only symbols originating from this file will be created.
    pub fn restrict_to_file(&mut self, file: PathBuf) {
        self.target_file = Some(file);
    }

    /// Sets the target API level for code generation.
    pub fn set_target_api(&mut self, target_api: TargetApi) {
        self.target_api = target_api;
    }

    /// Returns an immutable reference to the symbol table being built.
    pub fn symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }

    /// Returns a clone of the symbol table being built.
    pub fn symbol_table_clone(&self) -> SymbolTable {
        self.symbol_table.clone()
    }

    /// Consumes the creator and returns the symbol table being built.
    pub fn into_symbol_table(self) -> SymbolTable {
        self.symbol_table
    }

    /// Returns the ID of the global scope, if it has been created.
    pub(super) fn global_scope(&self) -> Option<ScopeId> {
        self.global_scope.clone()
    }

    fn enter_file_with_package(&mut self, package: String) {
        let mut current_scope_id = self.global_scope.clone().expect("Global scope must be created");

        // Look up the scope of the package or create new scopes if no scopes exist for (parts of) the package
        for part in package.split('.') {
            if let Some(existing_scope_id) =
                self.symbol_table.get_subscope_by_name(&current_scope_id, part)
            {
                current_scope_id = existing_scope_id;
            } else {
                let new_scope_id = self.symbol_table.create_scope(part.to_string());
                self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);
                current_scope_id = new_scope_id;
            }
        }
        self.current_scope_id = Some(current_scope_id);
    }

    fn leave_current_scope_to_enclosing_scope(&mut self) {
        if let Some(current_scope_id) = &self.current_scope_id {
            self.current_scope_id = Some(
                self.symbol_table
                    .get_scope(current_scope_id)
                    .expect("Current scope should be present")
                    .enclosing_scope
                    .clone()
                    .expect("Enclosing scope should be present")
                    .clone(),
            );
        }
    }

    /// Helper function to extract the text content from a comment pair.
    /// It strips comment delimiters (// or /* ... */) and trims whitespace.
    fn extract_comment_text(pair: Pair<ProtoRule>) -> String {
        let s = pair.as_str();

        if s.starts_with("/**") && s.ends_with("*/") {
            // 1. Javadoc Comment (Doc String)
            let content = s.trim_start_matches("/**").trim_end_matches("*/");

            // Process line by line, removing leading '*' and whitespace
            content
                .lines()
                .map(|line| {
                    let trimmed_line = line.trim_start();
                    if trimmed_line.starts_with('*') {
                        trimmed_line.trim_start_matches('*').trim_start().to_string()
                    } else {
                        trimmed_line.to_string()
                    }
                })
                // Filter out empty or whitespace-only lines
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<String>>()
                .join("\n")
        } else if s.starts_with("/*") && s.ends_with("*/") {
            // 2. Multi-line Block Comment
            let content = s.trim_start_matches("/*").trim_end_matches("*/");

            // Trim whitespace from each line and join
            content
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect::<Vec<String>>()
                .join("\n")
        } else if s.starts_with("//") {
            // 3. Single-line and Multi-slash Comments (//, ///, ////, etc.)
            let content = s.trim_start_matches('/');

            // After removing all leading slashes, trim any remaining whitespace
            content.trim_start().to_string()
        } else {
            // Fallback for unexpected format
            s.trim().to_string()
        }
    }
}

impl<F: FileProvider> VsidlVisitor for SymbolTableCreator<F> {
    fn start_visit_catalog(&mut self, catalog: &Catalog) {
        self.catalog = catalog.clone();
        if self.symbol_table.get_state() == SymbolTableState::NotCreated {
            self.symbol_table = SymbolTable::default();
            self.symbol_table.set_target_api(self.target_api.clone());
            self.global_scope = Some(self.symbol_table.create_scope("".to_string()));
        } else {
            // Reuse existing symbol table
            self.global_scope = Some(GLOBAL_SCOPE_ID);
        }
        self.created_channels =
            self.symbol_table.get_all_topic_symbols().iter().map(|s| s.name()).collect();
        self.created_rpc_channels =
            self.symbol_table.get_all_rpc_channel_symbols().iter().map(|s| s.name()).collect();
        self.current_scope_id = self.global_scope.clone();
        self.total_files = catalog.catalog_vsidl_files.len() + catalog.proto_descriptors.len();
        self.printer = ProgressPrinter::new(self.total_files);

        // Calculate max proto message size
        self.message_size_metadata = message_size::get_message_size_metadata(
            &catalog.proto_descriptors.iter().map(|(_, d)| d.clone()).collect(),
        )
        .unwrap();

        // Find all (transitive) dependencies of the Proto files
        let mut all_proto_files = HashMap::new();
        for (metadata, file_descriptor) in &catalog.proto_descriptors {
            all_proto_files.insert(
                file_descriptor.name.as_ref().expect("proto file has no file name").to_string(),
                (metadata, file_descriptor),
            );
        }
        self.proto_dependencies = HashMap::new();
        for (_, file_descriptor) in &catalog.proto_descriptors {
            let filename =
                file_descriptor.name.clone().expect("Proto file was expected to have a name");
            self.proto_dependencies.insert(
                filename.clone(),
                get_proto_dependencies(filename.clone(), &all_proto_files),
            );
        }

        self.diagnostics_context =
            Some(DiagnosticsContext::new(catalog).expect("Failed to create diagnostics context"));
    }

    fn start_visit_proto_file(&mut self, node: &ProtoFile) {
        let proto_path = node.0.folder.join(node.1.name());
        if let Some(target) = &self.target_file {
            if &proto_path != target {
                self.should_process_current_file = false;
                return;
            }
        }
        self.should_process_current_file = true;

        self.package = node.1.package().to_string();
        self.enter_file_with_package(self.package.clone());

        // Parse proto file using Pest and store Spans
        let proto_path = node.0.folder.join(node.1.name());
        let mut doc_comments: Vec<String> = vec![];
        if let Ok(proto_content) = self.file_provider.read_to_string(&proto_path) {
            if let Ok(mut parsed_file) = ProtoParser::parse(ProtoRule::proto, &proto_content) {
                for item in parsed_file.next().unwrap().into_inner() {
                    if item.as_rule() == ProtoRule::topLevelDef {
                        let top_level_element = item.into_inner().next().unwrap();
                        let name = top_level_element
                            .clone()
                            .into_inner()
                            .next()
                            .unwrap()
                            .as_str()
                            .to_string();
                        let type_ = match top_level_element.as_rule() {
                            ProtoRule::message => "message",
                            ProtoRule::enum_ => "enum",
                            ProtoRule::extend => "extend",
                            ProtoRule::service => "service",
                            _ => "",
                        };
                        let span = Span {
                            file_path: proto_path.display().to_string(),
                            start: top_level_element.as_span().start() as u32,
                            end: top_level_element.as_span().end() as u32,
                            ..Default::default()
                        };
                        self.current_proto_spans.insert((type_.to_string(), name.clone()), span);
                        self.current_proto_doc_strings
                            .insert((type_.to_string(), name), doc_comments.clone().join("\n"));
                        doc_comments.clear();
                    } else if item.as_rule() == ProtoRule::COMMENT {
                        doc_comments.push(Self::extract_comment_text(item));
                    } else {
                        doc_comments.clear();
                    }
                }
            }
        } else {
            self.current_proto_spans = HashMap::new();
        }

        self.proto_metadata = ProtoMetadata {
            folder: node.0.folder.clone(),
            filename: node.1.name().to_string(),
            rust_build_target_name: node.0.target_name.clone(),
            rust_crate_name: node.0.crate_name.clone(),
        };
        self.proto_comes_from_catalog = if let Some(catalog_path) = &self.catalog.catalog_path {
            // Canonicalize paths before comparison to handle symbolic links or different path representations
            match (node.0.folder.canonicalize(), catalog_path.canonicalize()) {
                (Ok(node_folder), Ok(catalog_p)) => node_folder.starts_with(&catalog_p),
                _ => false, // Paths could not be canonicalized, default to false
            }
        } else {
            false // Default to false if catalog_path is not set
        };
        self.processed_files += 1;
        self.printer.print_next_progress_step(
            format!("Importing \"{}\"", node.1.name.clone().unwrap_or_default()).as_ref(),
        );
    }

    fn start_visit_proto_descriptor(&mut self, node: &DescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        let message_name = node.name().to_string();
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Add scope for proto message (anonymous, i.e., without name, because proto messages place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope(message_name.clone());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        let message_fqn = format!("{}.{}", self.package, message_name);
        let max_size: Option<MessageSize> = message_size::compute_message_size(
            &message_fqn,
            &mut self.messages_computed_sizes,
            &self.message_size_metadata,
        )
        .ok();

        // Add symbol for proto message
        let symbol = ProtoMessageSymbol {
            ast_node: node.clone(),
            proto_metadata: self.proto_metadata.clone(),
            doc_string: self
                .current_proto_doc_strings
                .get(&("message".to_string(), message_name.clone()))
                .cloned()
                .unwrap_or_default(),
            proto_dependencies: self
                .proto_dependencies
                .get(&self.proto_metadata.filename)
                .cloned()
                .unwrap_or_else(Vec::new),
            max_size,
            ..Default::default()
        };
        let span = self.current_proto_spans.get(&("message".to_string(), message_name.clone()));
        let symbol_id = self.symbol_table.create_symbol(
            message_name,
            span.cloned(),
            SymbolKind::ProtoMessage(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn start_visit_proto_single_pub_message(
        &mut self,
        node: &DescriptorProto,
        publication: &SdvPublication,
    ) {
        if !self.should_process_current_file {
            return;
        }
        let message_name = node.name().to_string();
        // The proto message creates a scope. However, since the publication and the message are
        // the same language element, single pub message shall be in the service bundle's scope,
        // not in the message's scope. Therefore, we need to leave the current scope (i.e. the
        // message's scope) before processing the publication. This is resetted in the end_visit
        // function.
        self.message_scope_id = self.current_scope_id.clone();
        self.leave_current_scope_to_enclosing_scope();
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Add scope for proto message (anonymous, i.e., without name, because proto messages place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope(message_name.clone());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        // Add symbol for proto publisher
        let message_fqn = format!("{}.{}", self.package, message_name);
        let message_size: MessageSize = message_size::compute_message_size(
            &message_fqn,
            &mut self.messages_computed_sizes,
            &self.message_size_metadata,
        )
        .unwrap_or_default();

        let symbol = ProtoPublicationSymbol {
            ast_node: node.clone(),
            proto_metadata: self.proto_metadata.clone(),
            proto_dependencies: self
                .proto_dependencies
                .get(&self.proto_metadata.filename)
                .cloned()
                .unwrap_or_else(Vec::new),
            message_count: publication.message_count,
            message_size,
            doc_string: self
                .current_proto_doc_strings
                .get(&("message".to_string(), message_name.clone()))
                .cloned()
                .unwrap_or_default(),
            ..Default::default()
        };
        let span = self.current_proto_spans.get(&("message".to_string(), message_name.clone()));
        let symbol_id = self.symbol_table.create_symbol(
            message_name,
            span.cloned(),
            SymbolKind::ProtoPublication(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn start_visit_proto_multi_pub_message(
        &mut self,
        node: &DescriptorProto,
        publication: &SdvPublication,
    ) {
        if !self.should_process_current_file {
            return;
        }
        let message_name = node.name().to_string();
        // The proto message creates a scope. However, since the publication and the message are
        // the same language element, single pub message shall be in the service bundle's scope,
        // not in the message's scope. Therefore, we need to leave the current scope (i.e. the
        // message's scope) before processing the publication. This is resetted in the end_visit
        // function.
        self.message_scope_id = self.current_scope_id.clone();
        self.leave_current_scope_to_enclosing_scope();
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Add scope for proto message (anonymous, i.e., without name, because proto messages place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope(message_name.clone());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        // Add symbol for proto publisher
        let message_fqn = format!("{}.{}", self.package, message_name);
        let message_size: MessageSize = message_size::compute_message_size(
            &message_fqn,
            &mut self.messages_computed_sizes,
            &self.message_size_metadata,
        )
        .unwrap_or_default();

        let symbol = ProtoPublicationSymbol {
            ast_node: node.clone(),
            proto_metadata: self.proto_metadata.clone(),
            proto_dependencies: self
                .proto_dependencies
                .get(&self.proto_metadata.filename)
                .cloned()
                .unwrap_or_else(Vec::new),
            message_count: publication.message_count,
            message_size,
            model: PublisherModel::MULTI_PUB,
            doc_string: self
                .current_proto_doc_strings
                .get(&("message".to_string(), message_name.clone()))
                .cloned()
                .unwrap_or_default(),
            ..Default::default()
        };
        let span = self.current_proto_spans.get(&("message".to_string(), message_name.clone()));
        let symbol_id = self.symbol_table.create_symbol(
            message_name,
            span.cloned(),
            SymbolKind::ProtoPublication(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn start_visit_proto_enum_descriptor(&mut self, node: &EnumDescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        let enum_name = node.name().to_string();
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Add scope for proto enum (anonymous, i.e., without name, because proto enums place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope(enum_name.clone());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        // Add symbol for proto enum
        let symbol = ProtoEnumSymbol {
            ast_node: node.clone(),
            proto_metadata: self.proto_metadata.clone(),
            doc_string: self
                .current_proto_doc_strings
                .get(&("enum".to_string(), enum_name.clone()))
                .cloned()
                .unwrap_or_default(),
            ..Default::default()
        };
        let span = self.current_proto_spans.get(&("enum".to_string(), enum_name.clone()));
        let symbol_id = self.symbol_table.create_symbol(
            enum_name,
            span.cloned(),
            SymbolKind::ProtoEnum(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn start_visit_proto_service_descriptor(&mut self, node: &ServiceDescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        let service_name = node.name().to_string();
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Add scope for proto service (anonymous, i.e., without name, because proto services place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope(service_name.clone());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        // Add symbol for proto service
        let symbol = ProtoRpcServiceSymbol {
            ast_node: node.clone(),
            source: if self.proto_comes_from_catalog {
                ProtoRpcServiceSource::CATALOG
            } else {
                ProtoRpcServiceSource::DEPENDENCY
            },
            proto_metadata: self.proto_metadata.clone(),
            proto_dependencies: self
                .proto_dependencies
                .get(&self.proto_metadata.filename)
                .cloned()
                .unwrap_or_else(Vec::new),
            doc_string: self
                .current_proto_doc_strings
                .get(&("service".to_string(), service_name.clone()))
                .cloned()
                .unwrap_or_default(),
            ..Default::default()
        };
        let span = self.current_proto_spans.get(&("service".to_string(), service_name.clone()));
        let symbol_id = self.symbol_table.create_symbol(
            service_name,
            span.cloned(),
            SymbolKind::ProtoRpcService(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn end_visit_proto_single_pub_message(
        &mut self,
        _node: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        if !self.should_process_current_file {
            return;
        }
        // Leave publication's scope. Best practice is to leave the current scope and
        // jump to enclosing scope with the below function:
        // self.leave_current_scope_to_enclosing_scope();
        // However, here we reset current scope to message's scope
        self.current_scope_id = self.message_scope_id.clone();
    }

    fn end_visit_proto_multi_pub_message(
        &mut self,
        _node: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        if !self.should_process_current_file {
            return;
        }
        // Leave publication's scope. Best practice is to leave the current scope and
        // jump to enclosing scope with the below function:
        // self.leave_current_scope_to_enclosing_scope();
        // However, here we reset current scope to message's scope
        self.current_scope_id = self.message_scope_id.clone();
    }

    fn end_visit_proto_enum_descriptor(&mut self, _node: &EnumDescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        self.leave_current_scope_to_enclosing_scope();
    }

    fn end_visit_proto_service_descriptor(&mut self, _node: &ServiceDescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        self.leave_current_scope_to_enclosing_scope();
    }

    fn end_visit_proto_descriptor(&mut self, _node: &DescriptorProto) {
        if !self.should_process_current_file {
            return;
        }
        self.leave_current_scope_to_enclosing_scope();
    }

    fn end_visit_proto_file(&mut self, _node: &ProtoFile) {
        // we need to manually reset this as the start_visit function
        // will not be called for dependency catalogs
        self.should_process_current_file = self.target_file.is_none();
    }

    fn start_visit_catalog_vsidl_file(
        &mut self,
        node: &vsidl_v1_common::visitors::visitor::CatalogVsidlFile,
    ) {
        if let Some(target) = &self.target_file {
            if &node.0 != target {
                self.should_process_current_file = false;
                return;
            }
        }
        self.should_process_current_file = true;

        self.bundle_comes_from_catalog = true;
        self.processed_files += 1;
        self.current_vsidl_file = node.0.clone();
        self.printer.print_next_progress_step(format!("Importing {:?}", node.0).as_ref());
    }

    fn start_visit_vsidl_entry(&mut self, node: &VsidlEntry) {
        if !self.should_process_current_file {
            return;
        }
        self.package = node.package.clone();
        self.enter_file_with_package(self.package.clone());
    }

    fn start_visit_service_bundle(&mut self, node: &ServiceBundle) {
        if !self.should_process_current_file {
            return;
        }
        let bundle_name = &node.name;
        let current_scope_id = self.current_scope_id.clone().expect("Package scope must be set");

        // Diagnostics
        // Pass the cached context!
        let diagnostics_info = match DiagnosticsInfo::from(
            self.diagnostics_context.as_mut().expect("Diagnostics context must be initialized"),
            &self.package,
            node,
        ) {
            Ok(info) => info,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to extract diagnostics info for bundle '{}': {}",
                    node.name, e
                );
                None
            }
        };

        // Add scope for service bundle (anonymous, i.e., without name, because service bundles place all their members in their package)
        let new_scope_id = self.symbol_table.create_scope("".to_string());
        self.symbol_table.add_subscope(&current_scope_id, &new_scope_id);

        // Add symbol for service bundle
        let symbol = ServiceBundleSymbol {
            ast_node: node.clone(),
            diagnostics_info,
            source: if self.bundle_comes_from_catalog {
                ServiceBundleSource::CATALOG
            } else {
                ServiceBundleSource::DEPENDENCY
            },
            file_path: self.current_vsidl_file.clone(),
            ..Default::default()
        };
        let symbol_id = self.symbol_table.create_symbol(
            bundle_name.to_string(),
            node.span.as_ref().cloned(),
            SymbolKind::ServiceBundle(symbol),
        );
        self.current_service_bundle_symbol_id = Some(symbol_id.clone());

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);
        self.symbol_table.set_spanning_symbol(&new_scope_id, &symbol_id);
        self.current_scope_id = Some(new_scope_id);
    }

    fn start_visit_publisher(&mut self, node: &Publisher) {
        if !self.should_process_current_file {
            return;
        }
        let publisher_name = &node.message;
        let current_scope_id =
            self.current_scope_id.clone().expect("Service bundle scope must be set");

        // Add symbol for publisher
        let symbol = PublisherSymbol {
            ast_node: node.clone(),
            capacity: node.capacity as u32,
            owning_service_bundle: self.current_service_bundle_symbol_id.clone(),
            ..Default::default()
        };
        let symbol_id = self.symbol_table.create_symbol(
            publisher_name.to_string(),
            node.span.as_ref().cloned(),
            SymbolKind::Publisher(symbol),
        );

        // Create channel symbols if defined inline
        for (i, topic_name) in node.topic.iter().enumerate() {
            if !self.created_channels.contains(topic_name) {
                let topic_symbol = TopicSymbol { ast_node: node.clone(), ..Default::default() };
                let span = node.channel_span.get(i).cloned();
                let id = self.symbol_table.create_symbol(
                    topic_name.clone(),
                    span,
                    SymbolKind::Topic(topic_symbol),
                );
                // Add to global scope
                self.symbol_table.add_symbol_to_scope(&GLOBAL_SCOPE_ID, &id);
                self.created_channels.insert(topic_name.clone());
            };
        }

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);

        // Connect related symbols
        if let Some(service_bundle_symbol) = self.symbol_table.get_service_bundle_symbol_mut(
            &self
                .current_service_bundle_symbol_id
                .clone()
                .expect("Service bundle symbol must be set"),
        ) {
            service_bundle_symbol.publishers.insert(symbol_id);
        } else {
            eprintln!("Could not add publisher symbol to service bundle symbol");
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        if !self.should_process_current_file {
            return;
        }
        let subscriber_name = &node.message;
        let current_scope_id =
            self.current_scope_id.clone().expect("Service bundle scope must be set");

        // Add symbol for subscriber
        let symbol = SubscriberSymbol {
            ast_node: node.clone(),
            owning_service_bundle: self.current_service_bundle_symbol_id.clone(),
            ..Default::default()
        };
        let symbol_id = self.symbol_table.create_symbol(
            subscriber_name.to_string(),
            node.span.as_ref().cloned(),
            SymbolKind::Subscriber(symbol),
        );

        // Create channel symbols if defined inline
        for (i, topic_name) in node.topic.iter().enumerate() {
            if !self.created_channels.contains(topic_name) {
                let topic_symbol =
                    TopicSymbol { ast_node: Publisher::default(), ..Default::default() };
                let span = node.channel_span.get(i).cloned();
                let id = self.symbol_table.create_symbol(
                    topic_name.clone(),
                    span,
                    SymbolKind::Topic(topic_symbol),
                );
                // Add to global scope
                self.symbol_table.add_symbol_to_scope(&GLOBAL_SCOPE_ID, &id);
                self.created_channels.insert(topic_name.clone());
            };
        }

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);

        // Connect related symbols
        if let Some(service_bundle_symbol) = self.symbol_table.get_service_bundle_symbol_mut(
            &self
                .current_service_bundle_symbol_id
                .clone()
                .expect("Service bundle symbol must be set"),
        ) {
            service_bundle_symbol.subscribers.insert(symbol_id);
        } else {
            eprintln!("Could not add subscriber symbol to service bundle symbol");
        }
    }

    fn start_visit_server(&mut self, node: &Server) {
        if !self.should_process_current_file {
            return;
        }
        let service_name = &node.service;
        let current_scope_id =
            self.current_scope_id.clone().expect("Service bundle scope must be set");

        // Create RPC channel symbol if it doesn't exist
        let channel_name = node.channel.clone();
        if !channel_name.is_empty() && !self.created_rpc_channels.contains(&channel_name) {
            let rpc_channel_symbol = RpcChannelSymbol { ..Default::default() };
            let id = self.symbol_table.create_symbol(
                channel_name.clone(),
                node.channel_span.clone().into_option(),
                SymbolKind::RpcChannel(rpc_channel_symbol),
            );
            // Add to global scope
            self.symbol_table.add_symbol_to_scope(&GLOBAL_SCOPE_ID, &id);
            self.created_rpc_channels.insert(channel_name);
        }

        // Add symbol for RPC server
        let symbol = RpcServerSymbol {
            ast_node: node.clone(),
            owning_service_bundle: self.current_service_bundle_symbol_id.clone(),
            ..Default::default()
        };
        let symbol_id = self.symbol_table.create_symbol(
            service_name.to_string(),
            node.span.as_ref().cloned(),
            SymbolKind::RpcServer(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);

        // Connect related symbols
        if let Some(service_bundle_symbol) = self.symbol_table.get_service_bundle_symbol_mut(
            &self
                .current_service_bundle_symbol_id
                .clone()
                .expect("Service bundle symbol must be set"),
        ) {
            service_bundle_symbol.servers.insert(symbol_id);
        } else {
            eprintln!("Could not add RPC server symbol to service bundle symbol");
        }
    }

    fn start_visit_client(&mut self, node: &Client) {
        if !self.should_process_current_file {
            return;
        }
        let service_name = &node.service;
        let current_scope_id =
            self.current_scope_id.clone().expect("Service bundle scope must be set");

        // Create RPC channel symbol if it doesn't exist
        let channel_name = node.channel.clone();
        if !channel_name.is_empty() && !self.created_rpc_channels.contains(&channel_name) {
            let rpc_channel_symbol = RpcChannelSymbol { ..Default::default() };
            let id = self.symbol_table.create_symbol(
                channel_name.clone(),
                node.channel_span.clone().into_option(),
                SymbolKind::RpcChannel(rpc_channel_symbol),
            );
            // Add to global scope
            self.symbol_table.add_symbol_to_scope(&GLOBAL_SCOPE_ID, &id);
            self.created_rpc_channels.insert(channel_name);
        }

        // Add symbol for RPC client
        let symbol = RpcClientSymbol {
            ast_node: node.clone(),
            owning_service_bundle: self.current_service_bundle_symbol_id.clone(),
            ..Default::default()
        };
        let symbol_id = self.symbol_table.create_symbol(
            service_name.to_string(),
            node.span.as_ref().cloned(),
            SymbolKind::RpcClient(symbol),
        );

        // Connect symbol and scope
        self.symbol_table.add_symbol_to_scope(&current_scope_id, &symbol_id);

        // Connect related symbols
        if let Some(service_bundle_symbol) = self.symbol_table.get_service_bundle_symbol_mut(
            &self
                .current_service_bundle_symbol_id
                .clone()
                .expect("Service bundle symbol must be set"),
        ) {
            service_bundle_symbol.clients.insert(symbol_id);
        } else {
            eprintln!("Could not add RPC client symbol to service bundle symbol");
        }
    }

    fn end_visit_service_bundle(&mut self, _node: &ServiceBundle) {
        if !self.should_process_current_file {
            return;
        }
        self.leave_current_scope_to_enclosing_scope();
        self.current_service_bundle_symbol_id = None;
    }

    fn end_visit_vsidl_entry(&mut self, _node: &VsidlEntry) {
        if !self.should_process_current_file {
            return;
        }
        // after finishing the current vsidl entry, the scope goes back to global
        self.current_scope_id = Some(
            self.symbol_table
                .get_scope(&self.global_scope().unwrap())
                .expect("Global scope should be present")
                .id
                .clone(),
        );
    }

    fn end_visit_catalog_vsidl_file(
        &mut self,
        _node: &vsidl_v1_common::visitors::visitor::CatalogVsidlFile,
    ) {
        if !self.should_process_current_file {
            return;
        }
        // we need to manually reset this as the start_visit function
        // will not be called for dependency catalogs
        self.should_process_current_file = self.target_file.is_none();

        self.bundle_comes_from_catalog = false;
        self.current_vsidl_file = PathBuf::new();
    }

    fn end_visit_catalog(&mut self, _catalog: &Catalog) {
        self.symbol_table.set_state(SymbolTableState::CreatedNotLinked);
        self.proto_metadata = ProtoMetadata::default();
        self.message_size_metadata = MessageSizeMetadataSet::default();
        self.messages_computed_sizes = IndexMap::new();
        self.proto_dependencies = HashMap::new();
        self.diagnostics_context = None;
    }
}

impl Default for SymbolTableCreator<DiskFileProvider> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolKind;
    use crate::GLOBAL_SCOPE_ID;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_create_scopes_and_symbols_main_catalog() {
        // GIVEN A catalog with a service bundle
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // WHEN / THEN
        test_create_scopes_and_symbols(&catalog);
    }

    #[test]
    fn test_create_scopes_and_symbols_dependency_catalog() {
        // GIVEN A catalog with a service bundle
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .dependencies_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone());

        // WHEN / THEN
        test_create_scopes_and_symbols(&catalog);
    }

    fn test_create_scopes_and_symbols(catalog: &Catalog) {
        // WHEN Executing the symbol table creator
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // THEN Should include global scope
        let global_scope = creator
            .symbol_table()
            .get_scope(&creator.global_scope().unwrap())
            .expect("Global scope should be present");
        assert_eq!(global_scope.name, "");
        assert_eq!(global_scope.enclosing_scope, None);
        assert_eq!(global_scope.id, GLOBAL_SCOPE_ID);

        // THEN service bundle symbols should be present
        let service_bundles_one = creator.symbol_table().resolve(
            &creator.global_scope().unwrap(),
            "sdv.test.ServiceBundleOne",
            false,
        );
        assert_eq!(service_bundles_one.len(), 1);
        let service_bundle_symbol1 = service_bundles_one[0];
        let service_bundle_scope_id1 = service_bundle_symbol1
            .spanned_scope()
            .clone()
            .expect("Service bundle scope should be present");
        let service_bundle_scope1 = creator
            .symbol_table()
            .get_scope(&service_bundle_scope_id1)
            .expect("ServiceBundleOne scope should be present");
        assert_eq!(
            service_bundle_symbol1.full_name(creator.symbol_table()),
            "sdv.test.ServiceBundleOne"
        );
        assert_ne!(service_bundle_symbol1.symbol().enclosing_scope, global_scope.id);

        let service_bundles_two = creator.symbol_table().resolve(
            &creator.global_scope().unwrap(),
            "sdv.test.ServiceBundleTwo",
            false,
        );
        assert_eq!(service_bundles_two.len(), 1);
        let service_bundle_symbol2 = service_bundles_two[0];
        let service_bundle_scope_id2 = service_bundle_symbol2
            .spanned_scope()
            .clone()
            .expect("Service bundle scope should be present");
        let service_bundle_scope2 = creator
            .symbol_table()
            .get_scope(&service_bundle_scope_id2)
            .expect("ServiceBundleTwo scope should be present");
        assert_eq!(
            service_bundle_symbol2.full_name(creator.symbol_table()),
            "sdv.test.ServiceBundleTwo"
        );
        assert_ne!(service_bundle_symbol2.symbol().enclosing_scope, global_scope.id);
        assert_ne!(
            service_bundle_symbol2.enclosing_scope(creator.symbol_table()).id,
            global_scope.id
        );

        // THEN publisher symbols should be present
        let publisher_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("Pressure")
            .expect("Publisher symbol 'Pressure' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::Publisher(_))
            })
            .collect();
        eprintln!("Publisher symbols: {publisher_symbol_ids:?}");
        // ServiceBundleOne contains two publishers for type "Pressure"
        assert_eq!(publisher_symbol_ids.len(), 2);
        let publisher_symbol_id1 = &publisher_symbol_ids[0];
        let publisher_symbol1 = creator
            .symbol_table()
            .get_symbol(publisher_symbol_id1)
            .expect("Publisher symbol 'Pressure' should be present");
        assert_eq!(publisher_symbol1.name(), "Pressure");
        assert_ne!(publisher_symbol1.symbol().enclosing_scope, global_scope.id);
        assert_ne!(publisher_symbol1.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &publisher_symbol1 {
            SymbolKind::Publisher(publisher_symbol) => {
                publisher_symbol.owning_service_bundle.clone()
            }
            _ => panic!("Expected a Publisher symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol1.id().clone()));

        // THEN subscriber symbols should be present
        let subscriber_symbol_ids: Vec<_> = service_bundle_scope2
            .symbols
            .get("Pressure")
            .expect("Subscriber symbol 'Pressure' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::Subscriber(_))
            })
            .collect();
        eprintln!("Subscriber symbols: {subscriber_symbol_ids:?}");
        assert_eq!(subscriber_symbol_ids.len(), 1);
        let subscriber_symbol_id1 = &subscriber_symbol_ids[0];
        let subscriber_symbol1 = creator
            .symbol_table()
            .get_symbol(subscriber_symbol_id1)
            .expect("Subscriber symbol 'Pressure' should be present");
        assert_eq!(subscriber_symbol1.name(), "Pressure");
        assert_ne!(subscriber_symbol1.symbol().enclosing_scope, global_scope.id);
        assert_ne!(subscriber_symbol1.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &subscriber_symbol1 {
            SymbolKind::Subscriber(subscriber_symbol) => {
                subscriber_symbol.owning_service_bundle.clone()
            }
            _ => panic!("Expected a Subscriber symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol2.id().clone()));

        // THEN RPC server symbols should be present
        let rpc_server_symbol_ids: Vec<_> = service_bundle_scope2
            .symbols
            .get("DummyRpc")
            .expect("RPC server symbol 'DummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcServer(_))
            })
            .collect();
        assert_eq!(rpc_server_symbol_ids.len(), 1);
        let rpc_server_symbol_id1 = &rpc_server_symbol_ids[0];
        let rpc_server_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_server_symbol_id1)
            .expect("RPC server symbol 'DummyRpc' should be present");
        assert_eq!(rpc_server_symbol1.name(), "DummyRpc");
        assert_ne!(rpc_server_symbol1.symbol().enclosing_scope, global_scope.id);
        assert_ne!(rpc_server_symbol1.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &rpc_server_symbol1 {
            SymbolKind::RpcServer(rpc_server) => rpc_server.owning_service_bundle.clone(),
            _ => panic!("Expected an RPC Server symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol2.id().clone()));

        let rpc_server_symbol_ids: Vec<_> = service_bundle_scope2
            .symbols
            .get("AnotherDummyRpc")
            .expect("RPC server symbol 'AnotherDummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcServer(_))
            })
            .collect();
        assert_eq!(rpc_server_symbol_ids.len(), 1);
        let rpc_server_symbol_id2 = &rpc_server_symbol_ids[0];
        let rpc_server_symbol2 = creator
            .symbol_table()
            .get_symbol(rpc_server_symbol_id2)
            .expect("RPC server symbol 'AnotherDummyRpc' should be present");
        assert_eq!(rpc_server_symbol2.name(), "AnotherDummyRpc");
        assert_ne!(rpc_server_symbol2.symbol().enclosing_scope, global_scope.id);
        assert_ne!(rpc_server_symbol2.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &rpc_server_symbol2 {
            SymbolKind::RpcServer(rpc_server) => rpc_server.owning_service_bundle.clone(),
            _ => panic!("Expected an RPC Server symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol2.id().clone()));

        // THEN RPC client symbols should be present
        let rpc_client_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("DummyRpc")
            .expect("RPC client symbol 'DummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcClient(_))
            })
            .collect();
        assert_eq!(rpc_client_symbol_ids.len(), 1);
        let rpc_client_symbol_id1 = &rpc_client_symbol_ids[0];
        let rpc_client_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_client_symbol_id1)
            .expect("RPC client symbol 'DummyRpc' should be present");
        assert_eq!(rpc_client_symbol1.name(), "DummyRpc");
        assert_ne!(rpc_client_symbol1.symbol().enclosing_scope, global_scope.id);
        assert_ne!(rpc_client_symbol1.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &rpc_client_symbol1 {
            SymbolKind::RpcClient(rpc_client) => rpc_client.owning_service_bundle.clone(),
            _ => panic!("Expected an RPC Client symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol1.id().clone()));

        let rpc_client_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("AnotherDummyRpc")
            .expect("RPC client symbol 'AnotherDummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcClient(_))
            })
            .collect();
        assert_eq!(rpc_client_symbol_ids.len(), 1);
        let rpc_client_symbol_id1 = &rpc_client_symbol_ids[0];
        let rpc_client_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_client_symbol_id1)
            .expect("RPC client symbol 'AnotherDummyRpc' should be present");
        assert_eq!(rpc_client_symbol1.name(), "AnotherDummyRpc");
        assert_ne!(rpc_client_symbol1.symbol().enclosing_scope, global_scope.id);
        assert_ne!(rpc_client_symbol1.enclosing_scope(creator.symbol_table()).id, global_scope.id);
        let owning_service_bundle = match &rpc_client_symbol1 {
            SymbolKind::RpcClient(rpc_client) => rpc_client.owning_service_bundle.clone(),
            _ => panic!("Expected an RPC Client symbol"),
        };
        assert_eq!(owning_service_bundle, Some(service_bundle_symbol1.id().clone()));

        // THEN Verify that the service bundle symbols has all the related symbols
        let service_bundle_symbol_unwrapped1 = match &service_bundle_symbol1 {
            SymbolKind::ServiceBundle(symbol) => symbol,
            _ => panic!("Expected a ServiceBundle symbol"),
        };
        assert_eq!(service_bundle_symbol_unwrapped1.publishers.len(), 2);
        assert_eq!(service_bundle_symbol_unwrapped1.servers.len(), 0);

        let service_bundle_symbol_unwrapped2 = match &service_bundle_symbol2 {
            SymbolKind::ServiceBundle(symbol) => symbol,
            _ => panic!("Expected a ServiceBundle symbol"),
        };
        assert_eq!(service_bundle_symbol_unwrapped2.publishers.len(), 0);
        assert_eq!(service_bundle_symbol_unwrapped2.servers.len(), 2);
    }

    #[test]
    fn test_full_name_resolution() {
        // GIVEN A catalog with a service bundle
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // WHEN Executing the symbol table creator
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // THEN full names shall match the full names from the input files
        let global_scope = creator
            .symbol_table()
            .get_scope(&creator.global_scope().unwrap())
            .expect("Global scope should be present");
        let service_bundle_symbol1 = creator.symbol_table().resolve(
            &global_scope.id.clone(),
            "sdv.test.ServiceBundleOne",
            false,
        )[0];
        let service_bundle_scope_id1 = service_bundle_symbol1
            .spanned_scope()
            .clone()
            .expect("Service bundle scope should be present");
        let service_bundle_scope1 = creator
            .symbol_table()
            .get_scope(&service_bundle_scope_id1)
            .expect("ServiceBundleOne scope should be present");
        assert_eq!(
            service_bundle_symbol1.full_name(creator.symbol_table()),
            "sdv.test.ServiceBundleOne"
        );

        let publisher_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("Pressure")
            .expect("Publisher symbol 'Pressure' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::Publisher(_))
            })
            .collect();
        // ServiceBundleOne contains two publishers for type "Pressure"
        assert_eq!(publisher_symbol_ids.len(), 2);
        let publisher_symbol_id1 = &publisher_symbol_ids[0];
        let publisher_symbol1 = creator
            .symbol_table()
            .get_symbol(publisher_symbol_id1)
            .expect("Publisher symbol 'Pressure' should be present");
        assert_eq!(publisher_symbol1.full_name(creator.symbol_table()), "sdv.test.Pressure");

        let service_bundle_symbol2 = creator.symbol_table().resolve(
            &global_scope.id.clone(),
            "sdv.test.ServiceBundleTwo",
            false,
        )[0];
        let service_bundle_scope_id2 = service_bundle_symbol2
            .spanned_scope()
            .clone()
            .expect("Service bundle scope should be present");
        let service_bundle_scope2 = creator
            .symbol_table()
            .get_scope(&service_bundle_scope_id2)
            .expect("ServiceBundleTwo scope should be present");
        assert_eq!(
            service_bundle_symbol2.full_name(creator.symbol_table()),
            "sdv.test.ServiceBundleTwo"
        );

        let rpc_server_symbol_ids: Vec<_> = service_bundle_scope2
            .symbols
            .get("DummyRpc")
            .expect("RPC server symbol 'DummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcServer(_))
            })
            .collect();
        assert_eq!(rpc_server_symbol_ids.len(), 1);
        let rpc_server_symbol_id1 = &rpc_server_symbol_ids[0];
        let rpc_server_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_server_symbol_id1)
            .expect("RPC server symbol 'DummyRpc' should be present");
        assert_eq!(rpc_server_symbol1.full_name(creator.symbol_table()), "sdv.test.DummyRpc");

        let rpc_server_symbol_ids: Vec<_> = service_bundle_scope2
            .symbols
            .get("AnotherDummyRpc")
            .expect("RPC server symbol 'AnotherDummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcServer(_))
            })
            .collect();
        assert_eq!(rpc_server_symbol_ids.len(), 1);
        let rpc_server_symbol_id2 = &rpc_server_symbol_ids[0];
        let rpc_server_symbol2 = creator
            .symbol_table()
            .get_symbol(rpc_server_symbol_id2)
            .expect("RPC server symbol 'AnotherDummyRpc' should be present");
        assert_eq!(
            rpc_server_symbol2.full_name(creator.symbol_table()),
            "sdv.test.AnotherDummyRpc"
        );

        let rpc_client_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("DummyRpc")
            .expect("RPC client symbol 'DummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcClient(_))
            })
            .collect();
        assert_eq!(rpc_client_symbol_ids.len(), 1);
        let rpc_client_symbol_id1 = &rpc_client_symbol_ids[0];
        let rpc_client_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_client_symbol_id1)
            .expect("RPC client symbol 'DummyRpc' should be present");
        assert_eq!(rpc_client_symbol1.full_name(creator.symbol_table()), "sdv.test.DummyRpc");

        let rpc_client_symbol_ids: Vec<_> = service_bundle_scope1
            .symbols
            .get("AnotherDummyRpc")
            .expect("RPC client symbol 'AnotherDummyRpc' should be present")
            .iter()
            .filter(|id| {
                matches!(creator.symbol_table().get_symbol(id).unwrap(), SymbolKind::RpcClient(_))
            })
            .collect();
        assert_eq!(rpc_client_symbol_ids.len(), 1);
        let rpc_client_symbol_id1 = &rpc_client_symbol_ids[0];
        let rpc_client_symbol1 = creator
            .symbol_table()
            .get_symbol(rpc_client_symbol_id1)
            .expect("RPC client symbol 'AnotherDummyRpc' should be present");
        assert_eq!(
            rpc_client_symbol1.full_name(creator.symbol_table()),
            "sdv.test.AnotherDummyRpc"
        );
    }

    #[test]
    fn test_create_scopes_and_symbols_for_proto() {
        // GIVEN A catalog with a proto file
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        // WHEN Executing the symbol table creator
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // THEN Should include global scope
        let global_scope = creator
            .symbol_table()
            .get_scope(&creator.global_scope().unwrap())
            .expect("Global scope should be present");
        assert_eq!(global_scope.name, "");
        assert_eq!(global_scope.enclosing_scope, None);

        // THEN proto message symbols should be present
        let message_symbols: Vec<_> = creator
            .symbol_table()
            .resolve(&global_scope.id.clone(), "sdv.test.DummyRequest", false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoMessage(_)))
            .collect();
        assert_eq!(message_symbols.len(), 1);
        let message_symbol = message_symbols[0];
        let message_scope_id = message_symbol
            .spanned_scope()
            .clone()
            .expect("DummyRequest message scope should be present");
        let _message_scope = creator
            .symbol_table()
            .get_scope(&message_scope_id)
            .expect("DummyRequest message scope should be present");
        assert_eq!(message_symbol.full_name(creator.symbol_table()), "sdv.test.DummyRequest");

        // THEN proto enum symbols should be present
        let enum_symbols: Vec<_> = creator
            .symbol_table()
            .resolve(&global_scope.id.clone(), "sdv.test.BatteryChannels", false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoEnum(_)))
            .collect();
        assert_eq!(enum_symbols.len(), 1);
        let enum_symbol = enum_symbols[0];
        let enum_scope_id =
            enum_symbol.spanned_scope().clone().expect("BatteryChannels scope should be present");
        assert_eq!(enum_symbol.full_name(creator.symbol_table()), "sdv.test.BatteryChannels");
        let _enum_scope =
            creator.symbol_table().get_scope(&enum_scope_id).expect("Enum scope should be present");

        // THEN proto service symbols should be present
        let service_symbols: Vec<_> = creator
            .symbol_table()
            .resolve(&global_scope.id.clone(), "sdv.test.DummyRpc", false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoRpcService(_)))
            .collect();
        assert_eq!(service_symbols.len(), 1);
        let service_symbol = service_symbols[0];
        let service_scope_id =
            service_symbol.spanned_scope().clone().expect("DummyRpc scope should be present");
        let _service_scope = creator
            .symbol_table()
            .get_scope(&service_scope_id)
            .expect("DummyRpc scope should be present");
        assert_eq!(service_symbol.full_name(creator.symbol_table()), "sdv.test.DummyRpc");

        // THEN proto publication symbols should be present (SINGLEPUB case)
        let publication_symbols: Vec<_> = creator
            .symbol_table()
            .resolve(&global_scope.id.clone(), "sdv.test.FuelLevel", false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoPublication(_)))
            .collect();
        assert_eq!(publication_symbols.len(), 1);
        let publication_symbol = publication_symbols[0];
        let publication_scope_id = publication_symbol
            .spanned_scope()
            .clone()
            .expect("FuelLevel publication scope should be present");
        let _publication_scope = creator
            .symbol_table()
            .get_scope(&publication_scope_id)
            .expect("FuelLevel publication scope should be present");
        assert_eq!(publication_symbol.full_name(creator.symbol_table()), "sdv.test.FuelLevel");
        if let SymbolKind::ProtoPublication(publication_symbol) = &publication_symbol {
            assert_eq!(publication_symbol.model, PublisherModel::SINGLE_PUB);
            assert!(publication_symbol.is_single_pub());
        } else {
            panic!("Publication symbol does not have SymbolKind::ProtoPublication");
        }

        // THEN proto publication symbols should be present (MULTIPUB case)
        let publication_symbols: Vec<_> = creator
            .symbol_table()
            .resolve(&global_scope.id.clone(), "sdv.test.Speed", false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoPublication(_)))
            .collect();
        assert_eq!(publication_symbols.len(), 1);
        let publication_symbol = publication_symbols[0];
        let publication_scope_id = publication_symbol
            .spanned_scope()
            .clone()
            .expect("Speed publication scope should be present");
        let _publication_scope = creator
            .symbol_table()
            .get_scope(&publication_scope_id)
            .expect("Speed publication scope should be present");
        assert_eq!(publication_symbol.full_name(creator.symbol_table()), "sdv.test.Speed");
        if let SymbolKind::ProtoPublication(publication_symbol) = &publication_symbol {
            assert_eq!(publication_symbol.model, PublisherModel::MULTI_PUB);
            assert!(publication_symbol.is_multi_pub());
        } else {
            panic!("Publication symbol does not have SymbolKind::ProtoPublication");
        }
    }

    #[test]
    fn test_create_with_wrong_visitor_calls_does_not_work() {
        // GIVEN A catalog with a service bundle
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // WHEN deliberately calling visitor functions in the wrong order
        // (service bundle members wrongly called before service bundle itself)
        let mut creator = SymbolTableCreator {
            current_service_bundle_symbol_id: Some(SymbolId(999)),
            ..Default::default()
        };
        creator.start_visit_catalog(&catalog);
        creator.start_visit_vsidl_entry(&catalog.catalog_vsidl_files[0].1);
        creator.start_visit_publisher(
            &catalog.catalog_vsidl_files[0].1. // VSIDL Entry
            service_bundle[0]
                .publisher[0],
        );
        creator.start_visit_subscriber(
            &catalog.catalog_vsidl_files[0].1. // VSIDL Entry
            service_bundle[1]
                .subscriber[0],
        );
        creator.start_visit_server(
            &catalog.catalog_vsidl_files[0].1. // VSIDL Entry
            service_bundle[1]
                .server[0],
        );
        creator.start_visit_client(
            &catalog.catalog_vsidl_files[0].1. // VSIDL Entry
            service_bundle[0]
                .client[0],
        );
        creator.start_visit_service_bundle(
            &catalog.catalog_vsidl_files[0].1. // VSIDL Entry
            service_bundle[0],
        );

        // THEN service bundle is broken
        let service_bundle = creator.symbol_table().get_all_service_bundle_symbols()[0];
        assert_eq!(service_bundle.publishers.len(), 0);
        assert_eq!(service_bundle.servers.len(), 0);
        assert_eq!(service_bundle.clients.len(), 0);
        assert_eq!(service_bundle.subscribers.len(), 0);
    }
}
