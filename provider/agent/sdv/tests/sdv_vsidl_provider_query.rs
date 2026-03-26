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

//! Sample Cross VM VSIDL Provider Call
// TODO update readme with rest of commands
// TODO update python side

use clap::Parser;
use clap::ValueEnum;
use futures::StreamExt;
use log::info;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use sdv_comms::ContextRef;
use sdv_config_provider::vsidl_provider_api::AvailabilityChangeEvent;
use sdv_config_provider::{vsidl_provider, vsidl_provider_api::VsidlConfigProvider};
use sdv_dump::DumpBinderConnection;
use sdv_dump::DumpStateReporter;
use std::future;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::{sleep, timeout};

static DEFAULT_CLIENT_FQIN_STR: &str =
    "ignored:com.android.sdv.sample.vsidl_provider.Client/default";

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum QueryVariant {
    DiagnosticsDeclaration,
    PublicationDescriptor,
    RpcMethodDescriptor,
    MessageDescriptor,
    SubscribeAvailabilityChange,
}

#[derive(Parser, Debug)]
#[command(long_about = None)]
struct Args {
    #[arg(long)]
    server_vm_name: String,

    #[arg(long)]
    service_bundle_package_name: Option<String>,

    #[arg(long)]
    service_bundle_name: Option<String>,

    #[arg(value_enum, long)]
    query_variant: QueryVariant,

    #[arg(long)]
    client_fqin: Option<String>,

    #[arg(long)]
    message_name: Option<String>,

    // By making the unit_type_* args require each other, we create an
    // "all or none" group.
    /// Unit type package and name are required when querying for a
    /// publication descriptor or for an RPC method descriptor
    #[arg(long, requires("unit_type_name"))]
    unit_type_package_name: Option<String>,

    /// Unit type package, bundle and name are required when querying for a
    /// publication descriptor or for an RPC method descriptor
    #[arg(long, requires("unit_type_package_name"))]
    unit_type_name: Option<String>,

    #[arg(long)]
    method_name: Option<String>,

    #[arg(long)]
    loop_period: Option<u64>,

    #[arg(long)]
    num_loops: Option<u64>,

    #[arg(long, default_value_t = 5)]
    availability_timeout_s: u64,
}

// --- Helper function to build server_fqin when needed ---
fn build_server_fqin(args: &Args) -> anyhow::Result<ServiceFqin> {
    let package_name = args.service_bundle_package_name.clone().ok_or_else(|| {
        anyhow::anyhow!("--service-bundle-package-name is required for this query")
    })?;

    let bundle_name = args
        .service_bundle_name
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--service-bundle-name is required for this query"))?;

    ServiceFqin::builder()
        .sdv_vm_name(args.server_vm_name.clone())
        .sdv_package_name(package_name)
        .service_bundle_name(bundle_name)
        .service_instance_name("ignored")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build ServiceFqin: {:?}", e))
}

async fn query_diagnostics_declaration(
    ctx: ContextRef,
    provider: &Arc<impl VsidlConfigProvider>,
    server_fqin: &ServiceFqin,
) -> Option<String> {
    provider
        .get_diagnostics_declaration(server_fqin)
        .await
        .map(|diagnostics_declaration| {
            format!(
                "Querying VSIDL Provider Diagnostics from VM {} to VM {}. Received response: {:?}",
                ctx.get_self_fqin().get_sdv_vm_name(),
                server_fqin.get_sdv_vm_name(),
                diagnostics_declaration
            )
        })
        .ok()
}

async fn query_message_descriptor(
    args: &Args,
    ctx: ContextRef,
    provider: &Arc<impl VsidlConfigProvider>,
    server_fqin: &ServiceFqin,
) -> Option<String> {
    let Some(message_name) = args.message_name.as_ref() else {
        info!("A message name is required when querying for a message descriptor");
        return None;
    };

    provider.get_message_descriptor(server_fqin, message_name).await.map(|diagnostics_declaration|{
        format!(
            "Querying VSIDL Provider Message Descriptor from VM {} to VM {}. Received response: {:?}",
            ctx.get_self_fqin().get_sdv_vm_name(),
            server_fqin.get_sdv_vm_name(),
            diagnostics_declaration
        )
    }).ok()
}

fn get_unit_type(args: &Args) -> Option<UnitType> {
    match (args.unit_type_package_name.as_ref(), args.unit_type_name.as_ref()) {
        (Some(unit_type_package_name), Some(unit_type_name)) => Some(
            UnitType::builder()
                .set_sdv_package_name(unit_type_package_name.clone())
                .set_unit_type_name(unit_type_name.clone())
                .build()
                .ok()?,
        ),
        (_, _) => None,
    }
}

