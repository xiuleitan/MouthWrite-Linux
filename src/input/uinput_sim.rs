use crate::error::AppError;
use std::sync::Mutex;
use std::time::Duration;
use uinput::event::keyboard::Key;
use tracing::{debug, info};

static UINPUT_DEVICE: Mutex<Option<uinput::Device>> = Mutex::new(None);

pub struct UinputSim;

impl UinputSim {
    /// Initialize the persistent virtual keyboard device.
    fn ensure_device(device_lock: &mut Option<uinput::Device>) -> Result<(), AppError> {
        if device_lock.is_none() {
            info!("Creating persistent uinput virtual keyboard...");
            let device = uinput::default()
                .map_err(|e| AppError::InputError(format!("uinput default device failed: {}", e)))?
                .name("mouthwrite-virtual-keyboard")
                .map_err(|e| AppError::InputError(format!("uinput name failed: {}", e)))?
                .event(uinput::event::Keyboard::All)
                .map_err(|e| AppError::InputError(format!("uinput event set failed: {}", e)))?
                .create()
                .map_err(|e| AppError::InputError(format!("uinput create failed: {}", e)))?;

            // Wait for OS to recognize the virtual device
            std::thread::sleep(Duration::from_millis(100));
            *device_lock = Some(device);
        }
        Ok(())
    }

    /// Parse a key string (e.g., "KEY_LEFTSHIFT") into a uinput Key.
    fn parse_uinput_key(key_str: &str) -> Result<Key, AppError> {
        match key_str.trim() {
            "KEY_LEFTSHIFT" => Ok(Key::LeftShift),
            "KEY_RIGHTSHIFT" => Ok(Key::RightShift),
            "KEY_LEFTCTRL" => Ok(Key::LeftControl),
            "KEY_RIGHTCTRL" => Ok(Key::RightControl),
            "KEY_LEFTALT" => Ok(Key::LeftAlt),
            "KEY_RIGHTALT" => Ok(Key::RightAlt),
            "KEY_LEFTMETA" => Ok(Key::LeftMeta),
            "KEY_RIGHTMETA" => Ok(Key::RightMeta),
            "KEY_INSERT" => Ok(Key::Insert),
            "KEY_V" => Ok(Key::V),
            "KEY_SPACE" => Ok(Key::Space),
            "KEY_ENTER" => Ok(Key::Enter),
            _ => Err(AppError::ConfigError(format!("Unsupported paste key: {}", key_str))),
        }
    }

    /// Simulates a configurable key combination for pasting.
    /// `shortcut` format: "KEY_LEFTSHIFT+KEY_INSERT" or "KEY_LEFTCTRL+KEY_V"
    pub fn simulate_paste(shortcut: &str) -> Result<(), AppError> {
        debug!("Simulating paste shortcut: {}", shortcut);

        let keys: Vec<Key> = shortcut
            .split('+')
            .map(|k| Self::parse_uinput_key(k))
            .collect::<Result<Vec<_>, _>>()?;

        if keys.is_empty() {
            return Err(AppError::ConfigError("Empty paste shortcut".into()));
        }

        let mut lock = UINPUT_DEVICE.lock()
            .map_err(|e| AppError::InputError(format!("Failed to lock uinput device: {}", e)))?;

        Self::ensure_device(&mut lock)?;

        let device = lock.as_mut().unwrap();

        // Press all keys in order
        for key in &keys {
            device.send(*key, 1).map_err(|e| AppError::InputError(e.to_string()))?;
        }

        device.synchronize().map_err(|e| AppError::InputError(e.to_string()))?;
        std::thread::sleep(Duration::from_millis(20));

        // Release all keys in reverse order
        for key in keys.iter().rev() {
            device.send(*key, 0).map_err(|e| AppError::InputError(e.to_string()))?;
        }

        device.synchronize().map_err(|e| AppError::InputError(e.to_string()))?;

        debug!("Paste simulated.");
        Ok(())
    }
}
