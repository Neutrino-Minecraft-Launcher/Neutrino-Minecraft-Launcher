//! “发现”页的版本列表与安装流程（GUI 专用，CLI 不依赖）。
//!
//! 线程模型：
//! - Slint 的属性只能在 UI 线程写，所以后台线程一律通过
//!   `Weak::upgrade_in_event_loop` 把结果投递回 UI 线程；
//! - `core::Progress` 的回调可能来自多个下载线程，因此这里只捕获
//!   `InstallController`（内部全是 `Arc<Mutex<_>>`），满足 `Send + Sync`。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use neutrino_minecraft_launcher::core::{
    DEFAULT_MANIFEST_URL, Downloader, GamePaths, InstallOptions, InstallReport, Progress,
    VersionManifest, default_game_dir, install,
    modrinth::{self, ContentKind, ProjectHit},
};
use neutrino_minecraft_launcher::instances;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppState, AppWindow, InstanceInfo, ProjectInfo, VersionInfo};

/// 版本列表的筛选条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Release,
    Snapshot,
    All,
}

impl Filter {
    fn parse(key: &str) -> Self {
        match key {
            "snapshot" => Self::Snapshot,
            "all" => Self::All,
            _ => Self::Release,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Release => "正式版",
            Self::Snapshot => "快照版",
            Self::All => "版本",
        }
    }

    fn matches(self, kind: &str) -> bool {
        match self {
            Self::Release => kind == "release",
            Self::Snapshot => kind == "snapshot",
            Self::All => true,
        }
    }
}

/// 发现页的控制器，可在多个回调之间共享。
#[derive(Clone)]
pub struct InstallController {
    ui: Arc<Mutex<slint::Weak<AppWindow>>>,
    manifest: Arc<Mutex<Option<VersionManifest>>>,
    filter: Arc<Mutex<Filter>>,
    /// 当前发现页类别；None 表示"原版游戏"
    content: Arc<Mutex<Option<ContentKind>>>,
}

impl InstallController {
    /// 绑定窗口。
    pub fn new(ui: slint::Weak<AppWindow>) -> Self {
        Self {
            ui: Arc::new(Mutex::new(ui)),
            manifest: Arc::new(Mutex::new(None)),
            filter: Arc::new(Mutex::new(Filter::Release)),
            content: Arc::new(Mutex::new(None)),
        }
    }

    /// 从网络拉取版本清单（后台线程），成功后重建列表。
    pub fn refresh_versions(&self) {
        // 回调都在 UI 线程触发，这里的 upgrade 是安全的
        if self.loading() {
            return;
        }

        self.on_ui(|ui| {
            let state = ui.global::<AppState>();
            state.set_versions_loading(true);
            state.set_versions_error("".into());
        });

        let this = self.clone();
        std::thread::spawn(move || {
            let downloader = Downloader::new(Progress::silent());
            match downloader.get_json::<VersionManifest>(DEFAULT_MANIFEST_URL, "版本清单") {
                Ok(manifest) => {
                    if let Ok(mut cache) = this.manifest.lock() {
                        *cache = Some(manifest);
                    }
                    this.rebuild_from_cache();
                }
                Err(err) => {
                    this.on_ui(move |ui| {
                        let state = ui.global::<AppState>();
                        state.set_versions_loading(false);
                        state.set_versions_error(format!("获取版本列表失败：{err}").into());
                    });
                }
            }
        });
    }

    /// 切换筛选条件（只用缓存重建，不联网）。
    pub fn apply_filter(&self, key: &str) {
        if let Ok(mut filter) = self.filter.lock() {
            *filter = Filter::parse(key);
        }
        self.rebuild_from_cache();
    }

    /// 切换发现页类别；"vanilla" 走版本列表，其余走 Modrinth。
    pub fn open_category(&self, key: &str) {
        let kind = ContentKind::parse(key);
        if let Ok(mut content) = self.content.lock() {
            *content = kind;
        }

        let category = if kind.is_some() {
            key.to_string()
        } else {
            "vanilla".to_string()
        };

        self.on_ui(move |ui| {
            let state = ui.global::<AppState>();
            state.set_category(category.into());
            state.set_download_message("".into());
            state.set_projects_error("".into());
            state.set_install_message("".into());
        });

        match kind {
            Some(kind) => self.load_projects(kind),
            None => self.rebuild_from_cache(),
        }
    }

