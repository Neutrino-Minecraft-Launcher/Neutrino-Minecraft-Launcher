# Neutrino Minecraft Launcher

> 使用 Rust + Slint 构建的开源 Minecraft 启动器。

[![CI](https://img.shields.io/github/actions/workflow/status/NeutrinoMCL/Neutrino-Minecraft-Launcher/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/NeutrinoMCL/Neutrino-Minecraft-Launcher?style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/stargazers)
[![Downloads](https://img.shields.io/github/downloads/NeutrinoMCL/Neutrino-Minecraft-Launcher/total?label=downloads&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white&style=for-the-badge)](https://www.rust-lang.org/)
[![Slint](https://img.shields.io/badge/Slint-1.18-2379F4?style=for-the-badge)](https://slint.dev/)
[![License](https://img.shields.io/github/license/NeutrinoMCL/Neutrino-Minecraft-Launcher?label=license&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/blob/main/LICENSE)

> [!IMPORTANT]
> 项目处于早期开发阶段。当前已实现基础导航壳、壁纸界面、**命令行版本安装器**（`neutrino-install`），
> 以及 GUI「发现」页的版本安装与 Modrinth 内容浏览下载；Minecraft 启动与账户登录尚未实现。

## 简介

Neutrino Minecraft Launcher（中微子 Minecraft 启动器）的目标是提供一个轻量、易用的桌面 Minecraft 启动器。项目使用 Rust 编写核心逻辑，使用 Slint 构建声明式用户界面。

当前版本：`0.1.0`（开发中）

## 功能状态

- [x] Rust + Slint 桌面应用骨架
- [x] Slint UI 骨架、顶部导航与壁纸背景
- [x] 设置页、外观主题切换（跟随系统 / 深色 / 浅色）
- [x] 实例列表（扫描 `versions` 目录）与详情标签页
- [x] Java 运行时检测与打开安装目录
- [x] 版本安装核心库（版本清单、依赖规则、并发下载、SHA-1 校验、natives 解压）
- [x] 命令行安装器 `neutrino-install`（支持 `--list` / `--dry-run` / 镜像地址 / 线程数）
- [x] 安装功能接入 GUI（「实例」页：版本列表、筛选、安装进度、安装结果提示）
- [x] 「发现」页四类别：模组 / 光影包 / 材质包 / 整合包（来自 Modrinth，支持一键下载到 `mods/`、`shaderpacks/`、`resourcepacks/`）
- [x] GitHub Actions CI（格式检查、Clippy、测试）
- [x] 行为准则与安全策略
- [x] Microsoft 设备码登录流程（需配置客户端 ID）
- [ ] Minecraft 启动（参数拼装、JVM 参数、进程管理）
- [ ] 离线账户登录
- [ ] Java 运行时安装与切换
- [ ] 整合包一键安装（`.mrpack` 解析与版本生成）
- [ ] 下载源与镜像配置（核心库已预留地址参数）
- [ ] 自动更新与发布流程

## 技术栈

| 类别             | 技术                            |
| ---------------- | ------------------------------- |
| 语言             | Rust 2024                       |
| GUI              | Slint 1.18                      |
| 下载             | ureq（阻塞式 HTTP + rustls）    |
| 序列化 / 校验    | serde_json / sha1               |
| 解压             | zip（natives）                  |
| 构建             | Cargo                           |
| CI               | GitHub Actions（Windows）       |
| 当前主要构建目标 | Windows MSVC                    |
| 许可证           | GPL-3.0                         |

## Slint 文档

想进一步了解 Slint，可以阅读：

- [Slint 官方文档](https://docs.slint.dev/latest/docs/slint/)
- [Slint 中文文档](https://slintcn.neutriny.dev/)（非官方，由 [Neutriny](https://github.com/Neutriny) 维护）

## Microsoft 登录

微软账号登录使用 OAuth 2.0 设备码流程。需要先在 Azure Portal 注册支持个人 Microsoft 账号的公共客户端应用，然后设置客户端 ID：

```powershell
Copy-Item .env.example .env
```

编辑 `.env`：

```text
NEUTRINO_MSA_CLIENT_ID=<你的 Azure 应用客户端 ID>
```

也可以只在当前终端临时设置：

```powershell
$env:NEUTRINO_MSA_CLIENT_ID = "<你的 Azure 应用客户端 ID>"
```

- `.env` 已被 `.gitignore` 忽略；`.env.example` 可以提交。
- client ID 是公开标识，不是密钥，可以共享给协作者。
- 不需要也不应该把 `client_secret` 放进桌面客户端。

GitHub Actions 构建时可以从仓库变量读取：

```text
Settings → Secrets and variables → Actions → Variables
Name: NEUTRINO_MSA_CLIENT_ID
Value: <你的 Azure 应用客户端 ID>
```

工作流会把该变量传入编译环境；发布构建会把它作为编译时回退值写入程序，最终用户不需要再配置环境变量。本地开发仍优先使用 `.env` 或当前终端的环境变量。
- 登录 Scope 为 `XboxLive.signin offline_access`。
- 登录入口：账户 → 微软账号。
- 启动登录后会自动打开浏览器，并在界面显示设备码。

## 构建与运行

### 环境要求

- Rust stable **1.88+**（支持 edition 2024 与 `let` chains；当前用 1.99.1 验证）
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

## 版本安装（命令行）

GUI 的「发现」页已经可以直接装版本；`neutrino-install` 是同一套核心的命令行入口，
适合脚本化或排错时使用。两者都把版本完整装进 `.minecraft`（或你指定的目录），
装完启动器的「实例」页就能看到它。
```sh
# 查看可安装的版本（release / snapshot / all）
cargo run --bin neutrino-install -- --list
cargo run --bin neutrino-install -- --list snapshot

# 预演：只统计要下多少，不写盘
cargo run --bin neutrino-install -- 1.20.4 --dry-run

# 正式安装（含 assets）
cargo run --bin neutrino-install -- 1.20.4

# 只装最小可启动集合，装到指定目录、16 线程
cargo run --bin neutrino-install -- 1.20.4 -d D:/mctest --no-assets --threads 16
```

常用参数：

| 参数 | 说明 |
| --- | --- |
| `-d, --dir <路径>` | 游戏目录，默认取官方 `.minecraft` |
| `-t, --threads <数量>` | 并发下载线程数，默认 8 |
| `--retries <次数>` | 单文件失败重试次数，默认 3 |
| `--manifest <URL>` | 自定义版本清单地址（换镜像用） |
| `--no-assets` | 不下载资源文件（assets 通常几百 MB） |
| `--virtual-assets` | 为 1.6 及更早版本建立 virtual 资源目录 |
| `--dry-run` | 只统计下载量，不写盘 |

安装逻辑做了这些事：

1. 拉取 `version_manifest_v2.json` 并定位目标版本；
2. 拉取版本 JSON，按 `rules` 过滤出当前平台（Windows / macOS / Linux，x86_64 / arm64）需要的依赖库；
3. 下载客户端 jar、全部依赖库、资源索引与资源文件，**每个文件都用 SHA-1 校验**；
4. 1.18 及更早版本的 natives 会自动解压到 `versions/<id>/natives/`；
5. 已存在且校验通过的文件直接跳过，所以重复执行是幂等的，断点续传天然可用；
6. 结束时对比该版本要求的 Java 大版本与本机检测结果，给出提示。

## 实例页与发现页（GUI）

「实例」页左侧底部有「添加实例」入口，点击后展示 Mojang 版本清单（支持正式版 / 快照 / 全部筛选、安装进度与结果提示），安装完成后版本会出现在实例列表里。

「发现」页左侧分四个类别（均来自 Modrinth）：

| 类别 | 数据来源 | 下载位置 |
| --- | --- | --- |
| 模组 | Modrinth（project_type = mod） | `mods/` |
| 光影包 | Modrinth（project_type = shader） | `shaderpacks/` |
| 材质包 | Modrinth（project_type = resourcepack） | `resourcepacks/` |
| 整合包 | Modrinth（project_type = modpack） | 暂仅浏览，一键安装待实现 |

模组 / 光影 / 材质包会取项目最新的正式版文件（优先 `version_type = release` 的 primary 文件），
下载前同样走 SHA-1 校验。同一时间只允许一个下载任务。

## 项目结构

```text
.
├── src/main.rs                  # GUI 入口（Slint）
├── src/lib.rs                   # 共享库入口（GUI 与 CLI 共用）
├── src/core/                    # 安装核心
│   ├── manifest.rs              # 版本清单 / 版本 JSON / 规则求值 / Maven 坐标
│   ├── modrinth.rs              # Modrinth API 封装（搜索 / 版本列表 / 文件选择）
│   ├── net.rs                   # HTTP 下载、重试、SHA-1 校验、并发
│   ├── install.rs               # 安装主流程与报告
│   ├── natives.rs               # 旧版本 natives 解压
│   ├── paths.rs                 # .minecraft 目录布局
│   ├── progress.rs              # 进度上报（节流 + 可跨线程）
│   ├── hash.rs                  # SHA-1 工具
│   └── error.rs                 # 统一错误类型
├── src/bin/neutrino-install.rs  # 命令行安装器
├── src/desktop.rs               # 发现页控制器：版本列表 / 类别切换 / Modrinth 下载 / 安装线程编排（GUI 专用）
├── src/java.rs                  # Java 运行时检测
├── src/instances.rs             # versions 目录实例扫描
├── src/msa.rs                   # Microsoft 设备码登录
├── ui/app-window.slint          # 主窗口入口
├── ui/shell/                    # AppShell / TopBar / NavItem
├── ui/views/                    # 启动页 / 实例页 / 账户页 / 发现页 / 设置页
├── ui/models/                   # Page、JavaInfo、VersionInfo 等共享类型
├── ui/state/                    # AppState / Actions
├── ui/theme/                    # 主题偏好
├── ui/fonts/                    # 字体资源
├── ui/images/                   # 图标与壁纸
├── build.rs                     # 编译 Slint UI
├── .github/workflows/ci.yml     # CI 工作流
├── AGENTS.md                    # AI 协作说明
├── SECURITY.md                  # 安全政策
└── CODE_OF_CONDUCT.md           # 行为准则
```

## 下载

目前尚未发布可下载版本。首个 Release 发布后，顶部下载量徽章会自动更新。

## 已知问题

- **GUI 启动即崩溃（已规避）**：Slint 1.18.1 默认启用 `accessibility` 特性，在部分 Windows 环境下
  创建窗口时 `accesskit_windows` 的 `SetPropW` 会返回 `ERROR_NOT_ENOUGH_MEMORY` 并 panic，
  表现为双击 exe 一闪就退。当前 `Cargo.toml` 已关闭该特性（`default-features = false`），
  代价是暂时没有读屏软件（UIA）支持。等上游修复后可把 `"accessibility"` 加回 features。
- 安装入口位于「发现」页，同一时间只允许一个安装 / 下载任务，暂不支持取消。
- 整合包（`.mrpack`）目前只能浏览，一键安装需要解析覆盖文件并生成版本 JSON，尚未实现。

## 参与贡献

欢迎提交 Issue 和 Pull Request。开始前请阅读 [行为准则](CODE_OF_CONDUCT.md) 和 [安全政策](SECURITY.md)。

- `main` 分支受保护，请通过 PR 合并。
- PR 需要通过 CI，并获得至少 1 个 approval。
- 安全问题请勿公开提交 Issue，请按 `SECURITY.md` 中的方式报告。

## 许可证

本项目采用 [GPL-3.0](LICENSE) 许可证。

> NOT AN OFFICIAL MINECRAFT SERVICE. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
