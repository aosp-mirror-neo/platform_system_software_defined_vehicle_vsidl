// Copyright 2024 Google LLC
//! Provides runtime vsidl configuration for the SDV VM.
use crate::factory::{Factory, FactoryImpl};
use crate::types::{DataSource, DataType};
use protobuf::reflect::MessageDescriptor;
use sdv_comms::ContextRef;
use sdv_comms::{id::ServiceFqin, sd::UnitType};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Display;
use std::io::Error as IoError;
use std::string::String;
use std::time::SystemTime;
use thiserror::Error;
use vsidl_v1::{diagnostics_syntax::DiagnosticsDeclaration, someip::SomeIpMapping};

use crate::vsidl_provider_api::{
    PublicationDescriptor, RpcMethodDescriptor, VsidlConfigProviderV1,
};

const VSIDL_CONFIG_DIR: &str = "/etc/vsidl_v1/vsidl_config";

/// Create new instance of VsidlConfigProviderV1 implementation, with access to Diagnostics-related data.
///
/// To be used from Diagnostics agent.
pub fn new_for_diagnostics_agent(ctx: ContextRef) -> impl VsidlConfigProviderV1 {
    new_for_diagnostics_agent_with_injection(FactoryImpl, ctx)
}

/// Create new instance of VsidlProvider, with access to data on local VM only.
///
/// To be used from VSIDL provider agent.
pub fn new_for_vsidl_provider_agent() -> impl VsidlConfigProviderV1 {
    new_for_vsidl_provider_agent_with_injection(FactoryImpl)
}

/// Create new instance of VsidlProvider, with access to SOME/IP-related data only.
///
/// To be used from SOME/IP broker agent.
pub fn new_for_someip_broker_agent() -> impl VsidlConfigProviderV1 {
    new_for_someip_broker_agent_with_injection(FactoryImpl)
}

/// Create new instance of VsidlProvider, with access to VSIDL schemas data only via the VSIDL provider agent only.
///
/// To be used from Telemetry agent.
pub fn new_for_telemetry_agent(ctx: ContextRef) -> impl VsidlConfigProviderV1 {
    new_for_telemetry_agent_with_injection(FactoryImpl, ctx)
}

/// Create new instance of VsidlProvider, with access to all data via the VSIDL provider agent only.
///
/// To be used from integration test for cross-VM VSIDL provider usage.
pub fn new_for_integration_test(ctx: ContextRef) -> impl VsidlConfigProviderV1 {
    new_for_integration_test_with_injection(FactoryImpl, ctx)
}

fn new_for_vsidl_provider_agent_with_injection(
    factory: impl Factory,
) -> impl VsidlConfigProviderV1 {
    let data_types =
        HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]);

    let mut providers = HashMap::new();

    factory
        .provider_from_files(HashSet::from([DataType::VsidlSchemas]), VSIDL_CONFIG_DIR)
        .map(|provider| providers.insert(DataSource::Files, provider));

    factory
        .provider_from_apexes(data_types)
        .map(|provider| providers.insert(DataSource::Apexes, provider));

    let mut data_sources = HashMap::new();
    data_sources.insert(DataType::SomeIp, vec![DataSource::Apexes]);
    data_sources.insert(DataType::Diagnostics, vec![DataSource::Apexes]);
    data_sources.insert(DataType::VsidlSchemas, vec![DataSource::Apexes, DataSource::Files]);

    VsidlProviderV1 { data_sources, providers }
}

fn new_for_diagnostics_agent_with_injection(
    factory: impl Factory,
    ctx: ContextRef,
) -> impl VsidlConfigProviderV1 {
    let mut providers = HashMap::new();

    factory
        .provider_from_files(HashSet::from([DataType::VsidlSchemas]), VSIDL_CONFIG_DIR)
        .map(|provider| providers.insert(DataSource::Files, provider));

    factory
        .provider_from_apexes(HashSet::from([DataType::Diagnostics]))
        .map(|provider| providers.insert(DataSource::Apexes, provider));

    factory
        .provider_from_remote_agent(HashSet::from([DataType::Diagnostics]), ctx)
        .map(|provider| providers.insert(DataSource::Agent, provider));

    let mut data_sources = HashMap::new();
    data_sources.insert(DataType::Diagnostics, vec![DataSource::Agent, DataSource::Apexes]);
    data_sources.insert(DataType::VsidlSchemas, vec![DataSource::Files]);

    VsidlProviderV1 { data_sources, providers }
}

