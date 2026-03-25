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
//! Define VSIDLC command line parameters

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use formatter::Format;
use vsidlc_cli::VsidlcCommonCli;

/// Enum to control Android.bp formatting
#[derive(ValueEnum, Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum AndroidBpFormat {
    /// Format with bpfmt.
    Bpfmt,
    /// Do not format.
    None,
}

/// Enum to control .textproto formatting
#[derive(ValueEnum, Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum TextprotoFormat {
    /// Format with clang-format.
    ClangFormat,
    /// Do not format.
    None,
}

///Client args for the VSIDL parser.
#[derive(Parser, Clone)]
pub struct Cli {
    /// Path to catalog directory, i.e., the directory that contains .vsidl files
    #[arg(short, long, required_unless_present_any = ["explain", "lsp"])]
    pub(crate) catalog_path: Option<PathBuf>,

    /// Path to parent directory of `generated_rs` folder in which generated files are placed.
    #[arg(short, long, required_unless_present_any = ["explain", "lsp"])]
    pub(crate) output_path: Option<PathBuf>,

    /// Remove the `output_path/generated_rs` directory before generating new files.
    #[arg(short, long)]
    pub(crate) purge: bool,

    /// Ignore context condition findings with the given IDs
    #[arg(short, long)]
    pub(crate) ignore: Vec<String>,

    /// Use the Protobuf parser instead of the Pest parser to parse `.vsidl` files
    #[arg(long)]
    pub no_pest: bool,

    /// Optional. Specifies the directories of the dependent protos outside of the `catalog_path`. Each directory must contain an `Android.bp` file with a `rust_protobuf` build target to generate Rust API for the dependent protos. In the case of multi-layered dependency graph, please pass in one list with all the directories in the graph.
    #[arg(short, long, num_args(1..), value_delimiter = ',')]
    pub(crate) dependency_catalog_path: Vec<PathBuf>,

    /// Optional. Defines what tool is used to format rust code. By default will use rustfmt.
    /// - rust-fmt relies on either ANDROID_BUILD_TOP to be set or both RUSTFMT_PATH (executable) and RUSTFMT_TOML_PATH (format file)
    /// - pretty-please does not requires any additional setting to run
    /// - none does not requires any additional settings and will not apply any formatting to the output
    #[arg(long, value_enum, default_value_t = Format::RustFmt)]
    pub(crate) rust_formatter: Format,

    /// Optional. Defines what tool is used to format generated Android.bp files.
    #[arg(long, value_enum, default_value_t = AndroidBpFormat::Bpfmt)]
    pub(crate) android_bp_formatter: AndroidBpFormat,

    /// Optional. Defines what tool is used to format generated .textproto files.
    #[arg(long, value_enum, default_value_t = TextprotoFormat::ClangFormat)]
    pub(crate) textproto_formatter: TextprotoFormat,

    /// Explain a given error code
    #[arg(long, help = "Explain a given error code")]
    pub(crate) explain: Option<String>,

    /// Optional. When set, only Android.bp files, are generated
    /// including additional genrule. No source file is generated.
    #[arg(long)]
    pub(crate) genrule: bool,

    /// Optional. When set, config files are generated (e.g. ACL policies).
    #[arg(long)]
    pub(crate) apex: bool,

    /// Optional. When set, Rust services code is generated for each service bundle.
    #[arg(long)]
    pub(crate) services: bool,

    /// Optional. Specifies the target API level for code generation.
    /// Examples: "latest", "pdk29".
    #[arg(long, default_value = "latest")]
    pub(crate) target_api: String,

    /// Suppress all compiler warnings. Errors stay enabled.
    #[arg(short = 'w', long)]
    pub no_warn: bool,

    /// Optional. Starts VSIDLC as a Language Server Protocol (LSP) server. Overrides --no-pest to false.
    #[arg(long)]
    pub(crate) lsp: bool,
}

impl From<Cli> for VsidlcCommonCli {
    fn from(cli: Cli) -> Self {
        let mut common_cli = VsidlcCommonCli {
            catalog_path: cli.catalog_path.unwrap_or_default(),
            output_path: cli.output_path.unwrap_or_default(),
            purge: cli.purge,
            ignore: cli.ignore,
            dependency_catalog_path: cli.dependency_catalog_path,
            no_pest: cli.no_pest,
            no_warn: cli.no_warn,
            genrule: cli.genrule,
            target_api: cli.target_api,
        };
        if cli.lsp {
            common_cli.no_pest = false;
        }
        common_cli
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn test_execute_default_target_api() {
        // GIVEN a list of vsidlc command line argument without --target-api
        let args = vec!["fakeCommand", "--catalog-path", "catalog", "--output-path", "output"];

        // WHEN Cli is parsing those arguments
        let cli = Cli::parse_from(args);

        // THEN Cli is correctly initialized with default target-api
        assert_eq!(cli.target_api, "latest");
    }

    #[test]
    fn test_execute() {
        // GIVEN a list of vsidlc command line argument
        let args = vec![
            "fakeCommand",
            "--catalog-path",
            "catalog",
            "--output-path",
            "output",
            "--purge",
            "--ignore",
            "E100",
            "--ignore",
            "E101",
            "--dependency-catalog-path",
            "dep1,dep2",
            "--rust-formatter",
            "pretty-please",
            "--android-bp-formatter",
            "none",
            "--textproto-formatter",
            "none",
            "--genrule",
            "--apex",
            "--services",
            "--target-api",
            "pdk29",
            "--no-warn",
            "--no-pest",
            "--lsp",
        ];

        // WHEN Cli is parsing those arguments
        let cli = Cli::parse_from(args);

        // THEN Cli is correctly initialized
        assert_eq!(cli.catalog_path, Some(PathBuf::from("catalog")));
        assert_eq!(cli.output_path, Some(PathBuf::from("output")));
        assert!(cli.purge);
        assert_eq!(cli.ignore, vec!["E100".to_string(), "E101".to_string()]);
        assert_eq!(cli.dependency_catalog_path, vec![PathBuf::from("dep1"), PathBuf::from("dep2")]);
        assert_eq!(cli.rust_formatter, Format::PrettyPlease);
        assert_eq!(cli.android_bp_formatter, AndroidBpFormat::None);
        assert_eq!(cli.textproto_formatter, TextprotoFormat::None);
        assert!(cli.genrule);
        assert!(cli.apex);
        assert!(cli.services);
        assert_eq!(cli.target_api, "pdk29");
        assert!(cli.no_warn);
        assert!(cli.no_pest);
        assert!(cli.lsp);

        // WHEN deriving VsidlcCommonCli from Cli
        let common_cli = VsidlcCommonCli::from(cli);

        // THEN VsidlcCommonCli is correctly initialized
        assert_eq!(common_cli.catalog_path, PathBuf::from("catalog"));
        assert_eq!(common_cli.output_path, PathBuf::from("output"));
        assert!(common_cli.purge);
        assert_eq!(common_cli.ignore, vec!["E100".to_string(), "E101".to_string()]);
        assert_eq!(
            common_cli.dependency_catalog_path,
            vec![PathBuf::from("dep1"), PathBuf::from("dep2")]
        );
    }
}