async fn query_publication_descriptor(
    args: &Args,
    ctx: ContextRef,
    provider: &Arc<impl VsidlConfigProvider>,
    server_fqin: &ServiceFqin,
) -> Option<String> {
    let Some(unit_type) = get_unit_type(args) else {
        info!("Unit type package, bundle and name are required when querying for a publication descriptor");
        return None;
    };

    provider.get_publication_descriptor(server_fqin, &unit_type).await.map(|diagnostics_declaration|{
        format!(
            "Querying VSIDL Provider Publication Descriptor from VM {} to VM {}. Received response: {:?}",
            ctx.get_self_fqin().get_sdv_vm_name(),
            server_fqin.get_sdv_vm_name(),
            diagnostics_declaration
        )
    }).ok()
}

async fn query_rpc_method_descriptor(
    args: &Args,
    ctx: ContextRef,
    provider: &Arc<impl VsidlConfigProvider>,
    server_fqin: &ServiceFqin,
) -> Option<String> {
    let (Some(unit_type), Some(method_name)) = (get_unit_type(args), args.method_name.as_ref())
    else {
        info!("Unit type and method name are required when querying for an RPC method descriptor");
        return None;
    };

    provider.get_rpc_method_descriptor(server_fqin, &unit_type, method_name).await.map(|diagnostics_declaration|{
        format!(
            "Querying VSIDL Provider RPC Method Descriptor from VM {} to VM {}. Received response: {:?}",
            ctx.get_self_fqin().get_sdv_vm_name(),
            server_fqin.get_sdv_vm_name(),
            diagnostics_declaration
        )
    }).ok()
}

async fn query_subscribe_availability_change(
    args: &Args,
    ctx: ContextRef,
    provider: &Arc<impl VsidlConfigProvider>,
) -> Option<String> {
    let vm_name = &args.server_vm_name;
    let timeout_duration = Duration::from_secs(args.availability_timeout_s);

    let test_future = async {
        let mut availability_stream =
            provider.subscribe_availability_change_by_vm(vm_name).await.ok()?;
        match availability_stream.next().await {
            Some(AvailabilityChangeEvent::Available) => Some("Server is reachable".to_string()),
            _ => Some("Server is not reachable".to_string()),
        }
    };

    match timeout(timeout_duration, test_future).await {
        Ok(Some(result)) => Some(format!(
            "Querying VSIDL Provider availability on VM {} from VM {}. Received response: {}",
            vm_name,
            ctx.get_self_fqin().get_sdv_vm_name(),
            result
        )),
        _ => Some(format!("Timed out waiting for availability from VM {}", vm_name)),
    }
}

struct QueryTester {
    latest_query_result: Option<String>,
}

impl DumpStateReporter for QueryTester {
    fn dump(
        &self,
        writer: &mut dyn std::io::Write,
        _: &[&std::ffi::CStr],
    ) -> std::result::Result<(), std::io::Error> {
        if let Some(latest_query_result) = self.latest_query_result.as_ref() {
            writeln!(writer, "My state: {latest_query_result}")
        } else {
            writeln!(writer, "Latest query failed")
        }
    }
}

impl QueryTester {
    async fn query_diagnostics_declaration(
        ctx: ContextRef,
        provider: &Arc<impl VsidlConfigProvider>,
        server_fqin: &ServiceFqin,
    ) -> Self {
        Self {
            latest_query_result: query_diagnostics_declaration(ctx, provider, server_fqin).await,
        }
    }

    async fn query_message_descriptor(
        args: &Args,
        ctx: ContextRef,
        provider: &Arc<impl VsidlConfigProvider>,
        server_fqin: &ServiceFqin,
    ) -> Self {
        Self {
            latest_query_result: query_message_descriptor(args, ctx, provider, server_fqin).await,
        }
    }

    async fn query_publication_descriptor(
        args: &Args,
        ctx: ContextRef,
        provider: &Arc<impl VsidlConfigProvider>,
        server_fqin: &ServiceFqin,
    ) -> Self {
        Self {
            latest_query_result: query_publication_descriptor(args, ctx, provider, server_fqin)
                .await,
        }
    }

    async fn query_rpc_method_descriptor(
        args: &Args,
        ctx: ContextRef,
        provider: &Arc<impl VsidlConfigProvider>,
        server_fqin: &ServiceFqin,
    ) -> Self {
        Self {
            latest_query_result: query_rpc_method_descriptor(args, ctx, provider, server_fqin)
                .await,
        }
    }

