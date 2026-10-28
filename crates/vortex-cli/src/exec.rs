use std::env::var;
use std::error::Error;
use std::result::Result;

use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

pub async fn execute(task: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    let api_key = var("DEEPSEEK_API_KEY")?;

    let client = Client::new();

    let request = ChatRequest {
        //model: "deepseek-v4-flash".to_string(),
        model: "deepseek-v4-pro".to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: task,
            },
        ],
        stream: false,
    };

    #[cfg(debug_assertions)]
    let mut wire = crate::wire_trace::WireTrace::new("exec", &[&api_key]);
    #[cfg(debug_assertions)]
    wire.json("request", &request);

    let response = client
        .post("https://api.deepseek.com/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;
    #[cfg(debug_assertions)]
    wire.head(response.status().as_u16());

    if let Err(error) = response.error_for_status_ref() {
        #[cfg(debug_assertions)]
        let _ = wire.capture_error_body(response);
        return Err(error.into());
    }

    let body = response.bytes().await?;
    #[cfg(debug_assertions)]
    wire.payload("response.body", &body, false);

    let response: ChatResponse = serde_json::from_slice(&body)?;

    if let Some(choice) = response.choices.first()
        && let Some(content) = &choice.message.content
    {
        println!("{content}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_response_bytes_and_optional_content() {
        let response: ChatResponse =
            serde_json::from_slice(br#"{"choices":[{"message":{"content":"fixture answer"}}]}"#)
                .unwrap();
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("fixture answer")
        );
        for body in [
            br#"{"choices":[]}"#.as_slice(),
            br#"{"choices":[{"message":{"content":null}}]}"#,
        ] {
            let response: ChatResponse = serde_json::from_slice(body).unwrap();
            assert!(
                response
                    .choices
                    .first()
                    .and_then(|c| c.message.content.as_deref())
                    .is_none()
            );
        }
    }

    #[test]
    fn invalid_response_bytes_return_json_error() {
        for body in [b"not json".as_slice(), br#"{"choices":"invalid"}"#] {
            assert!(serde_json::from_slice::<ChatResponse>(body).is_err());
        }
    }
}
