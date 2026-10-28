mod support;

use futures_util::StreamExt;
use serde_json::{Value, json};
use support::{Capture, WAIT, response, serve};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    time::timeout,
};
use tracing::instrument::WithSubscriber;
use vortex_core::{
    ConversationItem, FinishReason, MessageRole, ModelEvent, ModelProvider, ModelRequest,
    ProviderError, SessionMessage,
};
use vortex_provider::OpenAiChatProvider;

fn provider(url: String) -> OpenAiChatProvider {
    let client = reqwest::Client::builder()
        .no_proxy()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    OpenAiChatProvider::from_client(client, url, "fixture-model", "fixture-secret")
}

fn request() -> ModelRequest {
    ModelRequest {
        items: vec![ConversationItem::Message(SessionMessage {
            role: MessageRole::User,
            content: "prompt fixture-secret".into(),
        })],
        tools: Vec::new(),
    }
}

fn sse(delta: Value, finish: Option<&str>) -> String {
    format!(
        "data: {}\n\n",
        json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
    )
}

async fn streaming_fixture(level: &str) {
    let capture = Capture::default();
    let dispatch = capture.dispatch(level);
    timeout(WAIT, async {
        let (release, released) = oneshot::channel();
        let (url, mut server) = serve(move |mut socket, body| async move {
            assert_eq!(body["messages"][0]["content"], "prompt fixture-secret");
            assert_eq!(body["stream"], true);
            assert_eq!(body["model"], "fixture-model");
            let first = sse(json!({"content": "Hello fixture-secret"}), None);
            let last = sse(json!({}), Some("stop")) + "data: [DONE]\n\n";
            let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", first.len() + last.len());
            socket.write_all(headers.as_bytes()).await.unwrap();
            // 将 SSE 前缀拆到两次写入，客户端仍按事件而非网络块解析。
            socket.write_all(&first.as_bytes()[..3]).await.unwrap();
            socket.write_all(&first.as_bytes()[3..]).await.unwrap();
            released.await.unwrap();
            socket.write_all(last.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }).await;
        let mut stream = provider(url).stream(request());
        assert_eq!(stream.next().await.unwrap().unwrap(), ModelEvent::TextDelta { delta: "Hello fixture-secret".into() });
        release.send(()).unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap(), ModelEvent::Finished { reason: FinishReason::Stop });
        assert!(stream.next().await.is_none());
        (&mut server.0).await.unwrap();
    }.with_subscriber(dispatch)).await.expect("streaming fixture timed out");

    let events = capture.events();
    if cfg!(debug_assertions) && level == "trace" {
        assert_eq!(events.len(), 5);
        assert_eq!(events[0]["fields"]["phase"], "request");
        assert_eq!(events[1]["fields"]["phase"], "response.head");
        assert_eq!(events[4]["fields"]["body"], "[DONE]");
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
}

#[tokio::test]
async fn trace_keeps_streaming_and_obeys_build_gate() {
    streaming_fixture("trace").await;
}

#[tokio::test]
async fn info_keeps_streaming_without_wire_events() {
    streaming_fixture("info").await;
}

#[tokio::test]
async fn reasoning_and_tool_arguments_are_captured_before_decoding() {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch("trace");
        let body = sse(json!({"reasoning_content": "fixture thought"}), None)
            + &sse(json!({"tool_calls": [{"index": 0, "id": "call-1", "type": "function", "function": {"name": "web_search", "arguments": "{\"query\":\"fixture-secret\"}"}}]}), None)
            + &sse(json!({}), Some("tool_calls")) + "data: [DONE]\n\n";
        let (url, mut server) = serve(move |mut socket, _| async move {
            response(&mut socket, 200, "text/event-stream", &body).await;
        }).await;
        async {
            let mut stream = provider(url).stream(request());
            assert_eq!(stream.next().await.unwrap().unwrap(), ModelEvent::ReasoningDelta { delta: "fixture thought".into() });
            let ModelEvent::ToolCall { call } = stream.next().await.unwrap().unwrap() else { panic!("expected tool call") };
            assert_eq!(call.arguments, json!({"query": "fixture-secret"}));
            assert_eq!(stream.next().await.unwrap().unwrap(), ModelEvent::Finished { reason: FinishReason::ToolCalls });
            assert!(stream.next().await.is_none());
            (&mut server.0).await.unwrap();
        }.with_subscriber(dispatch).await;
        let events = capture.events();
        if cfg!(debug_assertions) {
            let bodies: Vec<_> = events.iter().filter(|e| e["fields"]["phase"] == "response.sse")
                .map(|e| e["fields"]["body"].as_str().unwrap()).collect();
            assert_eq!(bodies.len(), 4);
            assert!(bodies[0].contains("reasoning_content"));
            assert!(bodies[1].contains("[REDACTED]"));
            assert!(!serde_json::to_string(&events).unwrap().contains("fixture-secret"));
        } else { assert!(events.is_empty()); }
    }).await.unwrap();
}

