# Neutrino Minecraft Launcher

> 使用 Rust + Slint 构建的开源 Minecraft 启动器。

[![CI](https://img.shields.io/github/actions/workflow/status/NeutrinoMCL/Neutrino-Minecraft-Launcher/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/NeutrinoMCL/Neutrino-Minecraft-Launcher?style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/stargazers)
[![Downloads](https://img.shields.io/github/downloads/NeutrinoMCL/Neutrino-Minecraft-Launcher/total?label=downloads&style=for-the-badge)](https://github.com/NeutrinoMCL/Neutrino-Minecraft-Launcher/releases)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white&style=for-the-badge)](https://www.rust-lang.org/)
[![Slint](https://img.shields.io/badge/Slint-1.18-2379F4?style=for-the-badge)](https://slint.dev/)

> [!IMPORTANT]
> 项目处于早期开发阶段。当前已实现基础导航壳、壁纸界面、**命令行版本安装器**（`neutrino-install`），
> 以及 GUI「实例」页的原版 / Fabric / Forge 实例创建与 Modrinth 内容浏览下载；
> Minecraft 启动（离线账号）已可用，Microsoft 设备码登录流程已接入。

## 简介

Neutrino Minecraft Launcher（中微子 Minecraft 启动器）的目标是提供一个轻量、易用的桌面 Minecraft 启动器。项目使用 Rust 编写核心逻辑，使用 Slint 构建声明式用户界面。

当前版本：`0.1.0`（开发中）

## 功能状态

- [x] Rust + Slint 桌面应用骨架
- [x] Slint UI 骨架、顶部导航与壁纸背景
- [x] 设置页、外观主题切换（跟随系统 / 深色 / 浅色）
- [x] 实例列表（扫描 `versions` 目录）与详情标签页
- [x] 实例内容扫描（模组、资源包、光影）
- [x] 离线角色添加、选择与本地保存
- [x] 离线角色皮肤（Steve / Alex / 自定义）
- [x] Java 运行时检测与打开安装目录
- [x] 版本安装核心库（版本清单、依赖规则、并发下载、SHA-1 校验、natives 解压）
- [x] 命令行安装器 `neutrino-install`（支持 `--list` / `--dry-run` / 镜像地址 / 线程数）
- [x] 安装功能接入 GUI（「实例」页：版本列表、创建实例向导、安装进度、安装结果提示）
- [x] 创建实例时选择模组加载器（Fabric / Forge）与加载器版本，或跳过只装原版
- [x] 自定义实例名，实例目录为自包含的 `versions/<实例名>/`
- [x] 「发现」页四类别：模组 / 光影包 / 材质包 / 整合包（来自 Modrinth，支持一键下载到 `mods/`、`shaderpacks/`、`resourcepacks/`）
- [x] GitHub Actions CI（格式检查、Clippy、测试）
- [x] 行为准则与安全策略
- [x] Microsoft 设备码登录流程（需配置客户端 ID）
- [x] Minecraft 启动（参数拼装、JVM 参数、进程管理；当前支持离线账号 + 原版 / Fabric / Forge 实例）
- [x] 离线账户登录
- [ ] 整合包一键安装（`.mrpack` 解析与版本生成）
- [ ] 下载源与镜像配置（核心库已预留地址参数）
- [ ] 自动更新与发布流程

## 技术栈

| 类别             | 技术                         |
|------------------|------------------------------|
| 语言             | Rust 2024                    |
| GUI              | Slint 1.18                   |
| 下载             | ureq（阻塞式 HTTP + rustls） |
| 序列化 / 校验    | serde_json / sha1            |
| 解压             | zip（natives）               |
| 构建             | Cargo                        |
| CI               | GitHub Actions（Windows）    |
| 当前主要构建目标 | Windows MSVC                 |

## Microsoft 登录

微软账号登录使用 OAuth 2.0 设备码流程，Scope 为 `XboxLive.signin offline_access`。

- 登录入口：账户 → 微软账号。
- 启动登录后会自动打开浏览器，并在界面显示设备码。
- 本地开发与 CI 的客户端 ID 配置见 [CONTRIBUTING.md](CONTRIBUTING.md#microsoft-登录配置可选)。

## 版本安装（命令行）

GUI 的「实例」页 →「添加实例」已经可以通过向导创建原版 / Fabric / Forge 实例；
`neutrino-install` 是同一套安装核心的命令行入口（只装原版），适合脚本化或排错时使用。
两者都把依赖库和资源文件装进 `.minecraft`（或你指定的目录），装完启动器的「实例」页就能看到它。

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

| 参数                   | 说明                                   |
|------------------------|----------------------------------------|
| `-d, --dir <路径>`     | 游戏目录，默认取官方 `.minecraft`      |
| `-t, --threads <数量>` | 并发下载线程数，默认 8                 |
| `--retries <次数>`     | 单文件失败重试次数，默认 3             |
| `--manifest <URL>`     | 自定义版本清单地址（换镜像用）         |
| `--no-assets`          | 不下载资源文件（assets 通常几百 MB）   |
| `--virtual-assets`     | 为 1.6 及更早版本建立 virtual 资源目录 |
| `--dry-run`            | 只统计下载量，不写盘                   |

安装逻辑做了这些事：

1. 拉取 `version_manifest_v2.json` 并定位目标版本；
2. 拉取版本 JSON，按 `rules` 过滤出当前平台（Windows / macOS / Linux，x86_64 / arm64）需要的依赖库；
3. 下载客户端 jar、全部依赖库、资源索引与资源文件，**每个文件都用 SHA-1 校验**；
4. 1.18 及更早版本的 natives 会自动解压到 `versions/<id>/natives/`；
5. 已存在且校验通过的文件直接跳过，所以重复执行是幂等的，断点续传天然可用；
6. 结束时对比该版本要求的 Java 大版本与本机检测结果，给出提示。

## 实例页与发现页（GUI）

「实例」页左侧底部有「添加实例」入口，点击后进入创建向导：

1. **选择版本**：展示 Mojang 版本清单，支持正式版 / 快照 / 全部筛选；列表不再检测本地是否已安装。
2. **选择加载器**：Fabric、Forge，或「跳过」只装原版。
3. **选择加载器版本**：Fabric 会标出稳定版，Forge 展示该 Minecraft 版本对应的全部 Forge 版本。
4. **输入实例名**：默认 `<版本>-<加载器>`（跳过加载器时就是版本号），保存前会校验非法字符和重名。

创建完成后会生成自包含的 `versions/<实例名>/`（合并后的版本 JSON + 客户端 jar），
加载器依赖库和资源文件放在共享的 `libraries/`、`assets/`。Forge 安装会调用官方 installer，
需要本机已检测到 Java；Fabric 直接读取 Fabric Meta 的 loader profile，不需要启动 Java。

启动时 `--gameDir` 和工作目录都指向 `versions/<实例名>/`，所以 `mods/`、`saves/`、`config/`、
`logs/` 都是按实例隔离的；`libraries/`、`assets/` 仍是所有实例共享的运行时文件。

「发现」页左侧分四个类别（均来自 Modrinth）：

| 类别   | 数据来源                                | 下载位置                 |
|--------|-----------------------------------------|--------------------------|
| 模组   | Modrinth（project_type = mod）          | `mods/`                  |
| 光影包 | Modrinth（project_type = shader）       | `shaderpacks/`           |
| 材质包 | Modrinth（project_type = resourcepack） | `resourcepacks/`         |
| 整合包 | Modrinth（project_type = modpack）      | 暂仅浏览，一键安装待实现 |

模组 / 光影 / 材质包会取项目最新的正式版文件（优先 `version_type = release` 的 primary 文件），
下载前同样走 SHA-1 校验。同一时间只允许一个下载任务。

## 下载

目前尚未发布可下载版本。首个 Release 发布后，顶部下载量徽章会自动更新。

## 已知问题

- **GUI 启动即崩溃（已规避）**：Slint 1.18.1 的 `accessibility` 特性在部分 Windows 环境下
  会导致窗口创建时 panic，表现为双击 exe 一闪就退。当前已关闭该特性，代价是暂时没有读屏软件（UIA）支持。
  相关开发注意事项见 [CONTRIBUTING.md](CONTRIBUTING.md#windows-开发注意事项)。
- 安装入口位于「实例」页，同一时间只允许一个安装 / 下载任务，暂不支持取消。
- 整合包（`.mrpack`）目前只能浏览，一键安装需要解析覆盖文件并生成版本 JSON，尚未实现。

## 参与贡献

欢迎提交 Issue 和 Pull Request！开发环境、代码约定、提交规范和 PR 流程请阅读
[CONTRIBUTING.md](CONTRIBUTING.md)，参与讨论前请遵守[行为准则](CODE_OF_CONDUCT.md)。

安全问题请勿公开提交 Issue，请按 [SECURITY.md](SECURITY.md) 中的方式报告。

## 许可证

本项目采用 [GPL-3.0](LICENSE) 许可证。

> NOT AN OFFICIAL MINECRAFT SERVICE. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
