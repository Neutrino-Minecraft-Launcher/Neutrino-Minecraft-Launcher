//! 版本安装主流程：解析清单 → 解析依赖 → 下载校验 → 解压 natives → 落盘。
//!
//! 一组约定：
//! - 所有路径都由 [`GamePaths`] 推导，和官方启动器目录结构完全一致；
//! - 写文件一律“下载到 `.part` → 校验 SHA-1 → rename”，中途失败不会留下半截文件；
//! - 已存在且校验通过的文件直接跳过，所以重复执行是幂等的（断点续传的效果）。

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::error::{CoreError, Result, io_err};
use super::manifest::{
    AssetIndex, AssetIndexRef, MavenCoordinate, VersionJson, VersionManifest, maven_url,
    natives_classifier, rules_allow,
};
use super::natives;
use super::net::{DownloadJob, DownloadStats, Downloader};
use super::paths::GamePaths;
use super::progress::Progress;
use super::{DEFAULT_MANIFEST_URL, LIBRARIES_BASE_URL, RESOURCES_BASE_URL};

/// 安装参数。
#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// `.minecraft` 根目录。
    pub game_dir: PathBuf,
    /// 要安装的版本 id，例如 `1.20.4`。
    pub version: String,
    /// 版本清单地址（换镜像时改这里）。
    pub manifest_url: String,
    /// 资源文件下载根地址。
    pub resources_base: String,
    /// 依赖库 Maven 根地址。
    pub libraries_base: String,
    /// 是否下载资源文件（assets，几百 MB；关掉可以只装“能启动的最小集合”）。
    pub assets: bool,
    /// 是否为 1.6 及更早的版本建立 virtual 资源目录。
    pub virtual_assets: bool,
    /// 是否解压旧版本的 natives。
    pub extract_natives: bool,
    /// 并发下载线程数。
    pub threads: usize,
    /// 单个文件失败重试次数。
    pub retries: u32,
    /// 只统计不下盘（用于预演体积）。
    pub dry_run: bool,
}

impl InstallOptions {
    /// 用默认的 Mojang 官方源创建参数。
    pub fn new(game_dir: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            game_dir: game_dir.into(),
            version: version.into(),
            manifest_url: DEFAULT_MANIFEST_URL.to_string(),
            resources_base: RESOURCES_BASE_URL.to_string(),
            libraries_base: LIBRARIES_BASE_URL.to_string(),
            assets: true,
            virtual_assets: false,
            extract_natives: true,
            threads: 8,
            retries: 3,
            dry_run: false,
        }
    }
}

/// 安装结果。
#[derive(Debug, Clone)]
pub struct InstallReport {
    pub version: String,
    pub game_dir: PathBuf,
    pub version_dir: PathBuf,
    pub client_jar: Option<PathBuf>,
    pub main_class: String,
    /// 需要下载的依赖库数量（不含 natives）。
    pub library_count: usize,
    /// 需要解压的 natives jar 数量。
    pub native_library_count: usize,
    /// 实际解压出的 natives 文件数量。
    pub native_file_count: usize,
    /// 资源文件数量。
    pub asset_count: usize,
    /// 依赖库计划下载字节数。
    pub library_bytes: u64,
    /// 资源文件计划下载字节数。
    pub asset_bytes: u64,
    pub library_stats: DownloadStats,
    /// 该版本要求的 Java 大版本。
    pub java_required: Option<u32>,
    /// 本机检测到的 Java 大版本。
    pub java_detected: Option<i32>,
    pub elapsed: Duration,
    pub dry_run: bool,
}

impl InstallReport {
    /// 本机 Java 是否满足该版本要求；信息不足时返回 `None`。
    pub fn java_ok(&self) -> Option<bool> {
        match (self.java_required, self.java_detected) {
            (Some(required), Some(detected)) => Some(detected >= required as i32),
            _ => None,
        }
    }

    /// 该版本目录下的 natives 目录。
    pub fn natives_dir(&self) -> PathBuf {
        self.version_dir.join("natives")
    }
}