#[tokio::test]
async fn http_and_mime_errors_return_before_body_and_keep_original_error() {
    for (status, mime) in [(401, "application/json"), (200, "application/json")] {
        for level in ["trace", "info"] {
            timeout(WAIT, async {
                let capture = Capture::default();
                let dispatch = capture.dispatch(level);
                let (release, released) = oneshot::channel();
                let body = "{\"error\":\"fixture-secret\"}";
                let (url, mut server) = serve(move |mut socket, _| async move {
                    let headers = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    socket.write_all(headers.as_bytes()).await.unwrap();
                    released.await.unwrap();
                    // INFO 和 Release 不读取错误正文，允许客户端提前关闭。
                    let _ = socket.write_all(body.as_bytes()).await;
                }).await;
                async {
                    let mut stream = provider(url).stream(request());
                    let error = stream.next().await.unwrap().unwrap_err();
                    let expected = if status == 401 { ProviderError::HttpStatus { status } }
                        else { ProviderError::InvalidResponse("expected text/event-stream response".into()) };
                    assert_eq!(error, expected);
                    // 只有业务错误先返回，服务端才获准发送错误正文。
                    release.send(()).unwrap();
                    if cfg!(debug_assertions) && level == "trace" {
                        let event = capture.wait_phase("response.error_body").await;
                        assert_eq!(event["fields"]["body"], "{\"error\":\"[REDACTED]\"}");
                        assert_eq!(event["fields"]["status"], status);
                        assert_eq!(event["fields"]["truncated"], false);
                    } else { assert!(capture.events().is_empty()); }
                    (&mut server.0).await.unwrap();
                }.with_subscriber(dispatch).await;
            }).await.unwrap();
        }
    }
}

#[tokio::test]
async fn dropping_active_stream_closes_connection_without_waiting_for_eof() {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch("trace");
        let (url, mut server) = serve(|mut socket, _| async move {
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
            socket.write_all(sse(json!({"content": "first"}), None).as_bytes()).await.unwrap();
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
        }).await;
        async {
            let mut stream = provider(url).stream(request());
            assert!(matches!(stream.next().await.unwrap().unwrap(), ModelEvent::TextDelta { .. }));
            drop(stream);
            (&mut server.0).await.unwrap();
        }.with_subscriber(dispatch).await;
    }).await.unwrap();
}

#[tokio::test]
async fn malformed_json_is_logged_before_decoder_rejects_it() {
    timeout(WAIT, async {
        let capture = Capture::default();
        let dispatch = capture.dispatch("trace");
        let (url, mut server) = serve(|mut socket, _| async move {
            response(
                &mut socket,
                200,
                "text/event-stream",
                "data: {broken fixture-secret\n\n",
            )
            .await;
        })
        .await;
        async {
            let mut stream = provider(url).stream(request());
            assert_eq!(
                stream.next().await.unwrap().unwrap_err(),
                ProviderError::InvalidResponse("invalid JSON in SSE data".into())
            );
            (&mut server.0).await.unwrap();
        }
        .with_subscriber(dispatch)
        .await;
        if cfg!(debug_assertions) {
            assert_eq!(
                capture.events().last().unwrap()["fields"]["body"],
                "{broken [REDACTED]"
            );
        } else {
            assert!(capture.events().is_empty());
        }
    })
    .await
    .unwrap();
}
