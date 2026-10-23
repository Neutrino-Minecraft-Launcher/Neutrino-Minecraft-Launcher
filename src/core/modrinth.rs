//! Modrinth API v2 的最小封装：搜索项目、取最新文件。
//!
//! 只实现浏览与下载需要的那部分字段，接口地址：<https://docs.modrinth.com/api/>
//! 注意：Modrinth 要求带可识别的 User-Agent，这里复用 [`Downloader`] 里的全局 UA。

use std::collections::HashMap;

use serde::Deserialize;

use super::error::{CoreError, Result};
use super::net::Downloader;

const API_BASE: &str = "https://api.modrinth.com/v2";

/// 发现页的内容类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Mod,
    Shader,
    ResourcePack,
    ModPack,
}

impl ContentKind {
    /// 解析 UI 传来的类别键。
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "mod" => Some(Self::Mod),
            "shader" => Some(Self::Shader),
            "resourcepack" => Some(Self::ResourcePack),
            "modpack" => Some(Self::ModPack),
            _ => None,
        }
    }

    /// Modrinth 的 `project_type` 取值。
    pub fn project_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::ModPack => "modpack",
        }
    }

    /// 下载后落盘的子目录（相对 `.minecraft`）。
    pub fn target_dir(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::ModPack => "modpacks",
        }
    }

    /// 现阶段是否支持"一键下载安装"。
    pub fn supports_install(self) -> bool {
        // 整合包需要解析 .mrpack 并生成版本 JSON，暂未实现
        !matches!(self, Self::ModPack)
    }
}

/// 搜索结果。
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub hits: Vec<ProjectHit>,
    #[serde(default)]
    pub total_hits: u64,
}

/// 搜索结果里的单个项目。
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectHit {
    pub project_id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub downloads: u64,
}

/// 项目的一个版本。
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectVersion {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

/// 版本里的文件。
#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
}

impl VersionFile {
    /// SHA-1（Modrinth 一定会给）。
    pub fn sha1(&self) -> Option<&str> {
        self.hashes.get("sha1").map(String::as_str)
    }
}

/// 按类别搜索项目，按下载量排序。
pub fn search(downloader: &Downloader, kind: ContentKind, limit: usize) -> Result<SearchResponse> {
    // facets 形如 [["project_type:mod"]]，这里手工百分号编码，避免额外依赖
    let url = format!(
        "{API_BASE}/search?limit={limit}&index=downloads&facets=%5B%5B%22project_type%3A{}%22%5D%5D",
        kind.project_type()
    );
    downloader.get_json(&url, "Modrinth 搜索结果")
}

/// 取某个项目的全部版本（接口按发布时间倒序返回）。
pub fn project_versions(downloader: &Downloader, project_id: &str) -> Result<Vec<ProjectVersion>> {
    let url = format!("{API_BASE}/project/{project_id}/version");
    downloader.get_json(&url, "Modrinth 版本列表")
}

/// 从版本列表里挑一个"最新且可下载"的文件。
///
/// 策略：优先最新的正式版（`version_type == "release"`），没有就退回最新版本；
/// 每个版本内部优先 `primary` 文件。
pub fn pick_file(versions: &[ProjectVersion]) -> Option<&VersionFile> {
    versions
        .iter()
        .find(|version| version.version_type == "release")
        .and_then(primary_file)
        .or_else(|| versions.iter().find_map(primary_file))
}

fn primary_file(version: &ProjectVersion) -> Option<&VersionFile> {
    version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
}

/// 校验类别键是否合法，顺便给出错误信息。
pub fn parse_kind(key: &str) -> Result<ContentKind> {
    ContentKind::parse(key).ok_or_else(|| CoreError::Other(format!("未知的内容类别：{key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_maps_to_modrinth_type_and_dir() {
        assert_eq!(ContentKind::Mod.project_type(), "mod");
        assert_eq!(ContentKind::Shader.target_dir(), "shaderpacks");
        assert_eq!(ContentKind::ResourcePack.target_dir(), "resourcepacks");
        assert!(ContentKind::Mod.supports_install());
        assert!(!ContentKind::ModPack.supports_install());
    }

    #[test]
    fn parses_search_hit() {
        let json = r#"{
          "hits": [{
            "project_id": "P7dR8mSH",
            "slug": "fabric-api",
            "title": "Fabric API",
            "description": "Lightweight and modular API",
            "author": "modmuss50",
            "categories": ["fabric", "library"],
            "project_type": "mod",
            "downloads": 286109303
          }],
          "total_hits": 12345
        }"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.total_hits, 12345);
        assert_eq!(parsed.hits[0].title, "Fabric API");
        assert_eq!(parsed.hits[0].downloads, 286109303);
    }

    #[test]
    fn prefers_release_then_primary_file() {
        let json = r#"[
          {
            "id": "new-beta", "version_type": "beta", "version_number": "2.0-beta",
            "files": [{ "url": "https://x/beta.jar", "filename": "beta.jar", "primary": true, "hashes": {"sha1": "bb"} }]
          },
          {
            "id": "old-release", "version_type": "release", "version_number": "1.5",
            "files": [
              { "url": "https://x/extra.jar", "filename": "extra.jar", "primary": false, "hashes": {} },
              { "url": "https://x/main.jar", "filename": "main.jar", "primary": true, "hashes": {"sha1": "aa"} }
            ]
          }
        ]"#;
        let versions: Vec<ProjectVersion> = serde_json::from_str(json).unwrap();
        let file = pick_file(&versions).unwrap();
        assert_eq!(file.filename, "main.jar");
        assert_eq!(file.sha1(), Some("aa"));
    }

    #[test]
    fn falls_back_to_newest_version() {
        let json = r#"[
          {
            "id": "b", "version_type": "beta",
            "files": [{ "url": "https://x/a.jar", "filename": "a.jar" }]
          }
        ]"#;
        let versions: Vec<ProjectVersion> = serde_json::from_str(json).unwrap();
        assert_eq!(pick_file(&versions).unwrap().filename, "a.jar");
        assert!(pick_file(&[]).is_none());
    }
}
