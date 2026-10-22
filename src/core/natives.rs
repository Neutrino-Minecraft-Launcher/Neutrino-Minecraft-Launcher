//! 旧版本（1.18 及更早）的 natives 解压。
//!
//! 1.19 起 Mojang 把 LWJGL natives 作为独立依赖库直接放进 classpath，由 LWJGL 自己解压，
//! 因此只有带 `natives` 字段的老版本才需要这一步。

use std::fs::{self, File};
use std::io;
use std::path::Path;

use zip::ZipArchive;

use super::error::{Result, io_err, zip_err};

/// 从 natives jar 解压到 `dest`，返回写出的文件数。
pub fn extract_natives(jar: &Path, dest: &Path, excludes: &[String]) -> Result<usize> {
    let file = File::open(jar).map_err(|err| io_err(jar, err))?;
    let mut archive = ZipArchive::new(file).map_err(|err| zip_err(jar, err))?;

    fs::create_dir_all(dest).map_err(|err| io_err(dest, err))?;

    let mut written = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| zip_err(jar, err))?;

        // enclosed_name 会拒绝 `../` 之类的路径，避免 zip slip
        let Some(relative) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };

        let name = relative.to_string_lossy().replace('\\', "/");
        if name.is_empty() || name.ends_with('/') {
            continue;
        }
        if excludes
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
        {
            continue;
        }

        let target = dest.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| io_err(parent, err))?;
        }

        let mut out = File::create(&target).map_err(|err| io_err(&target, err))?;
        io::copy(&mut entry, &mut out).map_err(|err| io_err(&target, err))?;
        written += 1;
    }

    Ok(written)
}