fn new_for_someip_broker_agent_with_injection(factory: impl Factory) -> impl VsidlConfigProviderV1 {
    let mut providers = HashMap::new();

    factory
        .provider_from_files(HashSet::from([DataType::VsidlSchemas]), VSIDL_CONFIG_DIR)
        .map(|provider| providers.insert(DataSource::Files, provider));

    factory
        .provider_from_apexes(HashSet::from([DataType::SomeIp]))
        .map(|provider| providers.insert(DataSource::Apexes, provider));

    let mut data_sources = HashMap::new();
    data_sources.insert(DataType::SomeIp, vec![DataSource::Apexes]);
    data_sources.insert(DataType::VsidlSchemas, vec![DataSource::Files]);

    VsidlProviderV1 { data_sources, providers }
}

fn new_for_telemetry_agent_with_injection(
    factory: impl Factory,
    ctx: ContextRef,
) -> impl VsidlConfigProviderV1 {
    let data_types = HashSet::from([DataType::VsidlSchemas]);

    let mut providers = HashMap::new();

    factory
        .provider_from_agent(data_types, ctx)
        .map(|provider| providers.insert(DataSource::Agent, provider));

    let mut data_sources = HashMap::new();
    data_sources.insert(DataType::VsidlSchemas, vec![DataSource::Agent]);

    VsidlProviderV1 { data_sources, providers }
}

fn new_for_integration_test_with_injection(
    factory: impl Factory,
    ctx: ContextRef,
) -> impl VsidlConfigProviderV1 {
    let data_types =
        HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]);

    let mut providers = HashMap::new();

    factory
        .provider_from_agent(data_types, ctx)
        .map(|provider| providers.insert(DataSource::Agent, provider));

    let mut data_sources = HashMap::new();
    data_sources.insert(DataType::VsidlSchemas, vec![DataSource::Agent]);
    data_sources.insert(DataType::Diagnostics, vec![DataSource::Agent]);
    data_sources.insert(DataType::SomeIp, vec![DataSource::Agent]);

    VsidlProviderV1 { data_sources, providers }
}

/// Vsidl Provider errors
#[derive(Error, Debug)]
pub(crate) enum VsidlProviderErrorV1 {
    /// Configuration file not found error
    #[error("Configuration file not found")]
    ConfigNotFound(#[from] IoError),
    /// Parsing error while reading configuration file
    #[error("Configuration file {0} could not be parsed")]
    ConfigParseError(String),
}

/// Implementation of VsidlConfigProvider
struct VsidlProviderV1 {
    data_sources: HashMap<DataType, Vec<DataSource>>,
    providers: HashMap<DataSource, Box<dyn VsidlConfigProviderV1>>,
}

impl Display for VsidlProviderV1 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.providers
            .iter()
            .try_for_each(|(data_source, provider)| write!(f, "{data_source:?}: {provider}"))
    }
}

#[async_trait::async_trait]
impl VsidlConfigProviderV1 for VsidlProviderV1 {
    #[tracing::instrument(skip_all, fields(source_fqin = source_fqin.get_service_bundle_name()))]
    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        let data_sources = self.data_sources.get(&DataType::SomeIp)?;

        for data_source in data_sources {
            let result = match self.providers.get(data_source).as_ref() {
                Some(provider) => provider.get_someip_mappings(source_fqin).await,
                None => None,
            };

            if result.is_some() {
                return result;
            }
        }

