use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{SemanticToken, SemanticTokens, SemanticTokensParams, SemanticTokensResult},
};
use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::{
    util::{get_node_text, TextProviderRope},
    Backend, SymbolInfo, QUERY_LANGUAGE,
};

pub async fn semantic_tokens_full(
    backend: &Backend,
    params: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>> {
    let uri = &params.text_document.uri;
    let mut tokens = Vec::new();
    if let (Some(tree), Some(rope), Some(supertypes)) = (
        backend.cst_map.get(uri),
        &backend.document_map.get(uri),
        backend.supertype_map_map.get(uri),
    ) {
        let query = Query::new(&QUERY_LANGUAGE, "(named_node (identifier) @cap)").unwrap();
        let mut cursor = QueryCursor::new();
        let provider = TextProviderRope(rope);
        let mut matches = cursor.matches(&query, tree.root_node(), &provider);
        let mut prev_line = 0;
        let mut prev_col = 0;
        while let Some(match_) = matches.next() {
            for cap in match_.captures.iter() {
                let node = &cap.node;
                let node_text = get_node_text(node, rope);
                let start_row = node.start_position().row as u32;
                let start_col = node.start_position().column as u32;
                let delta_line = start_row - prev_line;
                let length = node.byte_range().len() as u32;
                let delta_start = if start_row - prev_line == 0 {
                    start_col - prev_col
                } else {
                    start_col
                };
                if node_text == "ERROR" {
                    tokens.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length,
                        token_type: 1,
                        token_modifiers_bitset: 1,
                    });
                    prev_line = start_row;
                    prev_col = start_col;
                } else if supertypes.contains_key(&SymbolInfo {
                    label: node_text,
                    named: true,
                }) {
                    tokens.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                    prev_line = start_row;
                    prev_col = start_col;
                }
            }
        }
    }
    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    })))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use tower::{Service, ServiceExt};
    use tower_lsp::lsp_types::{
        request::SemanticTokensFullRequest, PartialResultParams, SemanticToken, SemanticTokens,
        SemanticTokensParams, SemanticTokensResult, TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::test_helpers::helpers::{
        initialize_server, lsp_request_to_jsonrpc_request, lsp_response_to_jsonrpc_response,
        TEST_URI,
    };

    #[tokio::test(flavor = "current_thread")]
    async fn semantic_tokens_full() {
        // Arrange
        let source = r"(ERROR) @error (supertype) @node (supertype) @node

(supertype) @node
        ";
        let mut service = initialize_server(&[(
            TEST_URI.clone(),
            source,
            Vec::new(),
            Vec::new(),
            vec!["supertype"],
        )])
        .await;

        // Act
        let tokens = service
            .ready()
            .await
            .unwrap()
            .call(lsp_request_to_jsonrpc_request::<SemanticTokensFullRequest>(
                SemanticTokensParams {
                    partial_result_params: PartialResultParams {
                        partial_result_token: None,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    text_document: TextDocumentIdentifier {
                        uri: TEST_URI.clone(),
                    },
                },
            ))
            .await
            .unwrap();

        // Assert
        let actual = Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 1,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 1,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 15,
                    length: 9,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 18,
                    length: 9,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 2,
                    delta_start: 1,
                    length: 9,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
            ],
        }));
        assert_eq!(
            tokens,
            Some(lsp_response_to_jsonrpc_response::<SemanticTokensFullRequest>(actual))
        );
    }
}
