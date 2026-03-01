mod app_core;
mod audio;
mod config;
mod error;
mod input;
mod logging;
mod network;

use clap::{Parser, Subcommand};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "MouthWrite - Linux 系统级全局语音输入工具",
    long_about = "MouthWrite 是一个基于 Rust 的 Linux 语音输入守护进程。\n\n\
        按住快捷键说话，松开后自动完成：语音识别 → 文本优化/翻译 → 剪贴板复制 → 点击粘贴。\n\n\
        首次运行会在 ~/.config/mouthwrite/config.toml 生成默认配置文件。\n\
        请编辑该文件填写 API Key 并根据需要调整快捷键、模型等设置。\n\n\
        权限要求：\n  \
        - 用户需加入 input 组：sudo usermod -aG input $USER\n  \
        - 需要 uinput 权限：创建 /etc/udev/rules.d/99-mouthwrite-uinput.rules\n    \
          内容：KERNEL==\"uinput\", GROUP=\"input\", MODE=\"0660\"\n    \
          然后执行：sudo udevadm control --reload-rules && sudo udevadm trigger"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 启动 MouthWrite 守护进程（默认行为，可省略此子命令）
    Start,
    /// 检查并打印当前配置文件内容，验证配置是否正确
    CheckConfig,
    /// 显示配置文件路径
    ConfigPath,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ignore SIGPIPE to prevent broken pipe signals from crashing the daemon
    // (e.g., Wayland display disconnection)
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN); }

    let _log_guard = logging::init_logging();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Start) {
        Commands::Start => {
            info!("Starting MouthWrite init sequence...");
            let config = config::Config::load_or_create();
            
            if let Err(e) = app_core::AppCore::run(config).await {
                error!("MouthWrite encountered a fatal error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::CheckConfig => {
            let config = config::Config::load_or_create();
            println!("✅ 配置加载成功！");
            println!("配置文件路径：{}", config::Config::config_path().display());
            println!();
            println!("{:#?}", config);
        }
        Commands::ConfigPath => {
            println!("{}", config::Config::config_path().display());
        }
    }

    Ok(())
}