        None
    }

    #[tracing::instrument(skip_all, fields(fqin = fqin.get_service_bundle_name()))]
    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration> {
        let data_sources = self.data_sources.get(&DataType::Diagnostics)?;

        for data_source in data_sources {
            let result = match self.providers.get(data_source).as_ref() {
                Some(provider) => provider.get_diagnostics_declaration(fqin).await,
                None => None,
            };

            if result.is_some() {
                return result;
            }
        }

        None
    }

    #[tracing::instrument(skip_all, fields(
        unit_type_name = unit_type.get_unit_type_name(),
        vm_name = source_fqin.get_sdv_vm_name(),
        await_start_timestamp=SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(
            |e| e.duration()
        ).as_nanos()
    ))]
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        let data_sources = self.data_sources.get(&DataType::VsidlSchemas)?;

        for data_source in data_sources {
            let result = match self.providers.get(data_source).as_ref() {
                Some(provider) => provider.get_publication_descriptor(source_fqin, unit_type).await,
                None => None,
            };

            if result.is_some() {
                return result;
            }
        }

        None
    }

    #[tracing::instrument(skip_all, fields(unit_type_name = unit_type.get_unit_type_name(), vm_name = source_fqin.get_sdv_vm_name()))]
    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        let data_sources = self.data_sources.get(&DataType::VsidlSchemas)?;

        for data_source in data_sources {
            let result = match self.providers.get(data_source).as_ref() {
                Some(provider) => {
                    provider.get_rpc_method_descriptor(source_fqin, unit_type, method_name).await
                }
                None => None,
            };

            if result.is_some() {
                return result;
            }
        }

        None
    }

    #[tracing::instrument(skip(self, source_fqin), fields(vm_name = source_fqin.get_sdv_vm_name()))]
    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message: &str,
    ) -> Option<MessageDescriptor> {
        let data_sources = self.data_sources.get(&DataType::VsidlSchemas)?;

        for data_source in data_sources {
            let result = match self.providers.get(data_source).as_ref() {
                Some(provider) => provider.get_message_descriptor(source_fqin, message).await,
                None => None,
            };

            if result.is_some() {
                return result;
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use protobuf::reflect::MessageDescriptor;
    use protobuf::well_known_types::empty::Empty;
    use sdv_comms::id::ServiceFqin;
    use sdv_comms::ContextRef;
    use std::collections::{HashMap, HashSet};
    use std::iter::once;
    use std::str::FromStr;

    use crate::factory::MockFactory;
    use crate::file_utils::directory_files;
    use crate::proto_schemas_data::ProtoSchemasData;
    use crate::types::DataType;
    use crate::vsidl_provider::VSIDL_CONFIG_DIR;
    use crate::vsidl_provider_api::*;

    use mockall::predicate::eq as mock_eq;
    use mockall::Predicate;

    use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;

    use googletest::assert_that;
    use googletest::prelude::eq;

    fn fqin(test_name: &str) -> ServiceFqin {
        let fqin_str =
            format!("ignored:com.google.sdv.vsidl_provider_unit_test.{test_name}.Client/ignored");
        ServiceFqin::from_str(&fqin_str).expect("Valid ServiceFqin")
    }

    fn context_eq(expected_ctx: ContextRef) -> impl Predicate<ContextRef> {
        mockall::predicate::function(move |ctx: &ContextRef| {
            ctx.get_self_fqin() == expected_ctx.get_self_fqin()
                && ctx.get_self_identity() == expected_ctx.get_self_identity()
        })
    }

    fn mock_factory(
        provider_from_files: Option<Box<dyn VsidlConfigProviderV1>>,
        provider_from_apexes: Option<Box<dyn VsidlConfigProviderV1>>,
        provider_from_agent: Option<Box<dyn VsidlConfigProviderV1>>,
        provider_from_remote_agent: Option<Box<dyn VsidlConfigProviderV1>>,
    ) -> MockFactory {
        let mut mock_factory = MockFactory::new();

        mock_factory
            .expect_provider_from_files()
            .times(0..=1)
            .return_once(|_, _| provider_from_files);

        mock_factory
            .expect_provider_from_apexes()
            .times(0..=1)
            .return_once(|_| provider_from_apexes);

        mock_factory
            .expect_provider_from_agent()
            .times(0..=1)
            .return_once(|_, _| provider_from_agent);

        mock_factory
            .expect_provider_from_remote_agent()
            .times(0..=1)
            .return_once(|_, _| provider_from_remote_agent);

        mock_factory
    }

    fn empty_provider(
        interactions: impl Fn(DataType) -> usize,
    ) -> Option<Box<dyn VsidlConfigProviderV1>> {
        let mut mock_provider = MockVsidlConfigProviderV1::new();

        mock_provider
            .expect_get_someip_mappings()
            .times(interactions(DataType::SomeIp))
            .returning(|_| None);

        mock_provider
            .expect_get_diagnostics_declaration()
            .times(interactions(DataType::Diagnostics))
            .returning(|_| None);

        mock_provider
            .expect_get_publication_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _| None);

        mock_provider
            .expect_get_rpc_method_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _, _| None);

        mock_provider
            .expect_get_message_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _| None);

        Some(Box::new(mock_provider))
    }

    fn non_empty_provider(
        interactions: impl Fn(DataType) -> usize,
    ) -> Option<Box<dyn VsidlConfigProviderV1>> {
        let mut mock_provider = MockVsidlConfigProviderV1::new();

        mock_provider
            .expect_get_someip_mappings()
            .times(interactions(DataType::SomeIp))
            .returning(|_| Some(vec![]));

        mock_provider
            .expect_get_diagnostics_declaration()
            .times(interactions(DataType::Diagnostics))
            .returning(|_| Some(DiagnosticsDeclaration::default()));

        mock_provider
            .expect_get_publication_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _| {
                Some(PublicationDescriptor {
                    message_descriptor: MessageDescriptor::for_type::<Empty>(),
                    size_metadata: SizeMetadata::default(),
                })
            });

        mock_provider
            .expect_get_rpc_method_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _, _| Some(RpcMethodDescriptor::default()));

        mock_provider
            .expect_get_message_descriptor()
            .times(interactions(DataType::VsidlSchemas))
            .returning(|_, _| Some(MessageDescriptor::for_type::<Empty>()));

        Some(Box::new(mock_provider))
    }

    #[tokio::test]
    async fn test_init_with_real_factory() {
        let ctx = ContextRef::create(fqin("test_init_with_real_factory"));

        let _for_someip = super::new_for_someip_broker_agent();
        let _for_diagnostics = super::new_for_diagnostics_agent(ctx);
        let _for_agent = super::new_for_vsidl_provider_agent();
        let _for_telemetry = super::new_for_telemetry_agent(ctx);
        let _for_integration_test = super::new_for_integration_test(ctx);
    }

    fn check_for_overwrites_in_directory(directory: &str) {
        let per_file_schemas = directory_files(directory)
            .map(|file_path| ProtoSchemasData::new(once(file_path)))
            .collect::<Vec<_>>();

        let mut file_descriptor_protos = HashMap::new();

        for schemas in per_file_schemas {
            for file_descriptor_proto in schemas.file_name_to_file_descriptor.values().cloned() {
                match file_descriptor_protos.get(&file_descriptor_proto.name) {
                    Some(existing_file_descriptor_proto) => {
                        if existing_file_descriptor_proto != &file_descriptor_proto {
                            panic!("Duplicate file descriptor proto file with different contents detected: {:?}", file_descriptor_proto.name);
                        }
                    }
                    _ => {
                        file_descriptor_protos
                            .insert(file_descriptor_proto.name.clone(), file_descriptor_proto);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_for_overwrites() {
        check_for_overwrites_in_directory(VSIDL_CONFIG_DIR);
    }

    #[tokio::test]
    #[should_panic(
        expected = "Duplicate file descriptor proto file with different contents detected: Some(\"overwritten_messages.proto\")"
    )]
    async fn test_overwrites_check() {
        check_for_overwrites_in_directory("test_data/v1/with_overwrites");
    }

    #[tokio::test]
    async fn test_init_new_for_vsidl_provider_agent() {
        let mut mock_factory = MockFactory::new();

        mock_factory
            .expect_provider_from_files()
            .with(mock_eq(HashSet::from([DataType::VsidlSchemas])), mock_eq(VSIDL_CONFIG_DIR))
            .times(1)
            .returning(|_, _| None);
        mock_factory
            .expect_provider_from_apexes()
            .with(mock_eq(HashSet::from([
                DataType::Diagnostics,
                DataType::VsidlSchemas,
                DataType::SomeIp,
            ])))
            .times(1)
            .returning(|_| None);
        mock_factory.expect_provider_from_agent().never();
        mock_factory.expect_provider_from_remote_agent().never();

        // VSIDL provider is successfully created.
        let _vsidl_provider = super::new_for_vsidl_provider_agent_with_injection(mock_factory);
    }

    #[tokio::test]
    async fn test_init_new_for_diagnostics_agent() {
        let mut mock_factory = MockFactory::new();
        let ctx = ContextRef::create(fqin("test_init_new_for_diagnostics_agent"));

        mock_factory
            .expect_provider_from_files()
            .with(mock_eq(HashSet::from([DataType::VsidlSchemas])), mock_eq(VSIDL_CONFIG_DIR))
            .times(1)
            .returning(|_, _| None);
        mock_factory
            .expect_provider_from_apexes()
            .with(mock_eq(HashSet::from([DataType::Diagnostics])))
            .times(1)
            .returning(|_| None);
        mock_factory.expect_provider_from_agent().never();
        mock_factory
            .expect_provider_from_remote_agent()
            .with(mock_eq(HashSet::from([DataType::Diagnostics])), context_eq(ctx))
            .times(1)
            .returning(|_, _| None);

        // VSIDL provider is successfully created.
        let _vsidl_provider = super::new_for_diagnostics_agent_with_injection(mock_factory, ctx);
    }

    #[tokio::test]
    async fn test_init_new_for_someip_broker_agent() {
        let mut mock_factory = MockFactory::new();

        mock_factory
            .expect_provider_from_files()
            .with(mock_eq(HashSet::from([DataType::VsidlSchemas])), mock_eq(VSIDL_CONFIG_DIR))
            .times(1)
            .returning(|_, _| None);
        mock_factory.expect_provider_from_apexes().times(1).returning(|_| None);
        mock_factory.expect_provider_from_agent().never();
        mock_factory.expect_provider_from_remote_agent().never();

        // VSIDL provider is successfully created.
        let _vsidl_provider = super::new_for_someip_broker_agent_with_injection(mock_factory);
    }

    #[tokio::test]
    async fn test_init_new_for_telemetry_agent() {
        let mut mock_factory = MockFactory::new();
        let ctx = ContextRef::create(fqin("test_init_new_for_telemetry_agent"));

        mock_factory.expect_provider_from_files().never();
        mock_factory.expect_provider_from_apexes().never();
        mock_factory
            .expect_provider_from_agent()
            .with(mock_eq(HashSet::from([DataType::VsidlSchemas])), context_eq(ctx))
            .times(1)
            .returning(|_, _| None);
        mock_factory.expect_provider_from_remote_agent().never();

        // VSIDL provider is successfully created.
        let _vsidl_provider = super::new_for_telemetry_agent_with_injection(mock_factory, ctx);
    }

    #[tokio::test]
    async fn test_init_new_for_integration_test() {
        let mut mock_factory = MockFactory::new();
        let ctx = ContextRef::create(fqin("test_init_new_for_integration_test"));

        mock_factory.expect_provider_from_files().never();
        mock_factory.expect_provider_from_apexes().never();
        mock_factory
            .expect_provider_from_agent()
            .with(
                mock_eq(HashSet::from([
                    DataType::SomeIp,
                    DataType::Diagnostics,
                    DataType::VsidlSchemas,
                ])),
                context_eq(ctx),
            )
            .times(1)
            .returning(|_, _| None);
        mock_factory.expect_provider_from_remote_agent().never();

        // VSIDL provider is successfully created.
        let _vsidl_provider = super::new_for_integration_test_with_injection(mock_factory, ctx);
    }

    #[tokio::test]
    async fn test_new_for_vsidl_provider_agent_get_data_from_files() {
        let fqin = fqin("test_new_for_vsidl_provider_agent_get_data_from_files");

        let mock_factory = mock_factory(
            // files
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 1,
            }),
            // apexes
            empty_provider(|_| 1),
            // agent
            None,
            // remote-only agent
            None,
        );

        let vsidl_provider = super::new_for_vsidl_provider_agent_with_injection(mock_factory);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&None));
        assert_that!(vsidl_provider.get_diagnostics_declaration(&fqin).await, eq(&None));
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_vsidl_provider_agent_get_data_from_apexes() {
        let fqin = fqin("test_new_for_vsidl_provider_agent_get_data_from_apexes");

        let mock_factory = mock_factory(
            // files
            non_empty_provider(|_| 0),
            // apexes
            non_empty_provider(|_| 1),
            // agent
            None,
            // remote-only agent
            None,
        );

        let vsidl_provider = super::new_for_vsidl_provider_agent_with_injection(mock_factory);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&Some(vec![])));
        assert_that!(
            vsidl_provider.get_diagnostics_declaration(&fqin).await,
            eq(&Some(DiagnosticsDeclaration::default()))
        );
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_diagnostics_agent_get_data() {
        let fqin = fqin("test_new_for_diagnostics_agent_get_data");
        let ctx = ContextRef::create(fqin.clone());

        let mock_factory = mock_factory(
            // files
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 1,
            }),
            // apexes
            non_empty_provider(|_| 0),
            // agent
            None,
            // remote-only agent
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 1,
                DataType::VsidlSchemas => 0,
            }),
        );

        let vsidl_provider = super::new_for_diagnostics_agent_with_injection(mock_factory, ctx);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&None));
        assert_that!(
            vsidl_provider.get_diagnostics_declaration(&fqin).await,
            eq(&Some(DiagnosticsDeclaration::default()))
        );
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_diagnostics_agent_get_data_from_apex() {
        let fqin = fqin("test_new_for_diagnostics_agent_get_data_from_apex");
        let ctx = ContextRef::create(fqin.clone());

        let mock_factory = mock_factory(
            // files
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 1,
            }),
            // apexes
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 1,
                DataType::VsidlSchemas => 0,
            }),
            // agent
            None,
            // remote-only agent
            empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 1,
                DataType::VsidlSchemas => 0,
            }),
        );

        let vsidl_provider = super::new_for_diagnostics_agent_with_injection(mock_factory, ctx);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&None));
        assert_that!(
            vsidl_provider.get_diagnostics_declaration(&fqin).await,
            eq(&Some(DiagnosticsDeclaration::default()))
        );
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_someip_broker_agent_get_data() {
        let fqin = fqin("test_new_for_someip_broker_agent_get_data");

        let mock_factory = mock_factory(
            // files
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 1,
            }),
            // apexes
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 1,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 0,
            }),
            // agent
            None,
            // remote-only agent
            None,
        );

        let vsidl_provider = super::new_for_someip_broker_agent_with_injection(mock_factory);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&Some(vec![])));
        assert_that!(vsidl_provider.get_diagnostics_declaration(&fqin).await, eq(&None));
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_telemetry_agent_get_data() {
        let fqin = fqin("test_new_for_telemetry_agent_get_data");
        let ctx = ContextRef::create(fqin.clone());

        let mock_factory = mock_factory(
            // files
            None,
            // apexes
            None,
            // agent
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 0,
                DataType::Diagnostics => 0,
                DataType::VsidlSchemas => 1,
            }),
            // remote-only agent
            None,
        );

        let vsidl_provider = super::new_for_telemetry_agent_with_injection(mock_factory, ctx);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&None));
        assert_that!(vsidl_provider.get_diagnostics_declaration(&fqin).await, eq(&None));
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }

    #[tokio::test]
    async fn test_new_for_integration_test_get_data() {
        let fqin = fqin("test_new_for_integration_test_get_data");
        let ctx = ContextRef::create(fqin.clone());

        let mock_factory = mock_factory(
            // files
            None,
            // apexes
            None,
            // agent
            non_empty_provider(|data_type| match data_type {
                DataType::SomeIp => 1,
                DataType::Diagnostics => 1,
                DataType::VsidlSchemas => 1,
            }),
            // remote-only agent
            None,
        );

        let vsidl_provider = super::new_for_integration_test_with_injection(mock_factory, ctx);

        assert_that!(vsidl_provider.get_someip_mappings(&fqin).await, eq(&Some(vec![])));
        assert_that!(
            vsidl_provider.get_diagnostics_declaration(&fqin).await,
            eq(&Some(DiagnosticsDeclaration::default()))
        );
        assert_that!(
            vsidl_provider.get_publication_descriptor(&fqin, &Default::default()).await,
            eq(&Some(PublicationDescriptor {
                message_descriptor: MessageDescriptor::for_type::<Empty>(),
                size_metadata: SizeMetadata::default()
            }))
        );
        assert_that!(
            vsidl_provider
                .get_rpc_method_descriptor(&fqin, &Default::default(), "test_method")
                .await,
            eq(&Some(RpcMethodDescriptor::default()))
        );
        assert_that!(
            vsidl_provider.get_message_descriptor(&fqin, "test_message").await,
            eq(&Some(MessageDescriptor::for_type::<Empty>()))
        );
    }
}
