use std::time::Duration;

use eventsource_stream::{EventStreamError, Eventsource};
use futures_util::StreamExt;
use serde::Serialize;
use tracing::{debug, trace};

use vortex_core::{MessageRole, ModelProvider, ModelRequest, ModelStream, ProviderError};

mod sse;

#[derive(Debug, Clone, Copy)]
pub struct QwenThinkingOptions {
    pub enabled: bool,
    pub preserve_history: bool,
}

pub struct OpenAiChatProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: SecretString,
    qwen_thinking: Option<QwenThinkingOptions>,
}

#[derive(Clone)]
struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
}

impl OpenAiChatProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            model: model.into(),
            api_key: SecretString(api_key.into()),
            qwen_thinking: None,
        }
    }

    pub fn with_qwen_thinking(mut self, options: QwenThinkingOptions) -> Self {
        self.qwen_thinking = Some(options);
        self
    }
}

// Wire 类型
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preserve_thinking: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

/*
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}
*/

impl ModelProvider for OpenAiChatProvider {
    fn stream(&self, request: ModelRequest) -> ModelStream {
        // 1. 同步构造拥有所有权的请求数据。
        let messages = request
            .messages
            .into_iter()
            .map(|message| ChatMessage {
                role: match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                },
                content: message.content,
            })
            .collect();

        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            enable_thinking: self.qwen_thinking.map(|options| options.enabled),
            preserve_thinking: self.qwen_thinking.map(|options| options.preserve_history),
        };

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'),);

        let client = self.client.clone();
        let api_key = self.api_key.clone();

        // 2. 外层流产生 ModelEvent。
        //    HTTP 请求在流被 poll 后才开始执行。
        Box::pin(async_stream::try_stream! {
            debug!(
                model = %request.model,
                messages_count = request.messages.len(),
                stream = request.stream,
                "provider streaming request",
            );

            // 3. 发送请求，限制等待响应头的时间。
            let response = tokio::time::timeout(
                Duration::from_secs(30),
                client
                    .post(url)
                    .bearer_auth(&api_key.0)
                    .json(&request)
                    .send(),
            )
            .await
            .map_err(|_| {
                ProviderError::Transport(
                    "request header timeout".to_string(),
                )
            })?
            .map_err(|_| {
                ProviderError::Transport(
                    "request failed".to_string(),
                )
            })?;

            let status = response.status();

            if !status.is_success() {
                Err(ProviderError::HttpStatus {
                    status: status.as_u16(),
                })?;
            }

            // 4. 确认响应确实是 SSE。
            //    允许 text/event-stream; charset=utf-8 这样的形式。
            let is_event_stream = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| {
                    value.trim().eq_ignore_ascii_case("text/event-stream")
                });

            if !is_event_stream {
                Err(ProviderError::InvalidResponse(
                    "expected text/event-stream response".to_string(),
                ))?;
            }

            // 5. 内层流只产生 Bytes，负责网络读取与空闲超时。
            let byte_stream = async_stream::try_stream! {
                let mut body = response.bytes_stream();

                loop {
                    let next = tokio::time::timeout(
                        Duration::from_secs(120),
                        body.next(),
                    )
                    .await
                    .map_err(|_| {
                        ProviderError::Transport(
                            "response idle timeout".to_string(),
                        )
                    })?;

                    match next {
                        Some(Ok(bytes)) => {
                            yield bytes;
                        }

                        Some(Err(_)) => {
                            Err(ProviderError::Transport(
                                "response read failed".to_string(),
                            ))?;
                        }

                        None => {
                            break;
                        }
                    }
                }
            };

            // 6. 注意：这里已经在 byte_stream 定义之外。
            //    明确内层流的错误类型，帮助编译器推断。
            let typed_bytes =
                byte_stream.map(|item: Result<_, ProviderError>| item);

            let mut events = Box::pin(typed_bytes.eventsource());
            let mut decoder = sse::StreamDecoder::default();

            // 7. SSE 分帧后，再解析每个事件中的 JSON。
            while let Some(event) = events.next().await {
                let event = event.map_err(|error| match error {
                    EventStreamError::Transport(error) => error,

                    EventStreamError::Utf8(_)
                    | EventStreamError::Parser(_) => {
                        ProviderError::InvalidResponse(
                            "invalid SSE encoding or framing".to_string(),
                        )
                    }
                })?;

                for item in decoder.push_data(&event.data)? {
                    yield item;
                }

                if decoder.is_done() {
                    break;
                }
            }

            // 8. 没有合法 [DONE] 就断流，不能当作成功。
            decoder.finish_eof()?;
        })
    }
}
/*
impl ModelProvider for OpenAiChatProvider {
    fn stream(&self, request: ModelRequest) -> ModelStream {
        let byte_stream = async_stream::try_stream! {
            let mut body = response.bytes_stream();

            loop {
                let next = tokio::time::timeout(Duration::from_secs(120), body.next())
                    .await
                    .map_err(|_| ProviderError::Transport("response idle timeout".into()))?;
                match next {
                    Some(Ok(bytes)) => yield bytes,
                    Some(Err(_)) => {
                        Err(ProviderError::Transport("response read failed".into()))?;
                    }
                    None => break,
                }
            }

            let typed_bytes = byte_stream.map(|item: Result<_, ProviderError>| item);
            let mut events = Box::pin(typed_bytes.eventsource());
            let mut decoder = sse::StreamDecoder::default();

            while let Some(event) = events.next().await {
                let event = event.map_err(|error| match error {
                    EventStreamError::Transport(error) => error,
                    EventStreamError::Utf8(_) | EventStreamError::Parser(_) => {
                        ProviderError::InvalidResponse("invalid SSE encoding or framing".into())
                    }
                })?;
                for item in decoder.push_data(&event.data)? {
                    yield item;
                }
                if decoder.is_done() {
                    break;
                }
            }

            decoder.finish_eof()?;
        };
    }

    /*
    fn complete(&self, request: ModelRequest) -> ProviderFuture<'_> {
        Box::pin(async move {
            let messages = request
                .messages
                .into_iter()
                .map(|message| ChatMessage {
                    role: match message.role {
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::System => "system",
                    },
                    content: message.content,
                })
                .collect();

            let request = ChatRequest {
                model: self.model.clone(),
                messages,
                stream: false,
            };

            debug!(
                model = %request.model,
                messages_count = request.messages.len(),
                stream = request.stream,
                "provider request build"
            );

            trace!(
                request = ?request,
                "provider request body",
            );

            let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'),);

            let response = self
                .client
                .post(url)
                .bearer_auth(&self.api_key.0)
                .json(&request)
                .send()
                .await
                .map_err(|error| ProviderError::Transport(error.to_string()))?;

            let status = response.status();

            if !status.is_success() {
                return Err(ProviderError::HttpStatus {
                    status: status.as_u16(),
                });
            }

            let response = response
                .json::<ChatResponse>()
                .await
                .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;

            response
                .choices
                .into_iter()
                .next()
                .and_then(|choice| choice.message.content)
                .filter(|content| !content.is_empty())
                .ok_or_else(|| {
                    ProviderError::InvalidResponse(
                        "response does not contain assistant content".to_string(),
                    )
                })
        })
    }
    */
}
*/
