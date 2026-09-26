use futures_util::StreamExt;
use vortex_core::ModelStream;

use super::{SecretString, decoder::OpenAiChatDecoder, request::ChatRequest};

pub(super) struct StreamContext {
    pub(super) client: reqwest::Client,
    pub(super) url: String,
    pub(super) api_key: SecretString,
    pub(super) request: ChatRequest,
}

pub(super) fn stream(context: StreamContext) -> ModelStream {
    Box::pin(async_stream::try_stream! {
        let request = context
            .client
            .post(context.url)
            .bearer_auth(&context.api_key.0)
            .json(&context.request);

        let mut events = crate::sse_transport::stream(request);
        let mut decoder = OpenAiChatDecoder::default();

        while let Some(event) = events.next().await {
            let event = event?;

            for model_event in decoder.push_data(&event.data)? {
                yield model_event;
            }

            if decoder.is_done() {
                break;
            }
        }

        decoder.finish_eof()?;
    })
}
