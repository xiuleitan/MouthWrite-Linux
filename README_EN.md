![Build Status](https://img.shields.io/github/actions/workflow/status/OWNER/REPO/ci.yml?branch=main&label=Build%20Status)
![Version](https://img.shields.io/github/v/release/OWNER/REPO?label=Version)
![License](https://img.shields.io/github/license/OWNER/REPO?label=License)

[![语言-中文](https://img.shields.io/badge/语言-中文-red)](README.md)
[![Language-English](https://img.shields.io/badge/Language-English-blue)](README_EN.md)

# MouthWrite Linux

MouthWrite is a system-level voice input tool for Linux.  
Hold down the hotkey to speak, and release it to automatically complete: Speech Recognition -> Text Optimization/Translation -> Copy to Clipboard -> Click target location to Auto-paste.

The project is currently a headless (CLI) version, running more stably with lower resource usage.

## Features

- Global hotkey to hold and record, release to trigger processing
- ASR (Automatic Speech Recognition) + LLM text optimization
- Switchable translation modes
- Automatically writes to the clipboard and simulates paste
- Plays a notification sound to prompt for clicking to paste

## System Requirements

- Linux (depends on `evdev` / `uinput`)
- Rust toolchain (for building)
- User must be added to the `input` group
- Configured `uinput` permissions

## Rust Environment Setup (For Beginners)

If you haven't installed Rust yet, you can follow these steps:

```bash
# 1) Install basic dependencies (Ubuntu / Debian)
sudo apt update
sudo apt install -y curl build-essential pkg-config libasound2-dev

# 2) Install rustup (Official Rust toolchain manager)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3) Apply to current shell
source "$HOME/.cargo/env"

# 4) Verify
rustc --version
cargo --version

# 5) Optional: Update to latest stable
rustup update stable
```

## Permissions Setup

```bash
sudo usermod -aG input "$USER"
echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-mouthwrite-uinput.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

After executing, it is recommended to re-login to the system once.

## Running Locally

```bash
cargo run -- start
```

On the first run, the configuration file will be created automatically:

```text
~/.config/mouthwrite/config.toml
```

You can check if the config is parseable first:

```bash
cargo run -- check-config
```

## User Installation (from tar.gz)

```bash
tar -xzf mouthwrite-linux-<version>-linux-x86_64.tar.gz
cd mouthwrite-linux-<version>-linux-x86_64

mkdir -p ~/.local/bin
cp mouthwrite-linux ~/.local/bin/
chmod +x ~/.local/bin/mouthwrite-linux

mkdir -p ~/.config/mouthwrite
cp config_template.toml ~/.config/mouthwrite/config.toml
```

Edit the config file to fill in your API Key:

```text
~/.config/mouthwrite/config.toml
```

## systemd User Service Installation

Using a user-level service is recommended (no root required, and it suits desktop sessions better).

Follow these steps to manually create the service file (you can adjust the config as needed):

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/mouthwrite.service <<'EOF'
[Unit]
Description=MouthWrite Linux Voice Input Daemon
# Wait for both graphical session (X11/Wayland) and audio services to be ready before starting
After=graphical-session.target pipewire.service
Requires=graphical-session.target
Wants=pipewire.service

[Service]
Type=simple
# Extra 3 seconds delay to ensure display server and audio devices are fully available
ExecStartPre=/usr/bin/sleep 3
ExecStart=%h/.local/bin/mouthwrite-linux start
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info,mouthwrite_linux=debug

[Install]
WantedBy=graphical-session.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now mouthwrite.service
systemctl --user status mouthwrite.service
```

If the executable is not in `~/.local/bin/mouthwrite-linux`, please change `ExecStart` to your actual path.

## Common Commands

```bash
# Start directly in foreground
~/.local/bin/mouthwrite-linux start

# Check configuration
~/.local/bin/mouthwrite-linux check-config

# View config path
~/.local/bin/mouthwrite-linux config-path

# View logs
journalctl --user -u mouthwrite.service -f
tail -f ~/.local/state/mouthwrite/app.log*
```

## Upgrade

```bash
systemctl --user stop mouthwrite.service
cp ./mouthwrite-linux ~/.local/bin/mouthwrite-linux
systemctl --user start mouthwrite.service
```

## Uninstall

```bash
systemctl --user disable --now mouthwrite.service
rm -f ~/.config/systemd/user/mouthwrite.service
systemctl --user daemon-reload
```

## Notes

- This project currently only supports Linux, it does not support running directly on Windows/macOS.
- If hotkeys do not work, first check if the `input` group permissions and `uinput` rules are in effect.
