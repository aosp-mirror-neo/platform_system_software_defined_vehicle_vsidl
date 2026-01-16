// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use anyhow::Result;
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use heck::ToSnakeCase;

pub struct InitializeHandlebars;

macro_rules! define_common_templates {
    ($name:ident, $prefix:expr) => {
        #[allow(dead_code)]
        const $name: &[(&str, &str)] = &[
            /* General Templates */
            ("copyright", include_str!(concat!($prefix, "/copyright.hbs"))),
            ("do_not_edit_warning", include_str!(concat!($prefix, "/do_not_edit_warning.hbs"))),

            /* VSIDL Templates */
            ("main_android_bp", include_str!(concat!($prefix, "/main_android_bp.hbs"))),
            ("service_bundle", include_str!(concat!($prefix, "/generated_rs/service_bundles/service_bundle.hbs"))),
            ("android_bp", include_str!(concat!($prefix, "/generated_rs/service_bundles/android_bp.hbs"))),
            ("rpc_service", include_str!(concat!($prefix, "/generated_rs/rpc/rpc_service.hbs"))),
            ("rpc/android_bp", include_str!(concat!($prefix, "/generated_rs/rpc/android_bp.hbs"))),

            /* RPC Service Partials */
            ("rpc/internal/mod", include_str!(concat!($prefix, "/generated_rs/rpc/internal/mod.hbs"))),
            (
                "rpc/internal/functions/get_method_map",
                include_str!(concat!($prefix, "/generated_rs/rpc/internal/functions/get_method_map.hbs")),
            ),
            (
                "rpc/internal/functions/get_rpc_method_map",
                include_str!(concat!($prefix, "/generated_rs/rpc/internal/functions/get_rpc_method_map.hbs")),
            ),
            (
                "rpc/interface/functions/method",
                include_str!(concat!($prefix, "/generated_rs/rpc/interface/functions/method.hbs")),
            ),
            (
                "rpc/interface/functions/init_server_options",
                include_str!(concat!($prefix, "/generated_rs/rpc/interface/functions/init_server_options.hbs")),
            ),
            (
                "rpc/client/functions/method",
                include_str!(concat!($prefix, "/generated_rs/rpc/client/functions/method.hbs")),
            ),
            (
                "rpc/impl_rpc_client/functions/new",
                include_str!(concat!($prefix, "/generated_rs/rpc/impl_rpc_client/functions/new.hbs")),
            ),

            /* Service Bundle Functions */
            (
                "service_bundles/functions/new",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/new.hbs")),
            ),
            (
                "service_bundles/functions/create_rpc_client",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_rpc_client.hbs")),
            ),
            (
                "service_bundles/functions/create_server_registration_event_stream",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_server_registration_event_stream.hbs")),
            ),
            (
                "service_bundles/functions/create_subscriber",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_subscriber.hbs")),
            ),
            (
                "service_bundles/functions/create_observer",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_observer.hbs")),
            ),
            (
                "service_bundles/functions/create_history",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_history.hbs")),
            ),
            (
                "service_bundles/functions/create_instant_reader",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_instant_reader.hbs")),
            ),
            (
                "service_bundles/functions/find_publication",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/find_publication.hbs")),
            ),
            (
                "service_bundles/functions/create_registration_event_stream",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/create_registration_event_stream.hbs")),
            ),
            (
                "service_bundles/functions/take_publisher",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/take_publisher.hbs")),
            ),
            (
                "service_bundles/functions/fill_publication_channels",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/functions/fill_publication_channels.hbs")),
            ),

            /* Service Bundle Modules */
            (
                "service_bundles/publisher/publisher",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/publisher/publisher.hbs")),
            ),
            (
                "service_bundles/subscriber/subscriber",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/subscriber/subscriber.hbs")),
            ),
            ("service_bundles/struct", include_str!(concat!($prefix, "/generated_rs/service_bundles/struct.hbs"))),

            /* Publisher Module Impls */
            (
                "service_bundles/publisher/impl_metadata/impl",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/publisher/impl_metadata/impl.hbs")),
            ),
            (
                "service_bundles/publisher/impl_channel/impl",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/publisher/impl_channel/impl.hbs")),
            ),

            /* Subscriber Module Impls */
            (
                "service_bundles/subscriber/impl_metadata/impl",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/subscriber/impl_metadata/impl.hbs")),
            ),
            (
                "service_bundles/subscriber/impl_channel/impl",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/subscriber/impl_channel/impl.hbs")),
            ),

            /* Publisher Module Impl Functions */
            (
                "generated_rs/service_bundles/publisher/impl_metadata/functions/get_message_metadata",
                include_str!(concat!($prefix, "/generated_rs/service_bundles/publisher/impl_metadata/functions/get_message_metadata.hbs")),
            ),

            /* Diagnostics */
            ("diagnostics", include_str!("../diagnostics/templates/diagnostics.hbs")),
            ("data_item_server", include_str!("../diagnostics/templates/server/data_item.hbs")),
            (
                "routine_control_server",
                include_str!("../diagnostics/templates/server/routine_control.hbs"),
            ),
            ("io_control_server", include_str!("../diagnostics/templates/server/io_control.hbs")),
            (
                "fault_listener_server",
                include_str!("../diagnostics/templates/server/fault_listener.hbs"),
            ),
            (
                "routine_handler_types_list",
                include_str!("../diagnostics/templates/routine_control/types_list.hbs"),
            ),

            (
                "apex_android_bp",
                include_str!(concat!($prefix, "/apex/Android.bp.hbs")),
            ),
            (
                "apex_manifest_json",
                include_str!(concat!($prefix, "/apex/apex_manifest.json.hbs")),
            ),
            (
                "file_contexts",
                include_str!(concat!($prefix, "/apex/file_contexts.hbs")),
            ),
            (
                "linker_config_json",
                include_str!(concat!($prefix, "/apex/linker.config.json.hbs")),
            ),
            (
                "sdv_service_bundles_manifest",
                include_str!(concat!($prefix, "/apex/sdv_service_bundles_manifest.textproto.hbs")),
            ),
            (
                "acls_android_bp",
                include_str!(concat!($prefix, "/configs/acls/Android.bp.hbs")),
            ),
            (
                "service_bundles_acls",
                include_str!(concat!($prefix, "/configs/acls/service_bundles_acls.textproto.hbs")),
            ),
            (
                "orchestration_android_bp",
                include_str!(concat!($prefix, "/configs/orchestration/Android.bp.hbs")),
            ),
            (
                "service_bundle_orchestration_config",
                include_str!(concat!($prefix, "/configs/orchestration/service_bundle_config.textproto.hbs")),
            ),
            (
                "vm_orchestration_config",
                include_str!(concat!($prefix, "/configs/orchestration/vm_orchestration_config.textproto.hbs")),
            ),

            /* Services Templates */
            (
                "services_android_bp",
                include_str!(concat!($prefix, "/services/Android.bp.hbs")),
            ),
            (
                "services_main_rs",
                include_str!(concat!($prefix, "/services/main.rs.hbs")),
            ),
        ];
    };
}