    async fn query_subscribe_availability_change(
        args: &Args,
        ctx: ContextRef,
        provider: &Arc<impl VsidlConfigProvider>,
    ) -> Self {
        Self { latest_query_result: query_subscribe_availability_change(args, ctx, provider).await }
    }
}

static BINDER_TAG: &str = "vsidl_provider_test";
static DUMP_REPORT_TITLE: &str = "SDV Agent dump - VSIDL Provider Test";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sdv_log::init_logger("sdv_vsidl_provider_query")
        .map_err(|e| anyhow::anyhow!("sdv_log::init_logger - Error {e:?}"))?;
    let dm = DumpBinderConnection::initialize(DUMP_REPORT_TITLE, BINDER_TAG);
    sdv_dump::ProcessState::start_thread_pool();

    let args: Args = Args::parse();

    // This will be obtained from Service Lifecycle functions.
    let client_fqin =
        ServiceFqin::from_str(args.client_fqin.as_deref().unwrap_or(DEFAULT_CLIENT_FQIN_STR))
            .expect("Valid ServiceFqin");
    let ctx: ContextRef = ContextRef::create(client_fqin);

    dm.set_self_fqin(ctx.get_self_fqin().to_string());

    // Using new_for_integration_test flavor of VSIDL Provider, since we want to test the calls to the agent in remote VMs
    // The new_for_integration_test only creates a remote RemoteProviderFromAgent
    let provider = Arc::new(vsidl_provider::new_for_integration_test(ctx));
    dm.add_reporter("VSIDL Provider", &provider);

    let mut dumpsys_reporters = Vec::new();
    for _ in 0..args.num_loops.unwrap_or(1) {
        let query_tester = Arc::new(match args.query_variant {
            QueryVariant::DiagnosticsDeclaration => {
                let server_fqin = build_server_fqin(&args)?;
                QueryTester::query_diagnostics_declaration(ctx, &provider, &server_fqin).await
            }
            QueryVariant::PublicationDescriptor => {
                let server_fqin = build_server_fqin(&args)?;
                QueryTester::query_publication_descriptor(&args, ctx, &provider, &server_fqin).await
            }
            QueryVariant::RpcMethodDescriptor => {
                let server_fqin = build_server_fqin(&args)?;
                QueryTester::query_rpc_method_descriptor(&args, ctx, &provider, &server_fqin).await
            }
            QueryVariant::MessageDescriptor => {
                let server_fqin = build_server_fqin(&args)?;
                QueryTester::query_message_descriptor(&args, ctx, &provider, &server_fqin).await
            }
            QueryVariant::SubscribeAvailabilityChange => {
                // This query variant does not require service_bundle_package_name or service_bundle_name
                QueryTester::query_subscribe_availability_change(&args, ctx, &provider).await
            }
        });
        dumpsys_reporters.push(query_tester.clone());
        dm.add_reporter("VSIDL Provider Query Tester", &query_tester);

        if let Some(loop_period) = args.loop_period {
            sleep(Duration::from_millis(loop_period)).await;
        } else {
            continue;
        }
    }

    future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::assert_that;
    use googletest::prelude::{eq, some};
    use sdv_comms::id::ServiceFqin;
    use sdv_comms::ContextRef;
    use sdv_config_provider::vsidl_provider::new_for_integration_test;
    use sdv_config_provider::vsidl_provider_api::AvailabilityChangeEvent;
    use sdv_config_provider::vsidl_provider_api::VsidlConfigProvider;
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::time::timeout;

    use futures::StreamExt;
    const TEST_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn test_rpc_server_on_local_agent_is_reachable() {
        let test_future = async {
            // setup Binder thread pool to enable use of Comms "listener" API.
            sdv_dump::ProcessState::start_thread_pool();
            let ctx = ContextRef::create(
                ServiceFqin::from_str(
                    "ignored:com.android.sdv.sample.test_rpc_server_on_local_agent_is_reachable.Client/ignored",
                )
                .expect("Valid ServiceFqin"),
            );

            let provider = new_for_integration_test(ctx);
            let own_vm = ctx.get_self_fqin().get_sdv_vm_name().to_string();

            let mut availability_stream =
                provider.subscribe_availability_change_by_vm(&own_vm).await.unwrap();

            assert_that!(
                availability_stream.next().await,
                some(eq(AvailabilityChangeEvent::Available))
            );
        };

        timeout(TEST_TIMEOUT_DURATION, test_future).await.expect("Test timed out - most likely because the RPC server on the VSIDL Provider Agent is not reachable or does not exist (or the agent is not running or does not exist).");
    }
}
