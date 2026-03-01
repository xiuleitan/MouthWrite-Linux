use std::path::PathBuf;
use tracing_appender::rolling;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, EnvFilter, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logging() -> WorkerGuard {
    let log_dir = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("mouthwrite");
        
    std::fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create log directory: {}", e);
    });

    // Daily rotating log file in ~/.local/state/mouthwrite/app.log
    let file_appender = rolling::daily(log_dir, "app.log");
    
    // Non-blocking writer for tracing
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    
    // We want to log at least INFO to the file, and filter via RUST_LOG env variable if present
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,mouthwrite_linux=debug"));

    let file_layer = fmt::Layer::new()
        .with_writer(non_blocking)
        .with_ansi(false); // No colors in log file
        
    let stdout_layer = fmt::Layer::new()
        .with_writer(std::io::stdout);

    Registry::default()
        .with(env_filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}
