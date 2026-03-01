use crate::config::HotkeysConfig;
use crate::error::AppError;
use crate::input::InputEvent;
use evdev::Key;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub struct EvdevHook {
    direct_keys: Vec<Key>,
    translate_keys: Vec<Key>,
    active_keys: HashSet<Key>,
    direct_held: bool,
    translate_held: bool,
}

impl EvdevHook {
    pub fn new(config: &HotkeysConfig) -> Result<Self, AppError> {
        let direct_keys = Self::parse_keys(&config.direct_mode)?;
        let translate_keys = Self::parse_keys(&config.translate_mode)?;

        Ok(Self {
            direct_keys,
            translate_keys,
            active_keys: HashSet::new(),
            direct_held: false,
            translate_held: false,
        })
    }

    fn parse_keys(key_string: &str) -> Result<Vec<Key>, AppError> {
        let mut keys = Vec::new();
        for k_str in key_string.split('+') {
            let k_str = k_str.trim();
            // Assuming the config provides strings like "KEY_RIGHTALT" or "KEY_RIGHTMETA"
            // We map these strings to `evdev::Key` dynamically or via a large match.
            // For brevity, we implement a simple lookup for common keys.
            let key = match k_str {
                "KEY_RIGHTALT" => Key::KEY_RIGHTALT,
                "KEY_LEFTALT" => Key::KEY_LEFTALT,
                "KEY_RIGHTMETA" => Key::KEY_RIGHTMETA,
                "KEY_LEFTMETA" => Key::KEY_LEFTMETA,
                "KEY_SPACE" => Key::KEY_SPACE,
                "KEY_RIGHTCTRL" => Key::KEY_RIGHTCTRL,
                "KEY_LEFTCTRL" => Key::KEY_LEFTCTRL,
                "KEY_LEFTSHIFT" => Key::KEY_LEFTSHIFT,
                "KEY_RIGHTSHIFT" => Key::KEY_RIGHTSHIFT,
                // Add more keys as needed by user config
                _ => return Err(AppError::ConfigError(format!("Unsupported or invalid key: {}", k_str))),
            };
            keys.push(key);
        }
        Ok(keys)
    }

    /// Spawns a background tokio task that listens to all input devices
    /// and emits `InputEvent`s to the provided sender.
    pub fn spawn_listener(mut self, tx: mpsc::Sender<InputEvent>) {
        tokio::spawn(async move {
            loop {
                // Find all valid devices
                let devices = evdev::enumerate().map(|t| t.1).collect::<Vec<_>>();
                
                if devices.is_empty() {
                    warn!("No evdev devices found. Do you have permissions? (e.g. member of `input` group)");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    continue;
                }

                let (internal_tx, mut internal_rx) = mpsc::channel(100);

                let mut task_handles = Vec::new();
                for mut device in devices {
                    let tx_clone = internal_tx.clone();
                    let name = device.name().unwrap_or("Unknown").to_string();
                    
                    // Only listen to devices that might emit keys or mouse clicks
                    if !device.supported_events().contains(evdev::EventType::KEY) {
                        continue;
                    }

                    let handle = tokio::task::spawn_blocking(move || {
                         loop {
                             match device.fetch_events() {
                                 Ok(events) => {
                                     for ev in events {
                                         if tx_clone.blocking_send((name.clone(), ev)).is_err() {
                                             return;
                                         }
                                     }
                                 }
                                 Err(_) => {
                                     // Device disconnected or error
                                     return;
                                 }
                             }
                         }
                    });
                    task_handles.push(handle);
                }

                drop(internal_tx); // drop the original so the receiver can close when all tasks die

                while let Some((_dev_name, ev)) = internal_rx.recv().await {
                    if ev.event_type() == evdev::EventType::KEY {
                        let key = Key::new(ev.code());
                        let value = ev.value(); // 0 = release, 1 = press, 2 = repeat

                        // Handle mouse left click specifically for pasting
                        if key == Key::BTN_LEFT {
                            if value == 0 { // Release
                                let _ = tx.send(InputEvent::MouseLeftClicked).await;
                            }
                            continue;
                        }

                        // Ignore non-keyboard button events (e.g. BTN_* from mouse/touchpad).
                        // Those can remain active and break strict hotkey matching.
                        if ev.code() >= 0x100 {
                            continue;
                        }

                        // Maintain active keys state.
                        // Treat left/right Ctrl as equivalent so KEY_RIGHTCTRL hotkeys
                        // still work on systems that report KEY_LEFTCTRL (or vice versa).
                        if value == 1 {
                            self.active_keys.insert(key);
                            if key == Key::KEY_LEFTCTRL || key == Key::KEY_RIGHTCTRL {
                                self.active_keys.insert(Key::KEY_LEFTCTRL);
                                self.active_keys.insert(Key::KEY_RIGHTCTRL);
                            }
                        } else if value == 0 {
                            self.active_keys.remove(&key);
                            if key == Key::KEY_LEFTCTRL || key == Key::KEY_RIGHTCTRL {
                                self.active_keys.remove(&Key::KEY_LEFTCTRL);
                                self.active_keys.remove(&Key::KEY_RIGHTCTRL);
                            }
                        }

                        // Evaluate combinations
                        self.evaluate_state(&tx).await;
                    }
                }

                // Bug 11 fix: Abort any remaining blocking tasks before rescanning
                for handle in task_handles {
                    handle.abort();
                }

                // Device set changed. Reset key state to avoid stale pressed keys
                // from locking the hotkey state machine after reconnect/rescan.
                self.active_keys.clear();
                self.direct_held = false;
                self.translate_held = false;

                // If we exit the loop, devices changed (e.g., unplugged). Sleep briefly and rescan.
                debug!("Evdev listener looping to rescan devices...");
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });
    }

    async fn evaluate_state(&mut self, tx: &mpsc::Sender<InputEvent>) {
        // Use subset matching so one stale extra key does not permanently block hotkeys.
        // Keep translate higher priority to avoid triggering both when combos overlap.
        let translate_match = self.are_keys_active(&self.translate_keys);
        let direct_match = self.are_keys_active(&self.direct_keys) && !translate_match;

        // State machine for translate mode.
        // Process translate first so overlap combos (e.g. RAlt + RCtrl) can preempt direct.
        match (self.translate_held, translate_match) {
            (false, true) => {
                self.translate_held = true;
                debug!("Emit TranslateModePressed");
                let _ = tx.send(InputEvent::TranslateModePressed).await;
            }
            (true, false) => {
                self.translate_held = false;
                debug!("Emit TranslateModeReleased");
                let _ = tx.send(InputEvent::TranslateModeReleased).await;
            }
            _ => {}
        }

        // State machine for direct mode
        match (self.direct_held, direct_match) {
            (false, true) => {
                self.direct_held = true;
                debug!("Emit DirectModePressed");
                let _ = tx.send(InputEvent::DirectModePressed).await;
            }
            (true, false) => {
                self.direct_held = false;
                debug!("Emit DirectModeReleased");
                let _ = tx.send(InputEvent::DirectModeReleased).await;
            }
            _ => {}
        }
    }

    fn are_keys_active(&self, target_keys: &[Key]) -> bool {
        if target_keys.is_empty() {
            return false;
        }
        for k in target_keys {
            if !self.active_keys.contains(k) {
                return false;
            }
        }
        true
    }
}
