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
use crate::lsp::recovering_parser::parse_vsidl_with_recovery;
use crate::lsp::token_builder::TokenBuilder;
use lsp_types::SemanticTokensParams;
use vsidl_v1_parser::file_provider::FileProvider;

pub(crate) fn semantic_tokens(
    params: SemanticTokensParams,
    file_provider: &LspFileProvider,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    eprintln!("semanticTokens called with uri: {:?}", params.text_document.uri,);

    let uri = params.text_document.uri;
    if let Ok(path) = uri.to_file_path() {
        let mut content = file_provider.read_to_string(&path).unwrap_or_default();

        let mut token_builder = TokenBuilder::new();
        let (result, _) = parse_vsidl_with_recovery(&mut content);
        if let Ok(pairs) = result {
            for pair in pairs {
                token_builder.push(pair);
            }
        }
        let tokens = token_builder.build();

        let result = serde_json::to_value(tokens)?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::setup_test_catalog;
    use lsp_types::{
        SemanticToken, SemanticTokens, SemanticTokensParams, TextDocumentIdentifier, Url,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;

    #[test]
    fn test_semantic_tokens() -> anyhow::Result<()> {
        // GIVEN a VSIDL file
        let temp_dir = setup_test_catalog();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");
        let vsidl_content = std::fs::read_to_string(&vsidl_file_path)?;

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        document_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(Url::from_file_path(&vsidl_file_path).unwrap(), vsidl_content.clone());
        let file_provider = LspFileProvider {
            document_store,
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let params = SemanticTokensParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(vsidl_file_path).unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        // WHEN semantic tokens are requested
        let result = semantic_tokens(params, &file_provider)?;

        // THEN the correct tokens are returned
        assert!(result.is_some());
        let tokens: SemanticTokens = serde_json::from_value(result.unwrap())?;

        let expected_tokens = vec![
            // Package String
            SemanticToken {
                delta_line: 0,
                delta_start: 9,
                length: 12,
                token_type: 2,
                token_modifiers_bitset: 0,
            },
            // Service Bundle Keyword
            SemanticToken {
                delta_line: 1,
                delta_start: 0,
                length: 14,
                token_type: 0,
                token_modifiers_bitset: 0,
            },
            // Publisher Keyword
            SemanticToken {
                delta_line: 2,
                delta_start: 2,
                length: 9,
                token_type: 0,
                token_modifiers_bitset: 0,
            },
            // "com.sdv.tm.TirePressure" String
            SemanticToken {
                delta_line: 0,
                delta_start: 9,
                length: 25,
                token_type: 2,
                token_modifiers_bitset: 0,
            },
            // "FRONT_RIGHT" String
            SemanticToken {
                delta_line: 0,
                delta_start: 9,
                length: 13,
                token_type: 2,
                token_modifiers_bitset: 0,
            },
            // Subscriber Keyword
            SemanticToken {
                delta_line: 3,
                delta_start: 2,
                length: 10,
                token_type: 0,
                token_modifiers_bitset: 0,
            },
            // "TirePressure" String
            SemanticToken {
                delta_line: 0,
                delta_start: 9,
                length: 14,
                token_type: 2,
                token_modifiers_bitset: 0,
            },
            // "FRONT_RIGHT" String
            SemanticToken {
                delta_line: 0,
                delta_start: 9,
                length: 13,
                token_type: 2,
                token_modifiers_bitset: 0,
            },
        ];
        for token in expected_tokens {
            assert!(tokens.data.contains(&token));
        }

        Ok(())
    }
}
