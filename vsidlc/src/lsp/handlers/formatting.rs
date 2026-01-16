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

use crate::lsp::lsp_file_provider::LspFileProvider;
use anyhow::{anyhow, Context};
use lsp_types::Url;
use lsp_types::{DocumentFormattingParams, TextEdit};
use pest::Parser;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::formatter;
use vsidl_v1_parser::pest_parser::{Rule, VsidlParser};

pub(crate) fn format_document(
    params: &DocumentFormattingParams,
    file_provider: &LspFileProvider,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    let uri = &params.text_document.uri;
    let url = Url::parse(uri.as_ref())?;
    let path = url.to_file_path().map_err(|_| anyhow::anyhow!("invalid file path"))?;
    let unformatted = file_provider.read_to_string(&path)?;

    let parsed = VsidlParser::parse(Rule::vsidl_file, &unformatted)?
        .next()
        .ok_or_else(|| anyhow!("Empty parse result"))?;

    let formatted = formatter::format(
        parsed,
        params.options.tab_size.try_into().with_context(|| {
            format!(
                "Invalid value for tab size: '{}'. Expected an integer between 0 and {}.",
                params.options.tab_size,
                u32::MAX
            )
        })?,
    );

    let range = lsp_types::Range {
        start: lsp_types::Position { line: 0, character: 0 },
        end: lsp_types::Position { line: unformatted.lines().count() as u32, character: 0 },
    };

    let result = Some(vec![TextEdit { range, new_text: formatted }]);
    let result = serde_json::to_value(result)?;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use lsp_types::Url;
    use lsp_types::{DocumentFormattingParams, TextDocumentIdentifier, WorkDoneProgressParams};
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use vsidl_v1_parser::file_provider::DiskFileProvider;

    #[test]
    fn test_format_document() -> anyhow::Result<()> {
        // GIVEN an unformatted VSIDL file
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let unformatted_vsidl_path = temp_dir.path().join("unformatted.vsidl");
        let mut unformatted_vsidl_file =
            File::create(&unformatted_vsidl_path).expect("Failed to create unformatted vsidl file");
        let unformatted_content = r##"package: "com.sdv.tm"
service_bundle {
  name: "Manager"

publisher { message: "com.sdv.tm.TirePressure" channel: "FRONT_RIGHT" }

  subscriber {
      message: "TirePressure"
      channel: "FRONT_RIGHT"
  }
}"##;
        unformatted_vsidl_file
            .write_all(unformatted_content.as_bytes())
            .expect("Failed to write to unformatted vsidl file");

        let file_provider = LspFileProvider {
            document_store: Arc::new(Mutex::new(HashMap::new())),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(unformatted_vsidl_path).unwrap(),
            },
            options: lsp_types::FormattingOptions { tab_size: 2, ..Default::default() },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let result = format_document(&params, &file_provider)?;

        // THEN the VSIDL was formatted correctly
        let expected_formatted_content = r##"package: "com.sdv.tm"

service_bundle {
  name: "Manager"

  publisher {
    message: "com.sdv.tm.TirePressure"
    channel: "FRONT_RIGHT"
  }

  subscriber {
    message: "TirePressure"
    channel: "FRONT_RIGHT"
  }
}
"##;

        assert!(result.is_some());
        let text_edits: Vec<TextEdit> = serde_json::from_value(result.unwrap())?;
        assert_eq!(text_edits.len(), 1);
        let text_edit = &text_edits[0];
        assert_eq!(text_edit.new_text, expected_formatted_content);

        Ok(())
    }
}
