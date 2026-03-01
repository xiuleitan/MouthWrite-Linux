use crate::error::AppError;
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

pub struct AudioRecorder;

impl AudioRecorder {
    /// Starts recording audio from the default input device.
    /// Returns a stop sender and a receiver that yields the complete PCM data
    /// (16kHz, mono, 16-bit LE) when recording stops.
    pub fn start_recording() -> Result<(tokio::sync::oneshot::Sender<()>, tokio::sync::oneshot::Receiver<Vec<u8>>), AppError> {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let (data_tx, data_rx) = tokio::sync::oneshot::channel::<Vec<u8>>();

        // Shared buffer to accumulate PCM data
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let buffer_clone = buffer.clone();

        // We use std::thread::spawn so we can safely block and keep `stream` thread-local.
        std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => { error!("No input device available"); return; }
            };
            
            info!("Using input device: {}", device.name().unwrap_or_else(|_| "unknown".to_string()));

            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => { error!("Config error: {}", e); return; }
            };

            let stream_config: cpal::StreamConfig = config.clone().into();
            let channels = stream_config.channels;
            let sample_rate = stream_config.sample_rate.0;

            let stream_result = match config.sample_format() {
                cpal::SampleFormat::F32 => Self::build_stream::<f32>(&device, &stream_config, buffer_clone.clone(), channels, sample_rate),
                cpal::SampleFormat::I16 => Self::build_stream::<i16>(&device, &stream_config, buffer_clone.clone(), channels, sample_rate),
                cpal::SampleFormat::U16 => Self::build_stream::<u16>(&device, &stream_config, buffer_clone.clone(), channels, sample_rate),
                format => {
                    error!("Unsupported sample format: {:?}", format);
                    return;
                }
            };

            match stream_result {
                Ok(stream) => {
                    if let Err(e) = cpal::traits::StreamTrait::play(&stream) {
                        error!("Failed to play stream: {}", e);
                        return;
                    }
                    // Wait until requested to stop
                    let _ = stop_rx.blocking_recv();
                    drop(stream);
                    info!("Audio recording stopped.");

                    // Send the accumulated PCM data
                    let pcm_data = {
                        let buf = buffer_clone.lock().unwrap();
                        buf.clone()
                    };
                    info!("Total recorded PCM data: {} bytes ({:.1}s)", 
                        pcm_data.len(), 
                        pcm_data.len() as f64 / (16000.0 * 2.0));
                    let _ = data_tx.send(pcm_data);
                }
                Err(e) => {
                    error!("Failed to build stream: {:?}", e);
                }
            }
        });

        Ok((stop_tx, data_rx))
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<Vec<u8>>>,
        channels: u16,
        sample_rate: u32,
    ) -> Result<cpal::Stream, AppError>
    where
        T: cpal::Sample + Send + Sync + 'static + cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let err_fn = |err| error!("An error occurred on the input audio stream: {}", err);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if let Ok(pcm_16khz) = Self::convert_to_16k_mono_pcm(data, channels, sample_rate) {
                        let mut buf = buffer.lock().unwrap();
                        buf.extend_from_slice(&pcm_16khz);
                    }
                },
                err_fn,
                None, // Provide None for timeout
            )
            .map_err(|e| AppError::AudioError(format!("Failed to build input stream: {}", e)))?;

        Ok(stream)
    }

    /// Converts raw audio samples to 16kHz, mono, 16-bit PCM.
    fn convert_to_16k_mono_pcm<T>(
        data: &[T],
        channels: u16,
        source_sample_rate: u32,
    ) -> Result<Vec<u8>, AppError>
    where
        T: cpal::Sample,
        f32: cpal::FromSample<T>,
    {
        let mut result = Vec::new();
        let ratio = source_sample_rate as f32 / 16000.0;
        let mut current_pos: f32 = 0.0;

        while (current_pos as usize) * (channels as usize) < data.len() {
            let base_idx = (current_pos as usize) * (channels as usize);
            
            // Average channels to get mono
            let mut sum = 0.0;
            for c in 0..channels {
                if base_idx + (c as usize) < data.len() {
                    let sample: f32 = cpal::FromSample::from_sample_(data[base_idx + c as usize]);
                    sum += sample;
                }
            }
            let mono_f32 = sum / channels as f32;
            
            // Convert to i16
            let mono_i16 = (mono_f32 * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            
            // Push little endian bytes
            result.extend_from_slice(&mono_i16.to_le_bytes());
            
            current_pos += ratio;
        }

        Ok(result)
    }
}