// Instantiate for each version to catch errors if common templates go missing
define_common_templates!(COMMON_TEMPLATES_LATEST, "../templates/latest");
define_common_templates!(COMMON_TEMPLATES_PDK29, "../templates/pdk29");

const LATEST_ONLY_TEMPLATES: &[(&str, &str)] = &[
    /* Publisher Module Impls */
    (
        "service_bundles/publisher/impl_identity/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_identity/impl.hbs"),
    ),
    (
        "service_bundles/publisher/impl_metadata/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_metadata/impl.hbs"),
    ),
    (
        "service_bundles/publisher/impl_to_type_name/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_to_type_name/impl.hbs"),
    ),

    /* Subscriber Module Impls */
    (
        "service_bundles/subscriber/impl_identity/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_identity/impl.hbs"),
    ),
    (
        "service_bundles/subscriber/impl_metadata/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_metadata/impl.hbs"),
    ),
    (
        "service_bundles/subscriber/impl_to_type_name/impl",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_to_type_name/impl.hbs"),
    ),

    /* Publisher Module Impl Functions */
    (
        "generated_rs/service_bundles/publisher/impl_identity/functions/get_id",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_identity/functions/get_id.hbs"),
    ),
    (
        "generated_rs/service_bundles/publisher/impl_identity/functions/get_service_unit_name",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_identity/functions/get_service_unit_name.hbs"),
    ),
    (
        "generated_rs/service_bundles/publisher/impl_metadata/functions/get_message_metadata",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_metadata/functions/get_message_metadata.hbs"),
    ),
    (
        "generated_rs/service_bundles/publisher/impl_to_type_name/functions/type_name",
        include_str!("../templates/latest/generated_rs/service_bundles/publisher/impl_to_type_name/functions/type_name.hbs"),
    ),

    /* Subscriber Module Impl Functions */
    (
        "generated_rs/service_bundles/subscriber/impl_identity/functions/get_id",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_identity/functions/get_id.hbs"),
    ),
    (
        "generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_unit_name",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_unit_name.hbs"),
    ),
    (
        "generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_bundle_package",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_bundle_package.hbs"),
    ),
    (
        "generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_bundle_name",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_identity/functions/get_service_bundle_name.hbs"),
    ),
    (
        "generated_rs/service_bundles/subscriber/impl_to_type_name/functions/type_name",
        include_str!("../templates/latest/generated_rs/service_bundles/subscriber/impl_to_type_name/functions/type_name.hbs"),
    ),

    /* Service Bundle Builder Functions */
    (
        "service_bundles/functions/service_bundle_builder",
        include_str!("../templates/latest/generated_rs/service_bundles/functions/service_bundle_builder.hbs")
    ),
    (
        "service_bundles/service_bundle_builder/builder",
        include_str!("../templates/latest/generated_rs/service_bundles/service_bundle_builder/builder.hbs"),
    ),
    (
        "service_bundles/service_bundle_builder/functions/set_comms",
        include_str!("../templates/latest/generated_rs/service_bundles/service_bundle_builder/functions/set_comms.hbs"),
    ),
    (
        "service_bundles/service_bundle_builder/functions/set_server",
        include_str!("../templates/latest/generated_rs/service_bundles/service_bundle_builder/functions/set_server.hbs")
    ),
    (
        "service_bundles/service_bundle_builder/functions/build",
        include_str!("../templates/latest/generated_rs/service_bundles/service_bundle_builder/functions/build.hbs")
    ),

    /* RPC Service Functions */
    (
        "rpc/impl_unit_type/unit_type",
        include_str!("../templates/latest/generated_rs/rpc/impl_unit_type/unit_type.hbs")
    ),
];

