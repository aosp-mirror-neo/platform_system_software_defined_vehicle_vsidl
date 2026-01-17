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

use crate::{AndroidBpFileGroups, Cli};
use handlebars::Handlebars;

#[derive(serde_derive::Serialize, Default)]
pub(crate) struct MainAndroidBpContext {
    pub(crate) year: i32,
    pub(crate) module_name: String,
    pub(crate) dependencies: Vec<String>,
    pub(crate) dependency_paths_str: String,
    pub(crate) vsidlc_args: String,
}

fn list_of_vsidlc_arguments_to_forward(cli: &Cli) -> Vec<String> {
    // The following section defines the list of vsidlc argument to NOT forward to the genrule execution.
    // --rust-formatter [constant] - Since RustFmt has problem to be executed inside genrule and on CI.
    //                               This flag is always set to pretty-please directly in the generate_zip_catalog.sh.
    // --purge [do not fwd] - This will clean up a folder in a Soong sandbox, it's not strictly required.
    // --explain [do not fwd] - N/A this does not generate anything.
    // --genrule [do not fwd] - This will avoid having rust code even in the genrule sandbox.
    // --apex [do not fwd] - This will generate others Android.bp and configuration which will
    //                       remain in the output Soong folder and never be referenced.
    // --services [do not fwd] - This will generate other Android.bp and configuration which will
    //                           remain in the output soong folder and never be referenced
    // --catalog_path [do not fwd]
    // --dependency_catalog_path [do not fwd]: hardcoding paths in genrule does not make sense.
    //                             genrule claims to be flexible using filegroups, hardcoding path will
    //                             make impossible for developer to change folder structure of dependencies catalog
    // --output_path [do not fwd]: This needs to be set to the genrule output in order to expose the result to
    //                             others Android.bp targets
    // other arguments are manually forwarded
    let mut vsidlc_args = Vec::new();

    // --no-pest [forward] - Useful for testing.
    if cli.no_pest {
        vsidlc_args.push("--no-pest".to_string());
    }

    // --no-warn [forward] - Required to speed up vsidlc in genrule avoiding printing all warning messages.
    if cli.no_warn {
        vsidlc_args.push("--no-warn".to_string());
    }

    // --ignore [forward] - Required to handle for catalog exceptions inside genrule.
    for i in &cli.ignore {
        vsidlc_args.push("--ignore".to_string());
        vsidlc_args.push(i.clone());
    }

    // --target-api [forward] - This is required to avoid breaking user code.
    vsidlc_args.push("--target-api".to_string());
    vsidlc_args.push(cli.target_api.clone());

    vsidlc_args
}

pub(super) fn generate_main_android_bp(
    catalog_filegroup: String,
    filegroup_name: AndroidBpFileGroups,
    cli: &Cli,
    handlebars: &Handlebars,
) -> anyhow::Result<String> {
    let mut dependencies = Vec::new();
    for filegroup in filegroup_name.catalog {
        if filegroup == catalog_filegroup {
            continue;
        }

        dependencies.push(filegroup.to_string())
    }

    for filegroup in filegroup_name.dependencies {
        dependencies.push(filegroup.to_string())
    }

    let dependency_paths_str =
        dependencies.iter().map(|d| format!("$(locations :{})", d)).collect::<Vec<_>>().join(" ");

    let context = MainAndroidBpContext {
        year: crate::rust::generator::get_current_year(),
        module_name: catalog_filegroup,
        dependencies,
        dependency_paths_str,
        vsidlc_args: list_of_vsidlc_arguments_to_forward(cli).join(" "),
    };

    Ok(handlebars.render("main_android_bp", &context)?)
}
