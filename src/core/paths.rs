//! `.minecraft` 目录布局。
//!
//! 官方启动器的目录结构是事实标准，所有路径都从这里生成，避免各处手写 `join`。

use std::env;
use std::path::{Path, PathBuf};

/// 一个 `.minecraft` 根目录下的全部关键路径。
#[derive(Debug, Clone)]
pub struct GamePaths {
    root: PathBuf,
}

impl GamePaths {
    /// 用根目录初始化。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `versions/`
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// `versions/<id>/`
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id)
    }

    /// `versions/<id>/<id>.json`
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    /// `versions/<id>/<id>.jar`
    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }

    /// `versions/<id>/natives/`（1.18 及更早版本的本地库解压目录）
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }

    /// `libraries/`
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    /// `assets/`
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// `assets/indexes/`
    pub fn indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    /// `assets/indexes/<asset_id>.json`
    pub fn asset_index_file(&self, asset_id: &str) -> PathBuf {
        self.indexes_dir().join(format!("{asset_id}.json"))
    }

    /// `assets/objects/`
    pub fn objects_dir(&self) -> PathBuf {
        self.assets_dir().join("objects")
    }

    /// `assets/objects/<hash 前两位>/<hash>`
    pub fn object_path(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(..2).unwrap_or(hash);
        self.objects_dir().join(prefix).join(hash)
    }

    /// `assets/virtual/<asset_id>/`（1.6 及更早版本的虚拟资源目录）
    pub fn virtual_assets_dir(&self, asset_id: &str) -> PathBuf {
        self.assets_dir().join("virtual").join(asset_id)
    }
}

/// 官方启动器使用的默认游戏目录。
pub fn default_game_dir() -> PathBuf {
    if cfg!(windows) {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".minecraft")
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Library")
            .join("Application Support")
            .join("minecraft")
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".minecraft")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_unix_style(path: PathBuf) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn version_paths_follow_official_layout() {
        let paths = GamePaths::new("C:/mc");
        assert_eq!(
            as_unix_style(paths.version_json("1.20.4")),
            "C:/mc/versions/1.20.4/1.20.4.json"
        );
        assert_eq!(
            as_unix_style(paths.client_jar("1.20.4")),
            "C:/mc/versions/1.20.4/1.20.4.jar"
        );
        assert_eq!(
            as_unix_style(paths.natives_dir("1.20.4")),
            "C:/mc/versions/1.20.4/natives"
        );
    }

    #[test]
    fn object_path_uses_two_char_prefix() {
        let paths = GamePaths::new("C:/mc");
        let hash = "1a2b3c4d5e6f708192a3b4c5d6e7f80912345678";
        assert_eq!(
            as_unix_style(paths.object_path(hash)),
            format!("C:/mc/assets/objects/1a/{hash}")
        );
    }

    #[test]
    fn short_hash_does_not_panic() {
        let paths = GamePaths::new("C:/mc");
        // 脏数据里的短 hash 不应该导致 panic
        assert!(paths.object_path("ab").ends_with("ab"));
        assert!(paths.object_path("").ends_with("objects"));
    }
}
