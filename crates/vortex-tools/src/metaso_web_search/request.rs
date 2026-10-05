use serde::Serialize;
use serde_json::Value;
use vortex_core::ToolError;

pub(super) const MAX_QUERY_CHARS: usize = 512;

pub(super) fn parse_query(arguments: Value) -> Result<String, ToolError> {
    let object = arguments
        .as_object()
        .ok_or_else(|| ToolError::InvalidArguments("参数必须是 JSON 对象".to_string()))?;

    if object.len() != 1 || !object.contains_key("query") {
        return Err(ToolError::InvalidArguments(
            "参数必须且只能包含 query 字段".to_string(),
        ));
    }

    let query = object["query"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidArguments("query 必须是字符串".to_string()))?;
    let query = query.trim();

    if query.is_empty() {
        return Err(ToolError::InvalidArguments("query 不能为空".to_string()));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(ToolError::InvalidArguments(format!(
            "query 最多允许 {MAX_QUERY_CHARS} 个字符"
        )));
    }

    Ok(query.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MetasoSearchRequest<'a> {
    q: &'a str,
    scope: &'static str,
    size: &'static str,
    include_summary: bool,
    include_raw_content: bool,
    concise_snippet: bool,
}

impl<'a> MetasoSearchRequest<'a> {
    pub(super) fn new(query: &'a str) -> Self {
        Self {
            q: query,
            scope: "webpage",
            size: "5",
            include_summary: false,
            include_raw_content: false,
            concise_snippet: false,
        }
    }
}
