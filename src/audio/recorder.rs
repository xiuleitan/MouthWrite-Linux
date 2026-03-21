use crate::error::AppError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tracing::{error, info};

/// Recorder states
const STATE_IDLE: u8 = 0;
const STATE_RECORDING: u8 = 1;

/// Shared inner state between the audio callback thread and the main logic.
struct RecorderInner {
    /// Ring buffer holding the most recent `pre_roll_ms` of 16kHz mono PCM.
    ring_buffer: Mutex<VecDeque<u8>>,
    /// Buffer that accumulates PCM data during an active recording session.
    recording_buffer: Mutex<Vec<u8>>,
    /// 0 = Idle (writing to ring buffer), 1 = Recording.
    state: AtomicU8,
    /// Maximum ring buffer size in bytes.
    ring_buffer_capacity: usize,
}

/// A persistent audio recorder that keeps the cpal input stream always running.
///
/// During idle periods the audio callback writes into a fixed-size ring buffer
/// (keeping the most recent `pre_roll_ms` of audio).  When a recording session
/// starts the ring buffer contents are flushed into the recording buffer as
/// "pre-roll" data, and subsequent audio callback data goes directly into the
/// recording buffer.  When the session stops the complete PCM data (pre-roll +
/// recording) is returned via a oneshot channel.
pub struct PersistentRecorder {
    inner: Arc<RecorderInner>,
    /// Sender half for returning PCM data when a recording session ends.
    data_tx: Mutex<Option<oneshot::Sender<Vec<u8>>>>,
    /// cpal stream handle — kept alive for the lifetime of the recorder.
    _stream_thread: std::thread::JoinHandle<()>,
    /// Receives the cpal::Stream from the background thread so we can confirm
    /// that the stream is running.
    stream_ready_rx: Mutex<Option<oneshot::Receiver<()>>>,
}

impl PersistentRecorder {
    /// Creates a new `PersistentRecorder` and starts the cpal input stream in a
    /// dedicated background thread.
    ///
    /// `pre_roll_ms` controls how many milliseconds of audio are retained in
    /// the ring buffer for pre-roll capture.
    pub fn new(pre_roll_ms: u64) -> Result<Self, AppError> {
        // 16kHz, mono, 16-bit = 2 bytes per sample = 32000 bytes/sec
        let ring_buffer_capacity = (pre_roll_ms as usize) * 32; // 32 bytes per ms

        let inner = Arc::new(RecorderInner {
            ring_buffer: Mutex::new(VecDeque::with_capacity(ring_buffer_capacity)),
            recording_buffer: Mutex::new(Vec::new()),
            state: AtomicU8::new(STATE_IDLE),
            ring_buffer_capacity,
        });

        let inner_clone = inner.clone();
        let (stream_ready_tx, stream_ready_rx) = oneshot::channel::<()>();

        let handle = std::thread::spawn(move || {
            Self::audio_thread(inner_clone, stream_ready_tx);
        });

        Ok(Self {
            inner,
            data_tx: Mutex::new(None),
            _stream_thread: handle,
            stream_ready_rx: Mutex::new(Some(stream_ready_rx)),
        })
    }

    /// Waits (with timeout) until the audio stream is confirmed running.
    /// Should be called once during startup.
    pub async fn wait_ready(&self) -> Result<(), AppError> {
        let rx = {
            let mut guard = self.stream_ready_rx.lock().unwrap();
            guard.take()
        };
        if let Some(rx) = rx {
            match tokio::time::timeout(tokio::time::Duration::from_millis(3000), rx).await {
                Ok(Ok(())) => {
                    info!("PersistentRecorder: audio stream is running.");
                    Ok(())
                }
                Ok(Err(_)) => Err(AppError::AudioError(
                    "Audio stream thread exited before signalling ready".into(),
                )),
                Err(_) => Err(AppError::AudioError(
                    "Timed out waiting for audio stream to start".into(),
                )),
            }
        } else {
            // Already waited once — stream is running.
            Ok(())
        }
    }

    /// Starts a recording session.
    ///
    /// The ring buffer contents (pre-roll) are flushed into the recording
    /// buffer, and subsequent audio data is accumulated until
    /// [`stop_recording`] is called.
    ///
    /// Returns a `Receiver` that will eventually contain the complete PCM data.
    pub fn start_recording(&self) -> oneshot::Receiver<Vec<u8>> {
        let (tx, rx) = oneshot::channel();

        // Prepare the recording buffer with pre-roll data.
        {
            let mut ring = self.inner.ring_buffer.lock().unwrap();
            let mut rec = self.inner.recording_buffer.lock().unwrap();
            rec.clear();
            // Flush ring buffer contents as pre-roll.
            let pre_roll: Vec<u8> = ring.drain(..).collect();
            let pre_roll_len = pre_roll.len();
            rec.extend(pre_roll);
            info!(
                "Recording started with {:.0}ms pre-roll ({} bytes)",
                pre_roll_len as f64 / 32.0,
                pre_roll_len
            );
        }

        // Store the sender.
        {
            let mut tx_guard = self.data_tx.lock().unwrap();
            *tx_guard = Some(tx);
        }

        // Switch state to recording.
        self.inner.state.store(STATE_RECORDING, Ordering::Release);

        rx
    }

