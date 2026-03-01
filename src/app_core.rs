use crate::audio::{player::AudioPlayer, recorder::AudioRecorder};
use crate::config::Config;
use crate::input::{evdev_hook::EvdevHook, uinput_sim::UinputSim, InputEvent};
use crate::network::{asr_http::AsrHttpClient, llm_sse::LlmClient};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub struct AppCore;

impl AppCore {
    pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
        let config = Arc::new(config);

        // Initialize Input Hook
        let (input_tx, mut input_rx) = mpsc::channel(100);
        let hook = EvdevHook::new(&config.hotkeys)?;
        hook.spawn_listener(input_tx);

        // State variables
        let mut recording_stop_tx: Option<tokio::sync::oneshot::Sender<()>> = None;
        let mut recording_data_rx: Option<tokio::sync::oneshot::Receiver<Vec<u8>>> = None;
        let mut current_mode: Option<&str> = None;
        let mut pending_paste = false;

        // Persistent clipboard — on Linux the clipboard owner must stay alive
        // to serve paste requests. We keep it across the event loop.
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(cb) => Some(cb),
            Err(e) => {
                warn!("Failed to initialize clipboard: {}", e);
                None
            }
        };

        info!("MouthWrite Daemon is running. Waiting for hotkeys...");

        // Main Event Loop
        while let Some(event) = input_rx.recv().await {
            match event {
                InputEvent::DirectModePressed | InputEvent::TranslateModePressed => {
                    let requested_mode = if event == InputEvent::DirectModePressed {
                        "Direct"
                    } else {
                        "Translate"
                    };

                    if recording_stop_tx.is_some() {
                        // Allow upgrading Direct -> Translate when overlap combo is formed
                        // while user is still holding the hotkey.
                        if current_mode == Some("Direct") && requested_mode == "Translate" {
                            current_mode = Some("Translate");
                            info!("Mode switched to Translate while recording.");
                        }
                        continue; // Already recording
                    }

                    current_mode = Some(requested_mode);

                    info!("Mode activated: {:?}", current_mode);

                    AudioPlayer::play_start_sound();

                    // Start Audio Recording (buffered mode)
                    match AudioRecorder::start_recording() {
                        Ok((stop_tx, data_rx)) => {
                            recording_stop_tx = Some(stop_tx);
                            recording_data_rx = Some(data_rx);
                        }
                        Err(e) => {
                            error!("Failed to start recording: {}", e);
                        }
                    }
                }
                InputEvent::DirectModeReleased | InputEvent::TranslateModeReleased => {
                    let release_mode = if event == InputEvent::DirectModeReleased {
                        "Direct"
                    } else {
                        "Translate"
                    };

                    // Ignore release events that do not match the active mode.
                    if current_mode != Some(release_mode) {
                        continue;
                    }

                    if let Some(stop_tx) = recording_stop_tx.take() {
                        let _ = stop_tx.send(()); // Stops the recording

                        AudioPlayer::play_end_sound();
                        info!("Recording ended, starting ASR...");

                        // Retrieve the complete PCM recording
                        let pcm_data = if let Some(data_rx) = recording_data_rx.take() {
                            match data_rx.await {
                                Ok(data) => data,
                                Err(_) => {
                                    error!("Failed to get recording data");
                                    continue;
                                }
                            }
                        } else {
                            warn!("No recording in progress when key released");
                            continue;
                        };

                        if pcm_data.is_empty() {
                            warn!("No speech detected");
                            continue;
                        }

                        // ASR: send complete recording to qwen3-asr-flash
                        let asr_result = match AsrHttpClient::transcribe(&config.asr, pcm_data).await {
                            Ok(text) => text,
                            Err(e) => {
                                error!("ASR failed: {}", e);
                                continue;
                            }
                        };

                        if asr_result.is_empty() {
                            warn!("ASR returned empty text");
                            continue;
                        }

                        info!("ASR text: {}", asr_result);

                        // LLM processing
                        let mode = current_mode.take().unwrap_or("Direct");
                        info!("Mode {} processing...", mode);

                        let (opt_tx, mut opt_rx) = mpsc::channel(100);
                        let config_clone = config.clone();
                        let mode_for_log = mode.to_string();
                        
                        tokio::spawn(async move {
                            if mode == "Direct" {
                                if let Err(e) = LlmClient::optimize_text_stream(&config_clone.llm, asr_result, opt_tx).await {
                                    error!("LLM Error: {}", e);
                                }
                            } else {
                                if let Err(e) = LlmClient::translate_text_stream(&config_clone.translation, asr_result, opt_tx).await {
                                    error!("MT Error: {}", e);
                                }
                            }
                            info!("{} stream worker finished.", mode_for_log);
                        });
                        
                        // Read stream and copy to clipboard
                        let mut final_result = String::new();
                        while let Some(chunk) = opt_rx.recv().await {
                            final_result.push_str(&chunk);
                        }

                        // Set clipboard (persistent — keeps content alive for paste)
                        if !final_result.is_empty() {
                            if let Some(ref mut cb) = clipboard {
                                if let Err(e) = cb.set_text(&final_result) {
                                    warn!("Failed to set clipboard: {}", e);
                                }
                            }
                            AudioPlayer::play_click_prompt_sound();
                            info!("Result ready. Please click target location to paste.");
                            pending_paste = true;
                        } else {
                            warn!("LLM/MT produced empty result");
                        }
                    }
                }
                InputEvent::MouseLeftClicked => {
                    if pending_paste {
                        // Brief delay to let the target window process the focus event
                        // from the mouse click before we inject keystrokes
                        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

                        if let Err(e) = UinputSim::simulate_paste(&config.hotkeys.paste_shortcut) {
                            error!("Paste simulation failed: {}", e);
                        }

                        // Brief delay to let paste complete
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        pending_paste = false;
                    }
                }
            }
        }

        warn!("Input event channel closed; main event loop exiting.");

        Ok(())
    }
}
