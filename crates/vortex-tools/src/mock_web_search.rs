use serde_json::{Value, json};
use vortex_core::{Tool, ToolError, ToolFuture, ToolSpec};

#[derive(Debug, Default)]
pub struct MockWebSearch;

impl MockWebSearch {
    pub const fn new() -> Self {
        Self
    }
}

impl Tool for MockWebSearch {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "web_search".to_string(),
            description: "Search the web. The current backend returns mock data.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|query| !query.is_empty())
                .ok_or_else(|| {
                    ToolError::InvalidArguments(
                        "field `query` must be a non-empty string".to_string(),
                    )
                })?;

            Ok(json!({
                "mock": true,
                "query": query,
                "results": [{
                    "title": "Mock search result",
                    "url": "https://example.invalid/result-1",
                    "snippet": "This is deterministic mock data."
                }]   
            })
            .to_string())
        })
    }
}
