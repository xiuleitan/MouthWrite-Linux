use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub hotkeys: HotkeysConfig,
    pub asr: AsrConfig,
    pub llm: LlmConfig,
    pub translation: TranslationConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HotkeysConfig {
    pub direct_mode: String,
    pub translate_mode: String,
    pub paste_shortcut: String,
    #[serde(default = "default_start_cue_delay_ms")]
    pub start_cue_delay_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AsrConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    #[serde(default = "default_false")]
    pub enable_thinking: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TranslationConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub source_lang: String,
    pub target_lang: String,
    #[serde(default = "default_false")]
    pub enable_thinking: bool,
}

fn default_false() -> bool {
    false
}

fn default_start_cue_delay_ms() -> u64 {
    800
}

impl Config {
    pub fn load_or_create() -> Self {
        let config_dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("mouthwrite");
            
        if !config_dir.exists() {
            if let Err(e) = fs::create_dir_all(&config_dir) {
                warn!("Failed to create config directory {:?}: {}", config_dir, e);
            }
        }

        let config_file = config_dir.join("config.toml");

        if !config_file.exists() {
            info!("Config file not found, creating default at {:?}", config_file);
            let default_config = include_str!("../config_template.toml");
            if let Err(e) = fs::write(&config_file, default_config) {
                warn!("Failed to write default config to {:?}: {}", config_file, e);
            }
            // Parse from the default template we just embedded
            return toml::from_str(default_config).expect("Default config template is invalid");
        }

        let content = fs::read_to_string(&config_file)
            .unwrap_or_else(|e| panic!("Failed to read config file {:?}: {}", config_file, e));

        toml::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse config file {:?}: {}", config_file, e))
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("mouthwrite")
            .join("config.toml")
    }
}