    /// 从 Modrinth 拉取某个类别的项目列表。
    fn load_projects(&self, kind: ContentKind) {
        self.on_ui(|ui| {
            let state = ui.global::<AppState>();
            state.set_projects_loading(true);
            state.set_projects_summary("".into());
            // 先清空，避免短暂看到上一个类别的数据
            state.set_projects(ModelRc::new(VecModel::from(Vec::<ProjectInfo>::new())));
        });

        let this = self.clone();
        std::thread::spawn(move || {
            let downloader = Downloader::new(Progress::silent());
            match modrinth::search(&downloader, kind, 30) {
                Ok(response) => {
                    let items: Vec<ProjectInfo> = response
                        .hits
                        .iter()
                        .map(|hit| to_project_info(hit, kind))
                        .collect();
                    let summary = format!(
                        "Modrinth 共 {} 个{}",
                        short_number(response.total_hits),
                        content_label(kind)
                    );

                    this.on_ui(move |ui| {
                        let state = ui.global::<AppState>();
                        state.set_projects(ModelRc::new(VecModel::from(items)));
                        state.set_projects_summary(summary.into());
                        state.set_projects_loading(false);
                        state.set_projects_error("".into());
                    });
                }
                Err(err) => {
                    this.on_ui(move |ui| {
                        let state = ui.global::<AppState>();
                        state.set_projects_loading(false);
                        state.set_projects_error(
                            format!("获取{}失败：{err}", content_label(kind)).into(),
                        );
                    });
                }
            }
        });
    }

    /// 下载（安装）某个项目：模组 / 光影 / 材质 存到对应目录。
    pub fn download_project(&self, project_id: &str) {
        let Some(kind) = self.content.lock().ok().and_then(|guard| *guard) else {
            return;
        };

        if !kind.supports_install() {
            self.on_ui(|ui| {
                let state = ui.global::<AppState>();
                state.set_download_ok(false);
                state.set_download_message("整合包一键安装还没做，暂时只能浏览".into());
            });
            return;
        }

        // 同一时间只允许一个下载任务
        if self
            .with_ui(|ui| ui.global::<AppState>().get_downloading_project())
            .is_some_and(|id| !id.is_empty())
        {
            return;
        }

        let project = project_id.to_string();
        {
            let project = project.clone();
            self.on_ui(move |ui| {
                let state = ui.global::<AppState>();
                state.set_downloading_project(project.into());
                state.set_download_message("".into());
            });
        }

        let this = self.clone();
        std::thread::spawn(move || {
            let (ok, message) = match download_latest_file(&project, kind) {
                Ok(dest) => (
                    true,
                    format!(
                        "已下载 {} 到 {}",
                        dest.file_name().unwrap_or_default().to_string_lossy(),
                        kind.target_dir()
                    ),
                ),
                Err(err) => (false, err),
            };

            this.on_ui(move |ui| {
                let state = ui.global::<AppState>();
                state.set_downloading_project("".into());
                state.set_download_ok(ok);
                state.set_download_message(message.into());
            });
        });
    }

    /// 在后台安装指定版本。
    pub fn install(&self, version: &str) {
        let Some(ui) = self.upgrade() else {
            return;
        };

        let state = ui.global::<AppState>();
        if state.get_installing() {
            return;
        }
        state.set_installing(true);
        state.set_installing_version(version.into());
        state.set_install_progress(0.0);
        state.set_install_progress_text("0%".into());
        state.set_install_stage("准备中…".into());
        state.set_install_message("".into());
        drop(state);
        drop(ui);

        let this = self.clone();
        let target = version.to_string();

        std::thread::spawn(move || {
            let options = InstallOptions::new(default_game_dir(), target.clone());

            let progress = {
                let this = this.clone();
                Progress::new(move |snapshot| {
                    let ratio = snapshot.ratio() as f32;
                    let text = format!("{:.0}%", ratio * 100.0);
                    let stage = snapshot.stage.clone();
                    this.on_ui(move |ui| {
                        let state = ui.global::<AppState>();
                        state.set_install_progress(ratio);
                        state.set_install_progress_text(text.into());
                        state.set_install_stage(stage.into());
                    });
                })
            };

            let outcome = install(&options, &progress);
            this.finish_install(&target, outcome);
        });
    }

    fn finish_install(
        &self,
        version: &str,
        outcome: Result<InstallReport, impl std::fmt::Display>,
    ) {
        let (ok, message) = match &outcome {
            Ok(report) => (
                true,
                format!(
                    "{} 安装完成：{} 个依赖库，{} 个资源文件，用时 {:.1}s",
                    report.version,
                    report.library_count,
                    report.asset_count,
                    report.elapsed.as_secs_f64()
                ),
            ),
            Err(err) => (false, format!("{version} 安装失败：{err}")),
        };

        // 安装会往 versions/ 里写目录，顺手把实例列表刷新掉
        let game_dir = default_game_dir();
        let instance_items = to_instance_infos(&game_dir.display().to_string());
        let version_items = self.build_items();

        self.on_ui(move |ui| {
            let state = ui.global::<AppState>();
            state.set_installing(false);
            state.set_install_stage("".into());
            state.set_install_message(message.into());
            state.set_install_ok(ok);

            if ok {
                state.set_install_progress(1.0);
                state.set_install_progress_text("100%".into());
            } else {
                state.set_install_progress(0.0);
                state.set_install_progress_text("".into());
            }

            state.set_instances(ModelRc::new(VecModel::from(instance_items)));
            if let Some(items) = version_items {
                state.set_versions(ModelRc::new(VecModel::from(items)));
            }
        });
    }

