# Neutrino Minecraft Launcher

> 使用 Rust + Slint 构建的开源 Minecraft 启动器。

[![CI](https://img.shields.io/github/actions/workflow/status/NeutrinoMCL/Neutrino-Minecraft-Launcher/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/NeutrinoMCL/Neutrino-Minecraft-Launcher?style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/stargazers)
[![Downloads](https://img.shields.io/github/downloads/NeutrinoMCL/Neutrino-Minecraft-Launcher/total?label=downloads&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white&style=for-the-badge)](https://www.rust-lang.org/)
[![Slint](https://img.shields.io/badge/Slint-1.18-2379F4?style=for-the-badge)](https://slint.dev/)
[![License](https://img.shields.io/github/license/NeutrinoMCL/Neutrino-Minecraft-Launcher?label=license&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/blob/main/LICENSE)

> [!IMPORTANT]
> 项目处于早期开发阶段。当前已实现基础导航壳和壁纸界面，Minecraft 启动、账户登录、实例管理等功能尚未实现。

## 简介

Neutrino Minecraft Launcher（中微子 Minecraft 启动器）的目标是提供一个轻量、易用的桌面 Minecraft 启动器。项目使用 Rust 编写核心逻辑，使用 Slint 构建声明式用户界面。

当前版本：`0.1.0`（开发中）

## 功能状态

- [x] Rust + Slint 桌面应用骨架
- [x] Slint UI 骨架、顶部导航与壁纸背景
- [x] 设置页、外观主题切换（跟随系统 / 深色 / 浅色）
- [x] 实例列表（扫描 `versions` 目录）与详情标签页
- [x] Java 运行时检测与打开安装目录
- [x] GitHub Actions CI（格式检查、Clippy、测试）
- [x] 行为准则与安全策略
- [ ] Minecraft 版本安装与启动
- [x] Microsoft 设备码登录流程（需配置客户端 ID）
- [ ] 离线账户登录
- [ ] Java 运行时安装与切换
- [ ] 模组与整合包支持
- [ ] 下载源与镜像配置
- [ ] 自动更新与发布流程

## 技术栈

| 类别             | 技术                      |
| ---------------- | ------------------------- |
| 语言             | Rust 2024                 |
| GUI              | Slint 1.18                |
| 构建             | Cargo                     |
| CI               | GitHub Actions（Windows） |
| 当前主要构建目标 | Windows MSVC              |
| 许可证           | GPL-3.0                   |

## Slint 文档

想进一步了解 Slint，可以阅读：

- [Slint 官方文档](https://docs.slint.dev/latest/docs/slint/)
- [Slint 中文文档](https://slintcn.neutriny.dev/)（非官方，由 [Neutriny](https://github.com/Neutriny) 维护）

## Microsoft 登录

微软账号登录使用 OAuth 2.0 设备码流程。需要先在 Azure Portal 注册支持个人 Microsoft 账号的公共客户端应用，然后设置客户端 ID：

```powershell
$env:NEUTRINO_MSA_CLIENT_ID = "<你的 Azure 应用客户端 ID>"
```

- 不需要也不应该把 `client_secret` 放进桌面客户端。
- 登录 Scope 为 `XboxLive.signin offline_access`。
- 登录入口：账户 → 微软账号。
- 启动登录后会自动打开浏览器，并在界面显示设备码。

## 构建与运行

### 环境要求

- Rust stable（支持 edition 2024）
- Windows：MSVC 工具链与 Visual Studio Build Tools（C++ 桌面开发）
- 其他平台尚未验证

### 常用命令

```sh
# 运行应用
cargo run

# 发布构建
cargo build --release

# 仅预览 Slint UI
slint-viewer ui/app-window.slint
```

### 本地检查

提交 PR 前请运行与 CI 相同的检查：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## 项目结构

```text
.
├── src/main.rs               # Rust 入口
├── src/java.rs               # Java 运行时检测
├── src/instances.rs          # versions 目录实例扫描
├── src/msa.rs                # Microsoft 设备码登录
├── ui/app-window.slint       # 主窗口入口
├── ui/shell/                 # AppShell / TopBar / NavItem
├── ui/views/                 # 启动页 / 设置页
├── ui/models/                # Page、JavaInfo 等共享类型
├── ui/state/                 # AppState / Actions
├── ui/theme/                 # 主题偏好
├── ui/fonts/                 # 字体资源
├── ui/images/                # 图标与壁纸
├── build.rs                  # 编译 Slint UI
├── .github/workflows/ci.yml  # CI 工作流
├── AGENTS.md                 # AI 协作说明
├── SECURITY.md               # 安全政策
└── CODE_OF_CONDUCT.md        # 行为准则
```

## 下载

目前尚未发布可下载版本。首个 Release 发布后，顶部下载量徽章会自动更新。

## 参与贡献

欢迎提交 Issue 和 Pull Request。开始前请阅读 [行为准则](CODE_OF_CONDUCT.md) 和 [安全政策](SECURITY.md)。

- `main` 分支受保护，请通过 PR 合并。
- PR 需要通过 CI，并获得至少 1 个 approval。
- 安全问题请勿公开提交 Issue，请按 `SECURITY.md` 中的方式报告。

## 许可证

本项目采用 [GPL-3.0](LICENSE) 许可证。

> NOT AN OFFICIAL MINECRAFT SERVICE. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