const PDK29_ONLY_TEMPLATES: &[(&str, &str)] = &[
    (
        "service_bundles/unit_type/unit_type",
        include_str!("../templates/pdk29/generated_rs/service_bundles/unit_type/unit_type.hbs"),
    ),
    (
        "service_bundles/functions/add_server",
        include_str!("../templates/pdk29/generated_rs/service_bundles/functions/add_server.hbs"),
    ),
];

fn register_templates(handlebars: &mut Handlebars, target_api: &str) -> Result<()> {
    let common_templates = match target_api {
        "latest" => COMMON_TEMPLATES_LATEST,
        "pdk29" => COMMON_TEMPLATES_PDK29,
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported target_api: {}. Supported values: 'latest', 'pdk29'.",
                target_api
            ))
        }
    };

    for (name, template) in common_templates {
        handlebars.register_template_string(name, template)?;
    }

    match target_api {
        "latest" => {
            for (name, template) in LATEST_ONLY_TEMPLATES {
                handlebars.register_template_string(name, template)?;
            }
        }
        "pdk29" => {
            for (name, template) in PDK29_ONLY_TEMPLATES {
                handlebars.register_template_string(name, template)?;
            }
        }
        _ => {} // Already handled
    }

    Ok(())
}

impl GeneratorStepExecution for InitializeHandlebars {
    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let mut handlebars = Handlebars::new();
        // don't escape HTML characters
        handlebars.register_escape_fn(handlebars::no_escape);
        // throw RenderError when trying to access non-existing fields
        handlebars.set_strict_mode(true);
        // Helpers
        handlebars.register_helper("snake_case", Box::new(snake_case_helper));

        register_templates(&mut handlebars, &state.cli.target_api.to_lowercase())?;

        state.handlebars = Some(handlebars);
        Ok(())
    }
}

// A helper to convert a string to snake_case
fn snake_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    // Get the parameter from the template
    let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
    // Write the snake_cased string to the output
    out.write(&param.to_snake_case())?;
    Ok(())
}
