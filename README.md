# MouthWrite Linux

MouthWrite 是一个 Linux 系统级语音输入工具。  
按住热键说话，松开后自动完成：语音识别 -> 文本优化/翻译 -> 复制到剪贴板 -> 点击目标位置自动粘贴。

项目当前为无 GUI 版本，运行更稳定、资源占用更低。

## 功能概览

- 全局热键按住录音，松开触发处理
- ASR（语音识别）+ LLM 文本优化
- 可切换翻译模式
- 自动写入剪贴板并模拟粘贴
- 播放提示音引导点击粘贴

## 系统要求

- Linux（依赖 `evdev` / `uinput`）
- Rust 工具链（构建时）
- 用户加入 `input` 组
- 已配置 `uinput` 权限

## Rust 环境搭建（新手）

如果你还没装过 Rust，可以按下面步骤：

```bash
# 1) 安装基础依赖（Ubuntu / Debian）
sudo apt update
sudo apt install -y curl build-essential pkg-config libasound2-dev

# 2) 安装 rustup（Rust 官方工具链管理器）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3) 让当前 shell 生效
source "$HOME/.cargo/env"

# 4) 验证
rustc --version
cargo --version

# 5) 可选：升级到最新 stable
rustup update stable
```

## 权限准备

```bash
sudo usermod -aG input "$USER"
echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-mouthwrite-uinput.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

执行后建议重新登录一次系统。

## 本地运行

```bash
cargo run -- start
```

首次运行会自动创建配置文件：

```text
~/.config/mouthwrite/config.toml
```

可先检查配置是否可解析：

```bash
cargo run -- check-config
```

## 用户安装（从 tar.gz）

```bash
tar -xzf mouthwrite-linux-<version>-linux-x86_64.tar.gz
cd mouthwrite-linux-<version>-linux-x86_64

mkdir -p ~/.local/bin
cp mouthwrite-linux ~/.local/bin/
chmod +x ~/.local/bin/mouthwrite-linux

mkdir -p ~/.config/mouthwrite
cp config_template.toml ~/.config/mouthwrite/config.toml
```

编辑配置文件，填写你的 API Key：

```text
~/.config/mouthwrite/config.toml
```

## systemd 用户服务安装

推荐用用户级服务（不需要 root，且更适配桌面会话）。

```bash
mkdir -p ~/.config/systemd/user
cp packaging/systemd/mouthwrite.service ~/.config/systemd/user/

systemctl --user daemon-reload
systemctl --user enable --now mouthwrite.service
systemctl --user status mouthwrite.service
```

## 常用命令

```bash
# 前台直接启动
~/.local/bin/mouthwrite-linux start

# 检查配置
~/.local/bin/mouthwrite-linux check-config

# 查看配置路径
~/.local/bin/mouthwrite-linux config-path

# 查看日志
journalctl --user -u mouthwrite.service -f
tail -f ~/.local/state/mouthwrite/app.log*
```

## 升级

```bash
systemctl --user stop mouthwrite.service
cp ./mouthwrite-linux ~/.local/bin/mouthwrite-linux
systemctl --user start mouthwrite.service
```

## 卸载

```bash
systemctl --user disable --now mouthwrite.service
rm -f ~/.config/systemd/user/mouthwrite.service
systemctl --user daemon-reload
```

## 说明

- 本项目当前只支持 Linux，不支持 Windows/macOS 直接运行。
- 如果热键无效，优先检查 `input` 组权限和 `uinput` 规则是否生效。
