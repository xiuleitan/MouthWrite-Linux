use crate::config::AsrConfig;
use crate::error::AppError;
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use tracing::{debug, error, info};

pub struct AsrHttpClient;

impl AsrHttpClient {
    /// Transcribes complete PCM audio data (16kHz, mono, 16-bit) using qwen3-asr-flash
    /// via the OpenAI-compatible chat completions API.
    pub async fn transcribe(
        config: &AsrConfig,
        pcm_data: Vec<u8>,
    ) -> Result<String, AppError> {
        if pcm_data.is_empty() {
            return Err(AppError::InternalError("No audio data to transcribe".into()));
        }

        let duration_secs = pcm_data.len() as f64 / (16000.0 * 2.0); // 16kHz, 16-bit = 2 bytes/sample
        info!("Transcribing {:.1}s of audio ({} bytes PCM)", duration_secs, pcm_data.len());

        // 1. Encode PCM as WAV
        let wav_data = Self::pcm_to_wav(&pcm_data);

        // 2. Base64 encode
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_data);
        let audio_data_uri = format!("data:audio/wav;base64,{}", b64);

        debug!("WAV size: {} bytes, base64 length: {}", wav_data.len(), b64.len());

        // 3. Build OpenAI-compatible request
        let payload = json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": audio_data_uri,
                                "format": "wav"
                            }
                        }
                    ]
                }
            ]
        });

        // 4. Send HTTP request
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.api_key))
                .map_err(|e| AppError::ConfigError(format!("Invalid API key: {}", e)))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        info!("Sending ASR request to: {}", config.api_url);

        let res = client
            .post(&config.api_url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            error!("ASR request failed with {}: {}", status, body);
            return Err(AppError::HttpError(status.as_u16(), body));
        }

        // 5. Parse response
        let body = res.text().await?;
        debug!("ASR response: {}", body);

        let json_res: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AppError::InternalError(format!("Failed to parse ASR response: {}", e)))?;

        let text = json_res["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if text.is_empty() {
            error!("ASR returned empty text. Full response: {}", body);
            return Err(AppError::InternalError("ASR returned no transcription".into()));
        }

        info!("ASR result: {}", text);
        Ok(text)
    }

    /// Wraps raw PCM data (16kHz, mono, 16-bit LE) with a standard 44-byte WAV header.
    fn pcm_to_wav(pcm_data: &[u8]) -> Vec<u8> {
        let data_len = pcm_data.len() as u32;
        let file_len = data_len + 36; // total file size minus 8 bytes for RIFF header
        let sample_rate: u32 = 16000;
        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate: u32 = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
        let block_align: u16 = num_channels * bits_per_sample / 8;

        let mut wav = Vec::with_capacity(44 + pcm_data.len());

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_len.to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt sub-chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
        wav.extend_from_slice(&1u16.to_le_bytes());  // audio format (PCM = 1)
        wav.extend_from_slice(&num_channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());

        // data sub-chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(pcm_data);

        wav
    }
}
