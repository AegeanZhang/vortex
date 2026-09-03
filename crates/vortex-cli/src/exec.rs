use std::env::var;
use std::error::Error;
use std::result::Result;

pub async fn execute(_task: String) -> Result<(), Box<dyn Error + Send + Sync>> {

    let _api_key = var("DEEPSEEK_API_KEY")?;

    Ok(())
}
