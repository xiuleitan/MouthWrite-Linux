use crate::error::AppError;
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use tracing::warn;

// Embed the audio files directly into the binary
const START_MP3: &[u8] = include_bytes!("../../assets/start.mp3");
const END_MP3: &[u8] = include_bytes!("../../assets/end.mp3");
const PLEASE_CLICK_MP3: &[u8] = include_bytes!("../../assets/please_click.mp3");

pub struct AudioPlayer;

impl AudioPlayer {
    /// Plays the 'start' recording sound blockingly (since it's very short)
    /// or asynchronously if preferred. We'll do a simple blocking play in a background thread
    /// so it doesn't block the main event loop.
    pub fn play_start_sound() {
        tokio::task::spawn_blocking(|| {
            if let Err(e) = Self::play_sound(START_MP3) {
                warn!("Failed to play start sound: {}", e);
            }
        });
    }

    /// Plays the 'end' recording sound.
    pub fn play_end_sound() {
        tokio::task::spawn_blocking(|| {
            if let Err(e) = Self::play_sound(END_MP3) {
                warn!("Failed to play end sound: {}", e);
            }
        });
    }

    /// Plays the "please click to paste" prompt sound.
    pub fn play_click_prompt_sound() {
        tokio::task::spawn_blocking(|| {
            if let Err(e) = Self::play_sound(PLEASE_CLICK_MP3) {
                warn!("Failed to play click prompt sound: {}", e);
            }
        });
    }

    fn play_sound(audio_data: &'static [u8]) -> Result<(), AppError> {
        // We get a default output stream
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| AppError::AudioError(format!("Failed to get output stream: {}", e)))?;
            
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| AppError::AudioError(format!("Failed to create audio sink: {}", e)))?;

        // Read the embedded file directly from memory
        let cursor = Cursor::new(audio_data);
        
        let decoder = Decoder::new(cursor)
            .map_err(|e| AppError::AudioError(format!("Failed to decode audio: {}", e)))?;
            
        sink.append(decoder);
        
        // Wait until the sound finishes playing (it's very short)
        sink.sleep_until_end();
        
        Ok(())
    }
}
