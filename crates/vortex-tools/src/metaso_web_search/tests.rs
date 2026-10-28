mod support;

use super::*;
use support::{Capture, WAIT, response, serve};
use tokio::{io::AsyncWriteExt, sync::oneshot, time::timeout};
use tracing::instrument::WithSubscriber;

fn fixture(url: String, limit: usize) -> MetasoWebSearch {
    let client = Client::builder()
        .no_proxy()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    MetasoWebSearch::from_parts(
        client,
        "fixture-secret".into(),
        url + "/search",
        Limits {
            request_timeout: Duration::from_secs(3),
            max_response_bytes: limit,
        },
        2,
    )
    .unwrap()
}

async fn run_case(
    status: u16,
    mime: &'static str,
    body: String,
    level: &str,
    limit: usize,
) -> (Result<String, ToolError>, Vec<Value>) {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch(level);
        let (url, mut server) = serve(move |mut socket, request| async move {
            assert_eq!(
                request,
                json!({
                    "q": "query fixture-secret", "scope": "webpage", "size": "2",
                    "includeSummary": false, "includeRawContent": false, "conciseSnippet": false,
                })
            );
            response(&mut socket, status, mime, &body).await;
        })
        .await;
        let result = async {
            let result = fixture(url, limit)
                .execute(json!({"query": "query fixture-secret"}))
                .await;
            if cfg!(debug_assertions) && level == "trace" && (status != 200 || mime == "text/html")
            {
                capture.wait_phase("response.error_body").await;
            }
            (&mut server.0).await.unwrap();
            result
        }
        .with_subscriber(dispatch)
        .await;
        let events = capture.events();
        if cfg!(debug_assertions) && level == "trace" {
            assert_eq!(events[0]["fields"]["component"], "search");
            assert_eq!(events[0]["fields"]["phase"], "request");
            for (index, event) in events.iter().enumerate() {
                assert_eq!(
                    event["fields"]["request_id"],
                    events[0]["fields"]["request_id"]
                );
                assert_eq!(event["fields"]["seq"], index + 1);
            }
            let log = serde_json::to_string(&events).unwrap();
            assert!(!log.contains("fixture-secret"));
            assert!(log.contains("[REDACTED]"));
        } else {
            assert!(events.is_empty());
        }
        (result, events)
    })
    .await
    .expect("search fixture timed out")
}

#[tokio::test]
async fn success_records_raw_body_without_changing_request_or_normalized_output() {
    let raw = json!({"webpages": [{"title": "fixture-secret title", "link": "https://example.test/page", "snippet": "fixture snippet"}]}).to_string();
    for level in ["trace", "info"] {
        let (result, events) = run_case(
            200,
            "application/json; charset=utf-8",
            raw.clone(),
            level,
            256 * 1024,
        )
        .await;
        let output: Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(output["query"], "query fixture-secret");
        assert_eq!(output["results"][0]["title"], "fixture-secret title");
        if cfg!(debug_assertions) && level == "trace" {
            assert_eq!(events.len(), 3);
            assert_eq!(events[2]["fields"]["phase"], "response.body");
            assert_eq!(
                events[2]["fields"]["body"],
                raw.replace("fixture-secret", "[REDACTED]")
            );
        }
    }
}

#[tokio::test]
async fn malformed_json_is_captured_before_normalization_error() {
    for level in ["trace", "info"] {
        let (result, events) = run_case(
            200,
            "application/json",
            "{broken fixture-secret".into(),
            level,
            1024,
        )
        .await;
        assert!(matches!(result, Err(ToolError::Execution(_))));
        if cfg!(debug_assertions) && level == "trace" {
            assert_eq!(events.last().unwrap()["fields"]["phase"], "response.body");
            assert_eq!(
                events.last().unwrap()["fields"]["body"],
                "{broken [REDACTED]"
            );
        }
    }
}

#[tokio::test]
async fn http_and_mime_errors_keep_business_errors_and_capture_preview() {
    for (status, mime) in [(401, "application/json"), (200, "text/html")] {
        for level in ["trace", "info"] {
            let (result, events) =
                run_case(status, mime, "fixture-secret denied".into(), level, 1024).await;
            let error = result.unwrap_err().to_string();
            assert!(error.contains(if status == 401 {
                "HTTP 401"
            } else {
                "不是 JSON"
            }));
            assert!(!error.contains("fixture-secret"));
            if cfg!(debug_assertions) && level == "trace" {
                assert_eq!(events.len(), 3);
                assert_eq!(events[2]["fields"]["body"], "[REDACTED] denied");
                assert_eq!(events[2]["fields"]["truncated"], false);
            }
        }
    }
}

#[tokio::test]
async fn diagnostics_do_not_bypass_declared_body_limit() {
    for level in ["trace", "info"] {
        let (result, events) =
            run_case(200, "application/json", "x".repeat(2048), level, 1024).await;
        assert!(result.unwrap_err().to_string().contains("正文大小限制"));
        if cfg!(debug_assertions) && level == "trace" {
            assert_eq!(events.len(), 3);
            assert_eq!(events[2]["fields"]["phase"], "response.body_unavailable");
            assert_eq!(events[2]["fields"]["truncated"], true);
            assert_eq!(events[2]["fields"]["body"], "");
        }
    }
}

#[tokio::test]
async fn diagnostics_do_not_bypass_streaming_body_limit_without_content_length() {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch("trace");
        let (url, mut server) = serve(|mut socket, _| async move {
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").await.unwrap();
            socket.write_all(&vec![b'x'; 2048]).await.unwrap();
            socket.shutdown().await.unwrap();
        }).await;
        async {
            let result = fixture(url, 1024).execute(json!({"query": "fixture"})).await;
            assert!(result.unwrap_err().to_string().contains("正文大小限制"));
            (&mut server.0).await.unwrap();
        }.with_subscriber(dispatch).await;
        if cfg!(debug_assertions) {
            assert_eq!(capture.events().last().unwrap()["fields"]["phase"], "response.body_unavailable");
        } else { assert!(capture.events().is_empty()); }
    }).await.unwrap();
}

#[tokio::test]
async fn diagnostics_do_not_extend_tool_timeout() {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch("trace");
        let (release, released) = oneshot::channel();
        let (url, mut server) = serve(move |mut socket, _| async move {
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 10\r\nConnection: close\r\n\r\n").await.unwrap();
            released.await.unwrap();
        }).await;
        async {
            let mut tool = fixture(url, 1024);
            tool.limits.request_timeout = Duration::from_millis(100);
            let result = tool.execute(json!({"query": "fixture"})).await;
            assert!(result.unwrap_err().to_string().contains("工具总超时"));
            release.send(()).unwrap();
            (&mut server.0).await.unwrap();
        }.with_subscriber(dispatch).await;
    }).await.unwrap();
}
