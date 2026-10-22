//! 安装核心的错误类型。
//!
//! 统一收口的好处：CLI 可以直接把错误打印给用户，GUI 以后可以按变体做不同提示，
//! 而不是像现在这样只 `eprintln!` 一行英文。

use std::path::{Path, PathBuf};

/// 安装过程中可能出现的所有错误。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("网络错误：{0}")]
    Network(String),

    #[error("HTTP {status}：{url}")]
    HttpStatus { status: u16, url: String },

    #[error("解析{context}失败：{source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("读写 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("解压 {path} 失败：{source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("校验失败：{path} 的 SHA-1 不匹配（期望 {expected}，实际 {actual}）")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("版本清单中没有版本 “{0}”")]
    UnknownVersion(String),

    #[error("版本信息缺少必要字段：{0}")]
    MissingField(&'static str),

    #[error("{0}")]
    Other(String),
}

/// 核心模块统一使用的结果类型。
pub type Result<T> = std::result::Result<T, CoreError>;

/// 给 IO 错误补上路径上下文。
pub fn io_err(path: impl AsRef<Path>, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.as_ref().to_path_buf(),
        source,
    }
}

/// 给 zip 错误补上文件路径。
pub fn zip_err(path: impl AsRef<Path>, source: zip::result::ZipError) -> CoreError {
    CoreError::Zip {
        path: path.as_ref().to_path_buf(),
        source,
    }
}
