use std::pin::Pin;
use std::time::Duration;

use eventsource_stream::{Event, EventStreamError, Eventsource};
use futures_util::{Stream, StreamExt};

use vortex_core::ProviderError;

pub(crate) type SseEventStream =
    Pin<Box<dyn Stream<Item = Result<Event, ProviderError>> + Send + 'static>>;

pub(crate) fn stream(request: reqwest::RequestBuilder) -> SseEventStream {
    Box::pin(async_stream::try_stream! {
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            request.send(),
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
                        "response idel timeout".to_string(),
                    )
                })?;

                match next {
                    Some(Ok(bytes)) => yield bytes,

                    Some(Err(_)) => {
                        Err(ProviderError::Transport(
                            "response read failed".to_string(),
                        ))?;
                    }

                    None => break,
                }
            }
        };

        let typed_bytes =
            byte_stream.map(|item: Result<_, ProviderError>| item);

        let mut events = Box::pin(typed_bytes.eventsource());

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

            yield event;
        }
    })
}
