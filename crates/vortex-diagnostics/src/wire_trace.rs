use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tracing::instrument::WithSubscriber;

const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 16 * 1024;
const ERROR_TIMEOUT: Duration = Duration::from_millis(300);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct WireTrace {
    enabled: bool,
    component: &'static str,
    request_id: String,
    seq: u64,
    status: u16,
    secrets: Vec<String>,
    remaining: usize,
    limit_reported: bool,
}

impl WireTrace {
    pub fn new(component: &'static str, secrets: &[&str]) -> Self {
        let enabled = tracing::enabled!(target: "vortex_wire", tracing::Level::TRACE);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut secrets: Vec<String> = if enabled {
            secrets
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| (*s).to_owned())
                .collect()
        } else {
            Vec::new()
        };
        // 先替换较长的完整认证串，再替换原始 key。
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        Self {
            enabled,
            component,
            request_id: format!(
                "{component}--{}--{timestamp}--{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed),
            ),
            seq: 0,
            status: 0,
            secrets,
            remaining: MAX_REQUEST_BYTES,
            limit_reported: false,
        }
    }

    pub fn json(&mut self, phase: &'static str, value: &impl Serialize) {
        if !self.enabled {
            return;
        }
        match serde_json::to_vec(value) {
            Ok(bytes) => self.payload(phase, &bytes, false),
            Err(_) => self.emit("capture.serialization_failed", "", true, 0),
        }
    }

    pub fn head(&mut self, status: u16) {
        self.status = status;
        if self.enabled {
            self.emit("response.head", "", false, 0);
        }
    }

    pub fn payload(&mut self, phase: &'static str, bytes: &[u8], source_truncated: bool) {
        if !self.enabled {
            return;
        }
        if self.remaining == 0 {
            if !self.limit_reported {
                self.limit_reported = true;
                self.emit("capture.limit", "", true, bytes.len());
            }
            return;
        }
        let mut text = String::from_utf8_lossy(bytes).into_owned();
        for secret in &self.secrets {
            text = text.replace(secret, "[REDACTED]");
        }
        let mut end = text.len().min(MAX_RECORD_BYTES).min(self.remaining);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = source_truncated || end < text.len();
        self.remaining -= end;
        if end == 0 && !text.is_empty() {
            self.remaining = 0;
        }
        self.emit(phase, &text[..end], truncated, bytes.len());
    }

    fn emit(&mut self, phase: &'static str, body: &str, truncated: bool, input_bytes: usize) {
        self.seq += 1;
        tracing::trace!(
            target: "vortex_wire",
            component = self.component,
            request_id = %self.request_id,
            seq = self.seq,
            phase,
            status = self.status,
            input_bytes,
            captured_bytes = body.len(),
            truncated,
            body,
            "开发期 HTTP 诊断",
        );
    }

    pub fn capture_error_body(
        mut self,
        mut response: reqwest::Response,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.enabled {
            return None;
        }
        // 原调用方立即返回原错误；这个任务只作有界诊断。
        Some(tokio::spawn(
            async move {
                let mut body = Vec::new();
                let mut truncated = false;
                let outcome = tokio::time::timeout(ERROR_TIMEOUT, async {
                    while let Some(chunk) = response.chunk().await? {
                        let take = chunk.len().min(MAX_ERROR_BYTES.saturating_sub(body.len()));
                        body.extend_from_slice(&chunk[..take]);
                        if take < chunk.len() || body.len() == MAX_ERROR_BYTES {
                            truncated = true;
                            break;
                        }
                    }
                    Ok::<(), reqwest::Error>(())
                })
                .await;
                self.payload(
                    "response.error_body",
                    &body,
                    truncated || !matches!(outcome, Ok(Ok(()))),
                );
            }
            .with_current_subscriber(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        io,
        sync::{Arc, Mutex},
    };
    use tracing::Level;
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> MakeWriter<'a> for Buffer {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }
    fn capture(level: Level, action: impl FnOnce()) -> Vec<Value> {
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .json()
            .with_env_filter(tracing_subscriber::EnvFilter::new(format!(
                "off,vortex_wire={}",
                level.as_str().to_ascii_lowercase()
            )))
            .with_writer(buffer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, action);
        let bytes = buffer.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn records_request_and_response_with_one_id_and_masks_known_key() {
        let events = capture(Level::TRACE, || {
            let mut trace = WireTrace::new("fixture", &["fixture-secret"]);
            trace.json("request", &json!({"prompt": "fixture-secret"}));
            trace.head(200);
            trace.payload("response.body", b"fixture response", false);
        });
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["fields"]["phase"], "request");
        assert_eq!(events[1]["fields"]["status"], 200);
        assert_eq!(
            events[0]["fields"]["request_id"],
            events[2]["fields"]["request_id"]
        );
        assert_eq!(events[2]["fields"]["seq"], 3);
        let log = serde_json::to_string(&events).unwrap();
        assert!(!log.contains("fixture-secret"));
        assert!(log.contains("[REDACTED]"));
    }

    #[test]
    fn info_does_not_serialize_payload() {
        struct MustNotSerialize;
        impl Serialize for MustNotSerialize {
            fn serialize<S>(&self, _: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                panic!("disabled diagnostics must not serialize");
            }
        }
        let events = capture(Level::INFO, || {
            WireTrace::new("fixture", &[]).json("request", &MustNotSerialize);
        });
        assert!(events.is_empty());
    }

    #[test]
    fn each_request_has_a_distinct_id() {
        let events = capture(Level::TRACE, || {
            WireTrace::new("model", &[]).head(200);
            WireTrace::new("search", &[]).head(200);
            WireTrace::new("model", &[]).head(200);
        });
        let ids: std::collections::HashSet<_> = events
            .iter()
            .map(|event| event["fields"]["request_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn serialization_failure_does_not_expose_error_text() {
        struct FailsToSerialize;
        impl Serialize for FailsToSerialize {
            fn serialize<S>(&self, _: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(<S::Error as serde::ser::Error>::custom(
                    "private fixture detail",
                ))
            }
        }
        let events = capture(Level::TRACE, || {
            WireTrace::new("fixture", &[]).json("request", &FailsToSerialize);
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["fields"]["phase"], "capture.serialization_failed");
        assert!(
            !serde_json::to_string(&events)
                .unwrap()
                .contains("private fixture detail")
        );
    }

    #[test]
    fn truncates_on_utf8_boundary() {
        let events = capture(Level::TRACE, || {
            WireTrace::new("fixture", &[]).payload(
                "response.body",
                "中".repeat(MAX_RECORD_BYTES).as_bytes(),
                false,
            );
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["fields"]["truncated"], true);
        let body = events[0]["fields"]["body"].as_str().unwrap();
        assert!(body.len() <= MAX_RECORD_BYTES);
        assert!(body.chars().all(|ch| ch == '中'));
    }

    #[test]
    fn reports_budget_exhaustion_once() {
        let events = capture(Level::TRACE, || {
            let mut trace = WireTrace::new("fixture", &[]);
            let chunk = vec![b'x'; MAX_RECORD_BYTES];
            for _ in 0..(MAX_REQUEST_BYTES / MAX_RECORD_BYTES) {
                trace.payload("response.body", &chunk, false);
            }
            trace.payload("response.body", b"extra", false);
            trace.payload("response.body", b"more", false);
        });
        let captured: usize = events
            .iter()
            .map(|event| event["fields"]["captured_bytes"].as_u64().unwrap() as usize)
            .sum();
        assert_eq!(captured, MAX_REQUEST_BYTES);
        assert_eq!(
            events
                .iter()
                .filter(|event| { event["fields"]["phase"] == "capture.limit" })
                .count(),
            1
        );
        assert_eq!(events.last().unwrap()["fields"]["truncated"], true);
    }

    struct AbortServer(tokio::task::JoinHandle<()>);
    impl Drop for AbortServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    // 使用固定响应和假密钥，不读取环境变量或访问外网。
    async fn error_response(
        body: Vec<u8>,
        hold: bool,
    ) -> (
        reqwest::Response,
        AbortServer,
        tokio::sync::oneshot::Sender<()>,
    ) {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
            sync::oneshot,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (release, released) = oneshot::channel();
        let server = AbortServer(tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut bytes = [0; 1024];
                let count = socket.read(&mut bytes).await.unwrap();
                assert!(count > 0 && request.len() < 16 * 1024);
                request.extend_from_slice(&bytes[..count]);
            }
            let length = body.len() + usize::from(hold);
            socket.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
            if hold {
                let _ = released.await;
            }
        }));
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap();
        (response, server, release)
    }

    async fn preview(body: Vec<u8>, hold: bool) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            let buffer = Buffer::default();
            let subscriber = tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .json()
                .with_env_filter(tracing_subscriber::EnvFilter::new("off,vortex_wire=trace"))
                .with_writer(buffer.clone())
                .finish();
            async {
                let (response, mut server, release) = error_response(body, hold).await;
                let mut trace = WireTrace::new("fixture", &["fixture-secret"]);
                trace.head(401);
                trace.capture_error_body(response).unwrap().await.unwrap();
                let _ = release.send(());
                (&mut server.0).await.unwrap();
            }
            .with_subscriber(subscriber)
            .await;
            let bytes = buffer.0.lock().unwrap().clone();
            let events: Vec<Value> = String::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(events.len(), 2);
            assert_eq!(
                events[0]["fields"]["request_id"],
                events[1]["fields"]["request_id"]
            );
            events[1].clone()
        })
        .await
        .expect("error preview timed out")
    }

    #[tokio::test]
    async fn error_preview_masks_secret_and_keeps_subscriber_in_spawned_task() {
        let event = preview(b"fixture-secret: denied".to_vec(), false).await;
        assert_eq!(event["fields"]["phase"], "response.error_body");
        assert_eq!(event["fields"]["status"], 401);
        assert_eq!(event["fields"]["body"], "[REDACTED]: denied");
        assert_eq!(event["fields"]["truncated"], false);
    }

    #[tokio::test]
    async fn error_preview_is_bounded_by_size() {
        let event = preview(vec![b'x'; MAX_ERROR_BYTES + 1024], false).await;
        assert_eq!(event["fields"]["captured_bytes"], MAX_ERROR_BYTES);
        assert_eq!(event["fields"]["truncated"], true);
    }

    #[tokio::test]
    async fn error_preview_times_out_without_waiting_for_server_eof() {
        // 服务端保留最后一字节，直到预览任务已退出才释放。
        let event = preview(b"partial".to_vec(), true).await;
        assert_eq!(event["fields"]["body"], "partial");
        assert_eq!(event["fields"]["truncated"], true);
    }

    #[tokio::test]
    async fn info_does_not_spawn_an_error_preview() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let buffer = Buffer::default();
            let subscriber = tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::new("off,vortex_wire=info"))
                .with_writer(buffer.clone())
                .finish();
            async {
                let (response, mut server, release) = error_response(Vec::new(), true).await;
                assert!(
                    WireTrace::new("fixture", &[])
                        .capture_error_body(response)
                        .is_none()
                );
                let _ = release.send(());
                (&mut server.0).await.unwrap();
            }
            .with_subscriber(subscriber)
            .await;
            assert!(buffer.0.lock().unwrap().is_empty());
        })
        .await
        .unwrap();
    }

    #[test]
    fn masks_longest_secret_first_before_truncating() {
        let events = capture(Level::TRACE, || {
            let mut trace =
                WireTrace::new("fixture", &["fixture-secret", "Bearer fixture-secret", ""]);
            let body = format!("{}Bearer fixture-secret", "x".repeat(MAX_RECORD_BYTES - 5));
            trace.payload("response.body", body.as_bytes(), false);
        });
        let body = events[0]["fields"]["body"].as_str().unwrap();
        assert_eq!(body.len(), MAX_RECORD_BYTES);
        assert!(body.ends_with("[REDA"));
        assert!(!body.contains("Bearer"));
        assert_eq!(events[0]["fields"]["truncated"], true);
    }
}
