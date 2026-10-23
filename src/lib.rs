//! Neutrino Minecraft Launcher 的共享库。
//!
//! GUI（`src/main.rs`）与命令行安装器（`src/bin/neutrino-install.rs`）共用这里的逻辑：
//!
//! - [`core`]：Minecraft 版本安装核心（版本清单、依赖解析、下载校验、natives 解压）
//! - [`java`]：本机 Java 运行时检测
//! - [`instances`]：`.minecraft/versions` 目录扫描

pub mod core;
pub mod instances;
pub mod java;