    fn rebuild_from_cache(&self) {
        let filter = self
            .filter
            .lock()
            .map(|guard| *guard)
            .unwrap_or(Filter::Release);
        let Some(manifest) = self.manifest.lock().ok().and_then(|guard| guard.clone()) else {
            return;
        };

        let items = build_version_items(&manifest, filter);
        let summary = format!("共 {} 个{}", items.len(), filter.label());
        let latest_release = manifest.latest.release;
        let latest_snapshot = manifest.latest.snapshot;

        self.on_ui(move |ui| {
            let state = ui.global::<AppState>();
            state.set_versions(ModelRc::new(VecModel::from(items)));
            state.set_versions_summary(summary.into());
            state.set_latest_release(latest_release.into());
            state.set_latest_snapshot(latest_snapshot.into());
            state.set_versions_loading(false);
            state.set_versions_error("".into());
        });
    }

    fn build_items(&self) -> Option<Vec<VersionInfo>> {
        let manifest = self.manifest.lock().ok().and_then(|guard| guard.clone())?;
        let filter = self
            .filter
            .lock()
            .map(|guard| *guard)
            .unwrap_or(Filter::Release);
        Some(build_version_items(&manifest, filter))
    }

    fn loading(&self) -> bool {
        self.with_ui(|ui| ui.global::<AppState>().get_versions_loading())
            .unwrap_or(true)
    }

    fn upgrade(&self) -> Option<AppWindow> {
        self.ui.lock().ok().and_then(|guard| guard.upgrade())
    }

    fn with_ui<R>(&self, f: impl FnOnce(&AppWindow) -> R) -> Option<R> {
        let ui = self.upgrade()?;
        Some(f(&ui))
    }

    /// 把一段操作投递到 UI 线程执行。
    fn on_ui(&self, f: impl FnOnce(AppWindow) + Send + 'static) {
        let weak = match self.ui.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let _ = weak.upgrade_in_event_loop(f);
    }
}

fn build_version_items(manifest: &VersionManifest, filter: Filter) -> Vec<VersionInfo> {
    let paths = GamePaths::new(default_game_dir());

    manifest
        .versions
        .iter()
        .filter(|version| filter.matches(&version.kind))
        .map(|version| VersionInfo {
            version_id: version.id.clone().into(),
            kind: version.kind.clone().into(),
            kind_label: kind_label(&version.kind).into(),
            release_date: short_date(&version.release_time).into(),
            installed: paths.version_json(&version.id).is_file(),
        })
        .collect()
}

fn to_instance_infos(game_dir: &str) -> Vec<InstanceInfo> {
    instances::detect_instances(game_dir)
        .into_iter()
        .map(|instance| InstanceInfo {
            name: instance.name.into(),
            path: instance.path.into(),
        })
        .collect()
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "release" => "正式版",
        "snapshot" => "快照",
        "old_beta" => "Beta",
        "old_alpha" => "Alpha",
        _ => "其他",
    }
}

/// `2026-09-15T11:23:02+00:00` -> `2026-09-15`
fn short_date(release_time: &str) -> String {
    release_time.chars().take(10).collect()
}

/// 取项目最新的正式版文件并下载到对应目录，返回落盘路径。
fn download_latest_file(project_id: &str, kind: ContentKind) -> Result<PathBuf, String> {
    let downloader = Downloader::new(Progress::silent());

    let versions = modrinth::project_versions(&downloader, project_id)
        .map_err(|err| format!("获取版本信息失败：{err}"))?;
    let file = modrinth::pick_file(&versions)
        .cloned()
        .ok_or_else(|| "该项目没有可下载的文件".to_string())?;

    let dir = default_game_dir().join(kind.target_dir());
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("创建 {} 目录失败：{err}", kind.target_dir()))?;

    let dest = dir.join(&file.filename);
    downloader
        .download(&file.url, &dest, file.sha1(), file.size)
        .map_err(|err| format!("下载失败：{err}"))?;

    Ok(dest)
}

fn to_project_info(hit: &ProjectHit, kind: ContentKind) -> ProjectInfo {
    let downloadable = kind.supports_install();

    ProjectInfo {
        project_id: hit.project_id.clone().into(),
        title: hit.title.clone().into(),
        initial: first_char(&hit.title).into(),
        subtitle: format!("{} · {} 次下载", hit.author, short_number(hit.downloads)).into(),
        description: hit.description.clone().into(),
        tag: hit.categories.first().cloned().unwrap_or_default().into(),
        action_label: if downloadable {
            "下载"
        } else {
            "暂不支持"
        }
        .into(),
        downloadable,
    }
}

fn content_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mod => "模组",
        ContentKind::Shader => "光影包",
        ContentKind::ResourcePack => "材质包",
        ContentKind::ModPack => "整合包",
    }
}

/// 1 亿以上用「亿」、1 万以上用「万」，其余原样。
fn short_number(value: u64) -> String {
    if value >= 100_000_000 {
        format!("{:.1} 亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1} 万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

fn first_char(text: &str) -> String {
    text.chars()
        .next()
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}