    /// Stops the current recording session and sends the accumulated PCM data
    /// through the channel returned by [`start_recording`].
    pub fn stop_recording(&self) {
        // Switch state back to idle.
        self.inner.state.store(STATE_IDLE, Ordering::Release);

        // Extract the accumulated PCM data and send it.
        let pcm_data = {
            let mut rec = self.inner.recording_buffer.lock().unwrap();
            std::mem::take(&mut *rec)
        };

        info!(
            "Recording stopped. Total PCM data: {} bytes ({:.1}s)",
            pcm_data.len(),
            pcm_data.len() as f64 / (16000.0 * 2.0)
        );

        let tx = {
            let mut tx_guard = self.data_tx.lock().unwrap();
            tx_guard.take()
        };
        if let Some(tx) = tx {
            let _ = tx.send(pcm_data);
        }
    }

    /// Background thread: opens the cpal input stream and keeps it running
    /// forever.
    fn audio_thread(inner: Arc<RecorderInner>, ready_tx: oneshot::Sender<()>) {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                error!("No input device available");
                return;
            }
        };

        info!(
            "PersistentRecorder using input device: {}",
            device.name().unwrap_or_else(|_| "unknown".to_string())
        );

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                error!("Config error: {}", e);
                return;
            }
        };

        let stream_config: cpal::StreamConfig = config.clone().into();
        let channels = stream_config.channels;
        let sample_rate = stream_config.sample_rate.0;

        let mut ready_tx = Some(ready_tx);

        let stream_result = match config.sample_format() {
            cpal::SampleFormat::F32 => Self::build_persistent_stream::<f32>(
                &device,
                &stream_config,
                inner.clone(),
                channels,
                sample_rate,
                &mut ready_tx,
            ),
            cpal::SampleFormat::I16 => Self::build_persistent_stream::<i16>(
                &device,
                &stream_config,
                inner.clone(),
                channels,
                sample_rate,
                &mut ready_tx,
            ),
            cpal::SampleFormat::U16 => Self::build_persistent_stream::<u16>(
                &device,
                &stream_config,
                inner.clone(),
                channels,
                sample_rate,
                &mut ready_tx,
            ),
            format => {
                error!("Unsupported sample format: {:?}", format);
                return;
            }
        };

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    error!("Failed to play stream: {}", e);
                    return;
                }
                info!("PersistentRecorder: cpal stream started successfully.");
                // Signal ready if the first callback hasn't fired yet.
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(());
                }
                // Park this thread forever — the stream must stay alive.
                loop {
                    std::thread::park();
                }
            }
            Err(e) => {
                error!("Failed to build persistent stream: {:?}", e);
            }
        }
    }

    fn build_persistent_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        inner: Arc<RecorderInner>,
        channels: u16,
        sample_rate: u32,
        ready_tx: &mut Option<oneshot::Sender<()>>,
    ) -> Result<cpal::Stream, AppError>
    where
        T: cpal::Sample + Send + Sync + 'static + cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let err_fn = |err| error!("An error occurred on the input audio stream: {}", err);
        let mut ready_tx = ready_tx.take();

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    // Signal readiness on the very first callback.
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(());
                    }

                    let pcm_16khz = match Self::convert_to_16k_mono_pcm(data, channels, sample_rate)
                    {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    let state = inner.state.load(Ordering::Acquire);
                    if state == STATE_RECORDING {
                        // Append directly to the recording buffer.
                        let mut rec = inner.recording_buffer.lock().unwrap();
                        rec.extend_from_slice(&pcm_16khz);
                    } else {
                        // Write into the ring buffer, evicting old data.
                        let mut ring = inner.ring_buffer.lock().unwrap();
                        ring.extend(pcm_16khz.iter());
                        while ring.len() > inner.ring_buffer_capacity {
                            ring.pop_front();
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| {
                AppError::AudioError(format!("Failed to build input stream: {}", e))
            })?;

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
            let mono_i16 =
                (mono_f32 * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;

            // Push little endian bytes
            result.extend_from_slice(&mono_i16.to_le_bytes());

            current_pos += ratio;
        }

        Ok(result)
    }
}
