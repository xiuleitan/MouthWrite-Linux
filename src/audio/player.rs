use crate::error::AppError;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::io::Cursor;
use tracing::warn;

// Embed the audio files directly into the binary
const START_MP3: &[u8] = include_bytes!("../../assets/start.mp3");
const END_MP3: &[u8] = include_bytes!("../../assets/end.mp3");
const PLEASE_CLICK_MP3: &[u8] = include_bytes!("../../assets/please_click.mp3");

/// Audio player that pre-initialises the output stream once and reuses it for
/// all sound playback, eliminating per-play OutputStream::try_default() cost.
pub struct AudioPlayer {
    /// Keep the stream alive — dropping it silences all sinks.
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

impl AudioPlayer {
    /// Creates a new `AudioPlayer`, opening the default output device once.
    pub fn new() -> Result<Self, AppError> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|e| AppError::AudioError(format!("Failed to get output stream: {}", e)))?;
        Ok(Self {
            _stream: stream,
            handle,
        })
    }

    /// Plays the 'start' recording cue without blocking the caller.
    pub fn play_start_sound(&self) {
        self.play_sound_async(START_MP3, "start");
    }

    /// Plays the 'end' recording cue without blocking the caller.
    pub fn play_end_sound(&self) {
        self.play_sound_async(END_MP3, "end");
    }

    /// Plays the "please click to paste" prompt without blocking the caller.
    pub fn play_click_prompt_sound(&self) {
        self.play_sound_async(PLEASE_CLICK_MP3, "click_prompt");
    }

    fn play_sound_async(&self, audio_data: &'static [u8], label: &'static str) {
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to create audio sink for {}: {}", label, e);
                return;
            }
        };

        let cursor = Cursor::new(audio_data);
        let decoder = match Decoder::new(cursor) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to decode {} audio: {}", label, e);
                return;
            }
        };

        sink.append(decoder);
        // Detach — the sink will play to completion and then drop itself.
        sink.detach();
    }
}
