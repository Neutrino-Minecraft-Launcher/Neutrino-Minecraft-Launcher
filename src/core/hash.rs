//! SHA-1 校验工具。
//!
//! Mojang 的版本清单、依赖库与资源索引都以 SHA-1 作为完整性依据，
//! 下载后必须核对，否则断点/镜像污染会直接导致启动失败。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

use super::error::{Result, io_err};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// 把字节序列编码成小写十六进制字符串。
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// 计算内存中数据的 SHA-1。
pub fn sha1_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

/// 计算文件的 SHA-1（流式读取，不会把大文件整个读进内存）。
pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|err| io_err(path, err))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|err| io_err(path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex_encode(&hasher.finalize()))
}

/// 判断文件是否存在且 SHA-1 与期望值一致。
///
/// 任何读取失败都视为“不匹配”（宁可重新下载，也不要拿坏文件去启动）。
pub fn sha1_matches(path: &Path, expected: &str) -> bool {
    if !path.is_file() {
        return false;
    }

    match sha1_file(path) {
        Ok(actual) => actual.eq_ignore_ascii_case(expected.trim()),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn sha1_of_known_inputs() {
        // 标准测试向量
        assert_eq!(sha1_bytes(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            sha1_bytes(b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn sha1_matches_rejects_missing_file() {
        assert!(!sha1_matches(
            Path::new("definitely-not-here.bin"),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        ));
    }
}
