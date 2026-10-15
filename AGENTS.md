# Neutrino Minecraft Launcher

> 中微子Minecraft启动器

面向 AI 编码代理的工作指南。

## 环境搭建

- 安装依赖：`cargo build`
- 构建：`cargo build`
- 运行测试：`cargo test`
- 代码检查 / 格式化：`cargo clippy -- -D warnings`

## 代码规范

- 全代码库强制严格类型。
- 运行 `cargo fmt`；`cargo clippy` 必须零警告通过。
- 库代码优先使用 `Result` 而非 panic。
- 非测试代码中不得使用 `unwrap()`，除非附有合理注释。

## 项目结构

- `src/` 模块；集成测试位于 `tests/`。

## 防护规则

以下是 AI 代理常犯的错误，必须严格遵守：

- 禁止手动编辑 `Cargo.lock`。
- 引入 `unsafe` 必须附有证明其安全性的注释。
- 完成任务前必须运行完整测试套件并修复所有失败。
- 禁止在未检查锁文件和现有约定的情况下添加新依赖。
- 禁止提交密钥、API 密钥或 `.env` 文件。
- 匹配现有模式。禁止为代码库已解决的功能引入新库。
- 保持变更范围与任务一致。禁止格式化或重构无关文件。
- 大规模重构或重命名公共 API 前必须先询问。

## 提交与拉取请求

- 使用约定式提交：`feat:`、`fix:`、`chore:`、`docs:`、`refactor:`。
- 保持 PR 聚焦单一关注点；描述变更内容及原因。
