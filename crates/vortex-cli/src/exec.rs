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
    content: Option<String>
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>
}

pub async fn execute(task: String) -> Result<(), Box<dyn Error + Send + Sync>> {

    let api_key = var("DEEPSEEK_API_KEY")?;

    let client = Client::new();

    let request = ChatRequest {
        model: "deepseek-v4-flash".to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: task,
            }
        ],
        stream: false
    };

    let response = client.post("https://api.deepseek.com/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;

    let response: ChatResponse = response.json().await?;

    if let Some(choice) = response.choices.first() {
        if let Some(content) = &choice.message.content {
            println!("{content}");
        }
    }

    Ok(())
}
