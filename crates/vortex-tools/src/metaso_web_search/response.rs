use std::collections::HashSet;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use vortex_core::ToolError;

const MAX_RESULTS: usize = 5;
const MAX_TITLE_CHARS: usize = 160;
const MAX_SNIPPET_CHARS: usize = 800;
const MAX_URL_BYTES: usize = 2_048;
pub(super) const MAX_OUTPUT_BYTES: usize = 16 * 1_024;

// 契约确认点： 字段必须以实际官方相应为准。
#[derive(Deserialize)]
struct MetasoResponse {
    webpages: Vec<MetasoPage>,
}

#[derive(Deserialize)]
struct MetasoPage {
    title: Option<String>,
    link: String,
    snippet: Option<String>,
}

#[derive(Serialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Serialize)]
struct SearchOutput<'a> {
    backend: &'static str,
    query: &'a str,
    results: Vec<SearchResult>,
    truncated: bool,
}

fn protocal_error() -> ToolError {
    ToolError::Execution("秘塔相应结构不符合已确认的搜索契约".to_string())
}

fn bounded_text(text: &str, max_chars: usize) -> (String, bool) {
    let cleaned: String = text
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect();
    let trimmed = cleaned.trim();
    let truncated = trimmed.chars().count() > max_chars;
    (trimmed.chars().take(max_chars).collect(), truncated)
}

fn result_url(link: &str) -> Option<String> {
    if link.len() > MAX_URL_BYTES || link.chars().any(char::is_control) {
        return None;
    }
    let url = Url::parse(link.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let url = url.to_string();
    (url.len() <= MAX_URL_BYTES).then_some(url)
}

pub(super) fn normalize_response(body: &[u8], query: &str) -> Result<String, ToolError> {
    let response: MetasoResponse = serde_json::from_slice(body).map_err(|_| protocal_error())?;
    let had_pages = !response.webpages.is_empty();
    let mut seen = HashSet::new();
    let mut output = SearchOutput {
        backend: "metaso",
        query,
        results: Vec::new(),
        truncated: false,
    };

    for page in response.webpages {
        if output.results.len() == MAX_RESULTS {
            output.truncated = true;
            break;
        }
        let Some(url) = result_url(&page.link) else {
            output.truncated = true;
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }

        let raw_title = page.title.as_deref().unwrap_or(&url);
        let raw_title = if raw_title.trim().is_empty() {
            &url
        } else {
            raw_title
        };
        let (title, title_truncated) = bounded_text(raw_title, MAX_TITLE_CHARS);
        let (snippet, snippet_truncated) =
            bounded_text(page.snippet.as_deref().unwrap_or(""), MAX_SNIPPET_CHARS);
        output.truncated |= title_truncated || snippet_truncated;
        output.results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }

    if had_pages && output.results.is_empty() {
        return Err(ToolError::Execution(
            "秘塔响应没有可用的 HTTP(s) 结果，请检查相应映射".to_string(),
        ));
    }

    loop {
        let encoded = serde_json::to_string(&output)
            .map_err(|_| ToolError::Execution("搜索结果序列化失败".to_string()))?;
        if encoded.len() <= MAX_OUTPUT_BYTES {
            return Ok(encoded);
        }
        if output.results.pop().is_none() {
            return Err(ToolError::Execution("搜索结果超出输出预算".to_string()));
        }
        output.truncated = true;
    }
}
