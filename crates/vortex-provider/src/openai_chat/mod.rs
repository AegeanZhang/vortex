mod decoder;
mod request;
mod stream;

use self::{request::ChatRequest, stream::StreamContext};

use vortex_core::{ModelProvider, ModelRequest, ModelStream};

#[derive(Clone)]
struct SecretString(String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("***")
    }
}

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

impl ModelProvider for OpenAiChatProvider {
    /*
    fn stream(&self, model_request: ModelRequest) -> ModelStream {
        let chat_request = ChatRequest::from_model_request(
            self.model.clone(),
            model_request,
            self.qwen_thinking,
        );

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'),);

        let client = self.client.clone();
        let api_key = self.api_key.clone();

        // 2. 外层流产生 ModelEvent。
        //    HTTP 请求在流被 poll 后才开始执行。
        Box::pin(async_stream::try_stream! {
            // TODO: 对话内容可能包含敏感信息，仅用于本地调试，commit时需要移除
            trace!("{:?}", chat_request);
            // 3. 发送请求，限制等待响应头的时间。
            let response = tokio::time::timeout(
                Duration::from_secs(30),
                client
                    .post(url)
                    .bearer_auth(&api_key.0)
                    .json(&chat_request)
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
            let mut decoder = OpenAiChatDecoder::default();

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
    */

    fn stream(&self, model_request: ModelRequest) -> ModelStream {
        let request =
            ChatRequest::from_model_request(self.model.clone(), model_request, self.qwen_thinking);

        let context = StreamContext {
            client: self.client.clone(),
            url: format!("{}/chat/completions", self.base_url.trim_end_matches('/'),),
            api_key: self.api_key.clone(),
            request,
        };

        stream::stream(context)
    }
}