/// 执行安装。
pub fn install(options: &InstallOptions, progress: &Progress) -> Result<InstallReport> {
    let started = Instant::now();
    let paths = GamePaths::new(&options.game_dir);
    let downloader = Downloader::new(progress.clone()).with_retries(options.retries);

    // 1. 版本清单
    progress.begin_stage("获取版本清单", 0);
    let manifest: VersionManifest = downloader.get_json(&options.manifest_url, "版本清单")?;
    let summary = manifest
        .find(&options.version)
        .cloned()
        .ok_or_else(|| CoreError::UnknownVersion(options.version.clone()))?;
    progress.finish_stage();

    // 2. 版本 JSON（支持 inheritsFrom 继承链）
    progress.begin_stage("获取版本信息", 0);
    let fetched = resolve_version(&downloader, &manifest, &summary.url, 0)?;
    let version = fetched.json;
    progress.finish_stage();

    if version.main_class.is_empty() {
        return Err(CoreError::MissingField("mainClass"));
    }

    // 3. 落盘版本 JSON（原样保存，便于以后启动时复读）
    let version_dir = paths.version_dir(&options.version);
    let version_json_path = paths.version_json(&options.version);
    if !options.dry_run {
        fs::create_dir_all(&version_dir).map_err(|err| io_err(&version_dir, err))?;
        fs::write(&version_json_path, &fetched.raw)
            .map_err(|err| io_err(&version_json_path, err))?;
    }

    // 4. 客户端 jar
    let client_size = version
        .downloads
        .client
        .as_ref()
        .map(|client| client.size.unwrap_or(0))
        .unwrap_or(0);
    progress.begin_stage("下载客户端", client_size);

    let mut client_jar = None;
    if let Some(client) = &version.downloads.client {
        let dest = paths.client_jar(&options.version);
        if !options.dry_run {
            downloader.download(
                &client.url,
                &dest,
                client.sha1.as_deref(),
                client.size.unwrap_or(0),
            )?;
        }
        client_jar = Some(dest);
    }
    progress.finish_stage();

    // 5. 依赖库
    let mut library_jobs = Vec::new();
    let mut native_jars = Vec::new();
    collect_library_jobs(
        &paths,
        &version,
        &options.libraries_base,
        &mut library_jobs,
        &mut native_jars,
    );

    let library_bytes = DownloadJob::total_size(&library_jobs);
    progress.begin_stage("下载依赖库", library_bytes);
    let library_stats = if options.dry_run {
        DownloadStats::default()
    } else {
        downloader.download_all(&library_jobs, options.threads)?
    };
    progress.finish_stage();

    // 6. natives（仅 1.18 及更早版本）
    let mut native_file_count = 0;
    if options.extract_natives && !native_jars.is_empty() {
        // 进度按 jar 的字节数统计，和前面几个阶段的单位保持一致
        let natives_bytes: u64 = native_jars
            .iter()
            .filter_map(|(jar, _)| jar.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        progress.begin_stage("解压 natives", natives_bytes);

        let natives_dir = paths.natives_dir(&options.version);
        for (jar, excludes) in &native_jars {
            let size = jar.metadata().map(|meta| meta.len()).unwrap_or(0);
            if !options.dry_run && jar.is_file() {
                native_file_count += natives::extract_natives(jar, &natives_dir, excludes)?;
            }
            progress.advance(size);
        }
        progress.finish_stage();
    }

    // 7. 资源文件
    let mut asset_count = 0;
    let mut asset_bytes = 0;
    if options.assets
        && let Some(index_ref) = &version.asset_index
    {
        progress.begin_stage("下载资源索引", 0);
        let index = load_asset_index(&downloader, &paths, index_ref, options.dry_run)?;
        progress.finish_stage();

        let asset_jobs = build_asset_jobs(&paths, &index, &options.resources_base);
        asset_count = asset_jobs.len();
        asset_bytes = DownloadJob::total_size(&asset_jobs);

        progress.begin_stage("下载资源文件", asset_bytes);
        if !options.dry_run {
            downloader.download_all(&asset_jobs, options.threads)?;
        }
        progress.finish_stage();

        if options.virtual_assets
            && (index.virtual_assets || index.map_to_resources)
            && !options.dry_run
        {
            copy_virtual_assets(&paths, &index, &index_ref.id, progress)?;
        }
    }

    let java_required = version
        .java_version
        .as_ref()
        .map(|java| java.major_version)
        .filter(|major| *major > 0);
    let java_detected = crate::java::detect_java().map(|info| info.major);

    Ok(InstallReport {
        version: options.version.clone(),
        game_dir: paths.root().to_path_buf(),
        version_dir,
        client_jar,
        main_class: version.main_class,
        library_count: library_jobs.len(),
        native_library_count: native_jars.len(),
        native_file_count,
        asset_count,
        library_bytes,
        asset_bytes,
        library_stats,
        java_required,
        java_detected,
        elapsed: started.elapsed(),
        dry_run: options.dry_run,
    })
}

/// 已下载但未合并的版本 JSON。
struct FetchedVersion {
    json: VersionJson,
    /// 原始字节，落盘时原样写出，避免丢失未建模的字段。
    raw: Vec<u8>,
}

fn fetch_version(downloader: &Downloader, url: &str) -> Result<FetchedVersion> {
    let raw = downloader.get_bytes(url)?;
    let json = serde_json::from_slice(&raw).map_err(|source| CoreError::Json {
        context: "版本信息 JSON".to_string(),
        source,
    })?;
    Ok(FetchedVersion { json, raw })
}

/// 解析版本 JSON，若有 `inheritsFrom` 则递归拉取父版本并合并（Forge / Fabric 依赖此机制）。
fn resolve_version(
    downloader: &Downloader,
    manifest: &VersionManifest,
    url: &str,
    depth: usize,
) -> Result<FetchedVersion> {
    if depth > 8 {
        return Err(CoreError::Other("版本继承链过深（超过 8 层）".to_string()));
    }

    let fetched = fetch_version(downloader, url)?;
    let Some(parent_id) = fetched.json.inherits_from.clone() else {
        return Ok(fetched);
    };

    let parent_summary = manifest
        .find(&parent_id)
        .cloned()
        .ok_or_else(|| CoreError::UnknownVersion(parent_id.clone()))?;
    let parent = resolve_version(downloader, manifest, &parent_summary.url, depth + 1)?;

    Ok(FetchedVersion {
        json: merge_with_parent(fetched.json, parent.json),
        raw: fetched.raw,
    })
}

/// 把父版本的信息补进子版本。
///
/// 只处理安装需要的字段；`arguments` 的深度合并留到启动器阶段再做。
fn merge_with_parent(mut child: VersionJson, parent: VersionJson) -> VersionJson {
    let mut libraries = parent.libraries;
    libraries.extend(std::mem::take(&mut child.libraries));
    child.libraries = libraries;

    if child.main_class.is_empty() {
        child.main_class = parent.main_class;
    }
    if child.assets.is_empty() {
        child.assets = parent.assets;
    }
    if child.asset_index.is_none() {
        child.asset_index = parent.asset_index;
    }
    if child.downloads.client.is_none() {
        child.downloads.client = parent.downloads.client;
    }
    if child.java_version.is_none() {
        child.java_version = parent.java_version;
    }
    if child.arguments.is_none() {
        child.arguments = parent.arguments;
    }
    if child.minecraft_arguments.is_none() {
        child.minecraft_arguments = parent.minecraft_arguments;
    }

    child
}

/// 把版本 JSON 里符合当前平台的依赖库翻译成下载任务。
fn collect_library_jobs(
    paths: &GamePaths,
    version: &VersionJson,
    libraries_base: &str,
    jobs: &mut Vec<DownloadJob>,
    native_jars: &mut Vec<(PathBuf, Vec<String>)>,
) {
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for library in &version.libraries {
        if !rules_allow(&library.rules) {
            continue;
        }

        match library.downloads.as_ref() {
            Some(downloads) => {
                if let Some(artifact) = &downloads.artifact {
                    let relative = artifact.path.clone().or_else(|| {
                        MavenCoordinate::parse(&library.name).map(|coord| coord.relative_path())
                    });

                    if let Some(relative) = relative {
                        let dest = paths.libraries_dir().join(relative);
                        if seen.insert(dest.clone()) {
                            jobs.push(DownloadJob::new(
                                &artifact.url,
                                dest,
                                artifact.sha1.clone(),
                                artifact.size.unwrap_or(0),
                            ));
                        }
                    }
                }

                // 旧版本：natives 以 classifier 形式存在，需要单独下载并解压
                if let Some(classifier) = natives_classifier(library)
                    && let Some(artifact) = downloads.classifiers.get(&classifier)
                {
                    let relative = artifact.path.clone().or_else(|| {
                        MavenCoordinate::parse(&library.name).map(|coord| coord.relative_path())
                    });

                    if let Some(relative) = relative {
                        let dest = paths.libraries_dir().join(relative);
                        if seen.insert(dest.clone()) {
                            jobs.push(DownloadJob::new(
                                &artifact.url,
                                dest.clone(),
                                artifact.sha1.clone(),
                                artifact.size.unwrap_or(0),
                            ));
                        }

                        let excludes = library
                            .extract
                            .as_ref()
                            .map(|extract| extract.exclude.clone())
                            .filter(|excludes| !excludes.is_empty())
                            .unwrap_or_else(|| vec!["META-INF/".to_string()]);
                        native_jars.push((dest, excludes));
                    }
                }
            }
            None => {
                // 极老的版本只给仓库根地址和 Maven 坐标
                if let Some(coord) = MavenCoordinate::parse(&library.name) {
                    let base = library.url.as_deref().unwrap_or(libraries_base);
                    let dest = paths.libraries_dir().join(coord.relative_path());
                    if seen.insert(dest.clone()) {
                        jobs.push(DownloadJob::new(maven_url(base, &coord), dest, None, 0));
                    }
                }
            }
        }
    }
}

fn load_asset_index(
    downloader: &Downloader,
    paths: &GamePaths,
    index_ref: &AssetIndexRef,
    dry_run: bool,
) -> Result<AssetIndex> {
    if dry_run {
        return downloader.get_json(&index_ref.url, "资源索引");
    }

    let dest = paths.asset_index_file(&index_ref.id);
    downloader.download(
        &index_ref.url,
        &dest,
        index_ref.sha1.as_deref(),
        index_ref.size.unwrap_or(0),
    )?;

    let bytes = fs::read(&dest).map_err(|err| io_err(&dest, err))?;
    serde_json::from_slice(&bytes).map_err(|source| CoreError::Json {
        context: "资源索引".to_string(),
        source,
    })
}

fn build_asset_jobs(
    paths: &GamePaths,
    index: &AssetIndex,
    resources_base: &str,
) -> Vec<DownloadJob> {
    let base = resources_base.trim_end_matches('/');

    index
        .objects
        .values()
        .filter_map(|object| {
            let prefix = object.hash.get(..2)?;
            Some(DownloadJob::new(
                format!("{base}/{prefix}/{}", object.hash),
                paths.object_path(&object.hash),
                Some(object.hash.clone()),
                object.size,
            ))
        })
        .collect()
}

/// 1.6 及更早版本需要把对象文件按资源名铺开成目录结构。
fn copy_virtual_assets(
    paths: &GamePaths,
    index: &AssetIndex,
    asset_id: &str,
    progress: &Progress,
) -> Result<()> {
    let dest_root = if index.map_to_resources {
        paths.root().join("resources")
    } else {
        paths.virtual_assets_dir(asset_id)
    };

    progress.begin_stage("建立虚拟资源目录", index.objects.len() as u64);

    for (name, object) in &index.objects {
        let source = paths.object_path(&object.hash);
        let dest = dest_root.join(name);

        if source.is_file() && !dest.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|err| io_err(parent, err))?;
            }
            fs::copy(&source, &dest).map_err(|err| io_err(&dest, err))?;
        }

        progress.advance(1);
    }

    progress.finish_stage();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::os_name;

    fn library_fixture() -> String {
        let json = r#"{
          "id": "test",
          "mainClass": "net.minecraft.client.main.Main",
          "libraries": [
            {
              "name": "com.mojang:logging:1.1.1",
              "downloads": {
                "artifact": {
                  "path": "com/mojang/logging/1.1.1/logging-1.1.1.jar",
                  "sha1": "aa",
                  "size": 5,
                  "url": "https://example.invalid/logging.jar"
                }
              }
            },
            {
              "name": "example:dummy:1.0",
              "rules": [{ "action": "disallow", "os": { "name": "__OS__" } }],
              "downloads": {
                "artifact": {
                  "path": "example/dummy/1.0/dummy-1.0.jar",
                  "sha1": "bb",
                  "size": 1,
                  "url": "https://example.invalid/dummy.jar"
                }
              }
            },
            {
              "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
              "downloads": {
                "classifiers": {
                  "natives-windows": {
                    "path": "org/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows.jar",
                    "sha1": "cc",
                    "size": 2,
                    "url": "https://example.invalid/natives-windows.jar"
                  },
                  "natives-linux": {
                    "path": "org/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-linux.jar",
                    "sha1": "dd",
                    "size": 2,
                    "url": "https://example.invalid/natives-linux.jar"
                  },
                  "natives-osx": {
                    "path": "org/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-osx.jar",
                    "sha1": "ee",
                    "size": 2,
                    "url": "https://example.invalid/natives-osx.jar"
                  }
                }
              },
              "natives": { "windows": "natives-windows", "linux": "natives-linux", "osx": "natives-osx" },
              "extract": { "exclude": ["META-INF/"] }
            },
            {
              "name": "net.old:thing:1.0",
              "url": "https://repo.example.invalid/"
            }
          ]
        }"#;

        json.replace("__OS__", os_name())
    }

    #[test]
    fn collect_jobs_filters_rules_and_adds_natives() {
        let version: VersionJson = serde_json::from_str(&library_fixture()).unwrap();
        let paths = GamePaths::new("C:/mc");

        let mut jobs = Vec::new();
        let mut natives = Vec::new();
        collect_library_jobs(
            &paths,
            &version,
            "https://libraries.minecraft.net/",
            &mut jobs,
            &mut natives,
        );

        // logging + natives jar + 老的 Maven 兜底；dummy 被 disallow 规则排除
        assert_eq!(jobs.len(), 3, "任务列表：{:?}", jobs);
        assert_eq!(natives.len(), 1);

        let logging = jobs
            .iter()
            .find(|job| job.url.ends_with("logging.jar"))
            .expect("logging 依赖应当被下载");
        assert_eq!(logging.size, 5);
        assert_eq!(logging.sha1.as_deref(), Some("aa"));

        assert!(!jobs.iter().any(|job| job.url.ends_with("dummy.jar")));

        let native = &natives[0];
        assert_eq!(native.1, vec!["META-INF/".to_string()]);
        assert!(native.0.to_string_lossy().contains("natives-"));
    }

    #[test]
    fn missing_downloads_falls_back_to_maven_url() {
        let version: VersionJson = serde_json::from_str(&library_fixture()).unwrap();
        let paths = GamePaths::new("C:/mc");

        let mut jobs = Vec::new();
        let mut natives = Vec::new();
        collect_library_jobs(
            &paths,
            &version,
            "https://libraries.minecraft.net/",
            &mut jobs,
            &mut natives,
        );

        let fallback = jobs
            .iter()
            .find(|job| job.url.contains("thing-1.0.jar"))
            .expect("没有 downloads 的库应按 Maven 坐标拼 URL");
        assert_eq!(
            fallback.url,
            "https://repo.example.invalid/net/old/thing/1.0/thing-1.0.jar"
        );
    }

    #[test]
    fn duplicate_libraries_produce_single_job() {
        // Forge 之类的版本里同一条依赖可能在父子 JSON 里重复出现
        let version: VersionJson = serde_json::from_str(
            r#"{
              "id": "dup",
              "libraries": [
                { "name": "com.mojang:logging:1.1.1",
                  "downloads": { "artifact": { "path": "com/mojang/logging/1.1.1/logging-1.1.1.jar", "url": "https://example.invalid/a.jar" } } },
                { "name": "com.mojang:logging:1.1.1",
                  "downloads": { "artifact": { "path": "com/mojang/logging/1.1.1/logging-1.1.1.jar", "url": "https://example.invalid/a.jar" } } }
              ]
            }"#,
        )
        .unwrap();

        let paths = GamePaths::new("C:/mc");
        let mut jobs = Vec::new();
        let mut natives = Vec::new();
        collect_library_jobs(
            &paths,
            &version,
            "https://libraries.minecraft.net/",
            &mut jobs,
            &mut natives,
        );

        assert_eq!(jobs.len(), 1);
        assert!(natives.is_empty());
    }

    #[test]
    fn parent_version_fills_missing_fields() {
        let parent: VersionJson = serde_json::from_str(
            r#"{
              "id": "1.20.4",
              "mainClass": "net.minecraft.client.main.Main",
              "assets": "12",
              "assetIndex": { "id": "12", "url": "https://example.invalid/12.json" },
              "downloads": { "client": { "url": "https://example.invalid/client.jar", "sha1": "aa", "size": 1 } },
              "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 17 },
              "libraries": [{ "name": "com.mojang:logging:1.1.1" }]
            }"#,
        )
        .unwrap();

        let child: VersionJson = serde_json::from_str(
            r#"{
              "id": "1.20.4-forge",
              "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
              "libraries": [{ "name": "net.minecraftforge:forge:1.0" }]
            }"#,
        )
        .unwrap();

        let merged = merge_with_parent(child, parent);

        assert_eq!(
            merged.main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(merged.assets, "12");
        assert_eq!(merged.libraries.len(), 2);
        assert_eq!(merged.libraries[0].name, "com.mojang:logging:1.1.1");
        assert_eq!(merged.java_version.unwrap().major_version, 17);
        assert_eq!(merged.downloads.client.unwrap().sha1.as_deref(), Some("aa"));
    }

    #[test]
    fn asset_jobs_point_at_hashed_objects() {
        let index: AssetIndex = serde_json::from_str(
            r#"{
              "objects": {
                "icons/icon_16x16.png": { "hash": "1a2b3c4d5e6f708192a3b4c5d6e7f80912345678", "size": 42 },
                "broken": { "hash": "" }
              }
            }"#,
        )
        .unwrap();

        let paths = GamePaths::new("C:/mc");
        let jobs = build_asset_jobs(&paths, &index, "https://resources.download.minecraft.net");

        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].url,
            "https://resources.download.minecraft.net/1a/1a2b3c4d5e6f708192a3b4c5d6e7f80912345678"
        );
        assert_eq!(jobs[0].size, 42);
    }

    #[test]
    fn java_ok_compares_major_versions() {
        let mut report = InstallReport {
            version: "1.20.4".to_string(),
            game_dir: PathBuf::from("C:/mc"),
            version_dir: PathBuf::from("C:/mc/versions/1.20.4"),
            client_jar: None,
            main_class: "a.B".to_string(),
            library_count: 0,
            native_library_count: 0,
            native_file_count: 0,
            asset_count: 0,
            library_bytes: 0,
            asset_bytes: 0,
            library_stats: DownloadStats::default(),
            java_required: Some(17),
            java_detected: Some(21),
            elapsed: Duration::from_secs(1),
            dry_run: false,
        };

        assert_eq!(report.java_ok(), Some(true));
        report.java_detected = Some(8);
        assert_eq!(report.java_ok(), Some(false));
        report.java_detected = None;
        assert_eq!(report.java_ok(), None);
    }
}
