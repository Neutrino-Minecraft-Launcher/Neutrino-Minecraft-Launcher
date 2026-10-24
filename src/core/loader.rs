//! 模组加载器：版本查询、Fabric profile 获取、Forge 安装器调用。
//!
//! Fabric 的 loader profile 是一段完整的版本 JSON（带 `inheritsFrom`），
//! 可以直接合并进实例；Forge 则需要运行官方 installer，由它生成
//! `libraries/` 里的补丁 jar 和版本 JSON。

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use super::error::{CoreError, Result, io_err};
use super::net::Downloader;
use super::paths::GamePaths;

pub const FABRIC_META: &str = "https://meta.fabricmc.net";
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";

/// 支持的模组加载器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderKind {
    Fabric,
    Forge,
}

impl LoaderKind {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "fabric" => Some(Self::Fabric),
            "forge" => Some(Self::Forge),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Forge => "forge",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
        }
    }
}

/// 加载器的一个可安装版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderVersion {
    pub version: String,
    /// Fabric 的稳定版标记；Forge 没有这个概念，恒为 `false`（界面不显示徽章）。
    pub stable: bool,
}

#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}

#[derive(Deserialize)]
struct FabricLoader {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// 列出某个 Minecraft 版本可用的加载器版本（新的在前，Fabric 稳定版优先）。
pub fn loader_versions(
    downloader: &Downloader,
    kind: LoaderKind,
    mc_version: &str,
) -> Result<Vec<LoaderVersion>> {
    match kind {
        LoaderKind::Fabric => fabric_versions(downloader, mc_version),
        LoaderKind::Forge => forge_versions(downloader, mc_version),
    }
}

fn fabric_versions(downloader: &Downloader, mc_version: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!("{FABRIC_META}/v2/versions/loader/{mc_version}");
    let entries: Vec<FabricEntry> = downloader.get_json(&url, "Fabric 加载器版本")?;

    let mut versions: Vec<LoaderVersion> = entries
        .into_iter()
        .map(|entry| LoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect();

    // API 本身是新到旧；稳定版排到前面，同一组内保持原有顺序。
    versions.sort_by_key(|version| !version.stable);
    Ok(versions)
}

fn forge_versions(downloader: &Downloader, mc_version: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!("{FORGE_MAVEN}/net/minecraftforge/forge/maven-metadata.xml");
    let xml = downloader.get_bytes(&url)?;
    Ok(parse_forge_versions(
        &String::from_utf8_lossy(&xml),
        mc_version,
    ))
}

fn parse_forge_versions(xml: &str, mc_version: &str) -> Vec<LoaderVersion> {
    let prefix = format!("{mc_version}-");

    let mut seen = HashSet::new();
    let mut versions: Vec<LoaderVersion> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</version>").next())
        .map(str::trim)
        .filter_map(|version| version.strip_prefix(&prefix))
        .filter(|version| seen.insert(version.to_string()))
        .map(|version| LoaderVersion {
            version: version.to_string(),
            stable: false,
        })
        .collect();

    versions.sort_by_key(|item| std::cmp::Reverse(version_key(&item.version)));
    versions
}

/// 把 `47.4.28` 这样的版本转成可比较的数字序列。
fn version_key(value: &str) -> Vec<u64> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Fabric 的 loader profile（原始 JSON，`inheritsFrom` 指向原版版本）。
pub fn fabric_profile(
    downloader: &Downloader,
    mc_version: &str,
    loader_version: &str,
) -> Result<Vec<u8>> {
    let url =
        format!("{FABRIC_META}/v2/versions/loader/{mc_version}/{loader_version}/profile/json");
    downloader.get_bytes(&url)
}

/// Forge 官方安装器的下载地址。
pub fn forge_installer_url(mc_version: &str, loader_version: &str) -> String {
    let id = format!("{mc_version}-{loader_version}");
    format!("{FORGE_MAVEN}/net/minecraftforge/forge/{id}/forge-{id}-installer.jar")
}

/// Forge 安装器运行后的输出。
#[derive(Debug)]
pub struct ForgeInstall {
    /// 安装器生成的版本 id（通常是 `<mc>-forge-<loader>`）。
    pub id: String,
    /// 生成版本 JSON 的原始字节。
    pub raw: Vec<u8>,
    /// 这个版本目录是否是本次安装新建的（决定要不要清理）。
    pub created: bool,
}

/// 下载并运行 Forge 官方安装器（需要 Java）。
///
/// 安装器会直接往 `game_dir/libraries` 写补丁 jar，并在 `versions/` 下生成
/// 一个临时版本目录；调用方读完 JSON 后需要自行清理该目录。
pub fn install_forge(
    downloader: &Downloader,
    game_dir: &Path,
    mc_version: &str,
    loader_version: &str,
    java: &Path,
) -> Result<ForgeInstall> {
    let paths = GamePaths::new(game_dir);
    let before = version_dirs(&paths);

    // 官方安装器要求 launcher_profiles.json 至少存在。
    let profiles = game_dir.join("launcher_profiles.json");
    if !profiles.is_file() {
        std::fs::write(&profiles, br#"{"profiles":{}}"#).map_err(|err| io_err(&profiles, err))?;
    }

    let installer = std::env::temp_dir().join(format!(
        "neutrino-forge-{mc_version}-{loader_version}-installer.jar"
    ));
    let url = forge_installer_url(mc_version, loader_version);
    downloader.download(&url, &installer, None, 0)?;

    let output = run_installer(java, &installer, game_dir);
    let _ = std::fs::remove_file(&installer);

    let output = output?;
    if !output.status.success() {
        return Err(CoreError::Other(format!(
            "Forge 安装器失败（退出码 {:?}）：{}",
            output.status.code(),
            output_tail(&output.stdout, &output.stderr)
        )));
    }

    let id = find_generated_version(&paths, mc_version, loader_version, &before)?;
    let created = !before.contains(&id);
    let json_path = paths.version_json(&id);
    let raw = std::fs::read(&json_path).map_err(|err| io_err(&json_path, err))?;

    Ok(ForgeInstall { id, raw, created })
}

fn run_installer(java: &Path, installer: &Path, game_dir: &Path) -> Result<std::process::Output> {
    let mut command = Command::new(java);
    command
        .arg("-jar")
        .arg(installer)
        .arg("--installClient")
        .arg(game_dir)
        .current_dir(game_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }

    command
        .output()
        .map_err(|err| CoreError::Other(format!("启动 Forge 安装器失败：{err}")))
}

fn version_dirs(paths: &GamePaths) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(paths.versions_dir()) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

fn find_generated_version(
    paths: &GamePaths,
    mc_version: &str,
    loader_version: &str,
    before: &HashSet<String>,
) -> Result<String> {
    // 正常情况就是 `<mc>-forge-<loader>`。
    let expected = format!("{mc_version}-forge-{loader_version}");
    if paths.version_json(&expected).is_file() {
        return Ok(expected);
    }

    // 老版本 Forge 的 id 规则不同，退回到“新出现的版本目录”。
    let after = version_dirs(paths);
    let mut created = after
        .difference(before)
        .filter(|name| *name != mc_version)
        .cloned()
        .collect::<Vec<_>>();
    created.sort();
    if let Some(id) = created.into_iter().next() {
        return Ok(id);
    }

    Err(CoreError::Other(
        "Forge 安装器没有生成版本 JSON".to_string(),
    ))
}

fn output_tail(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(stderr));

    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(10);
    lines[start..]
        .iter()
        .map(|line| line.chars().take(300).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 安装器临时版本目录的清理。
pub fn cleanup_generated(game_dir: &Path, id: &str, mc_version: &str) {
    if id == mc_version {
        return;
    }
    let dir = GamePaths::new(game_dir).version_dir(id);
    let _ = std::fs::remove_dir_all(dir);
}

/// 默认实例名：原版直接用版本号，带加载器时拼上加载器名。
pub fn default_instance_name(mc_version: &str, kind: Option<LoaderKind>) -> String {
    match kind {
        Some(kind) => format!("{mc_version}-{}", kind.key()),
        None => mc_version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loader_kinds() {
        assert_eq!(LoaderKind::parse("fabric"), Some(LoaderKind::Fabric));
        assert_eq!(LoaderKind::parse("forge"), Some(LoaderKind::Forge));
        assert_eq!(LoaderKind::parse("quilt"), None);
        assert_eq!(LoaderKind::Fabric.key(), "fabric");
        assert_eq!(LoaderKind::Forge.name(), "Forge");
    }

    #[test]
    fn version_key_orders_numerically() {
        assert!(version_key("47.4.28") > version_key("47.4.9"));
        assert!(version_key("47.4.0") > version_key("47.4"));
        assert_eq!(version_key("1.20.1"), vec![1, 20, 1]);
    }

    #[test]
    fn default_names() {
        assert_eq!(default_instance_name("1.20.1", None), "1.20.1");
        assert_eq!(
            default_instance_name("1.20.1", Some(LoaderKind::Fabric)),
            "1.20.1-fabric"
        );
    }

    #[test]
    fn forge_installer_url_matches_maven_layout() {
        assert_eq!(
            forge_installer_url("1.20.1", "47.4.28"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.28/forge-1.20.1-47.4.28-installer.jar"
        );
    }

    #[test]
    fn parses_forge_metadata_versions() {
        let xml = r#"
        <metadata>
          <versioning>
            <versions>
              <version>1.20.1-47.4.9</version>
              <version>1.20.1-47.4.28</version>
              <version>1.20.1-47.4.10</version>
              <version>1.19.2-43.3.0</version>
              <version>1.20.1-47.4.28</version>
            </versions>
          </versioning>
        </metadata>"#;

        let versions = parse_forge_versions(xml, "1.20.1");
        let numbers = versions
            .iter()
            .map(|item| item.version.as_str())
            .collect::<Vec<_>>();
        assert_eq!(numbers, vec!["47.4.28", "47.4.10", "47.4.9"]);
    }
}
