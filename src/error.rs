use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),



    #[error("Input event error: {0}")]
    InputError(String),

    #[error("Audio system error: {0}")]
    AudioError(String),

    #[error("HTTP error {0}: {1}")]
    HttpError(u16, String),

    #[error("Internal application error: {0}")]
    InternalError(String),
}
