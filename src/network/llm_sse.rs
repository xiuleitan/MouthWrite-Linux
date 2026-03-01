use crate::config::{LlmConfig, TranslationConfig};
use crate::error::AppError;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info};
use eventsource_stream::Eventsource;
use futures::StreamExt;

pub struct LlmClient;

impl LlmClient {
    pub async fn optimize_text_stream(
        config: &LlmConfig,
        final_asr_text: String,
        optimized_tx: mpsc::Sender<String>,
    ) -> Result<(), AppError> {
        let tagged_input = format!("<text>{}</text>", final_asr_text);

        let payload = json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": config.system_prompt},
                {"role": "user", "content": tagged_input}
            ],
            "stream": true,
            "enable_thinking": config.enable_thinking,
        });

        Self::stream_request(&config.api_url, &config.api_key, payload, optimized_tx).await
    }

    pub async fn translate_text_stream(
        config: &TranslationConfig,
        final_asr_text: String,
        translated_tx: mpsc::Sender<String>,
    ) -> Result<(), AppError> {
        let payload = json!({
            "model": config.model,
            "messages": [
                {"role": "user", "content": final_asr_text}
            ],
            "translation_options": {
                "source_lang": config.source_lang,
                "target_lang": config.target_lang
            },
            "stream": true,
            "enable_thinking": config.enable_thinking,
        });

        Self::stream_request(&config.api_url, &config.api_key, payload, translated_tx).await
    }

    async fn stream_request(
        url: &str,
        api_key: &str,
        payload: serde_json::Value,
        tx: mpsc::Sender<String>,
    ) -> Result<(), AppError> {
        info!("Starting LLM request to: {}", url);
        
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .map_err(|e| AppError::ConfigError(format!("Invalid Header Value: {}", e)))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let res = client
            .post(url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            error!("LLM Request failed with {}: {}", status, body);
            return Err(AppError::HttpError(status.as_u16(), body));
        }

        let mut stream = res.bytes_stream().eventsource();
        
        while let Some(event) = stream.next().await {
            match event {
                Ok(ev) => {
                    let data = ev.data;
                    if data == "[DONE]" {
                        info!("LLM Streaming finished naturally.");
                        break;
                    }
                    
                    if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(&data) {
                        if let Some(choices) = json_data.get("choices") {
                            if let Some(choice) = choices.get(0) {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !content.is_empty() {
                                            if let Err(_) = tx.send(content.to_string()).await {
                                                break; // receiver dropped
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    error!("Error during SSE streaming: {}", e);
                    break;
                }
            }
        }
        
        Ok(())
    }
}
