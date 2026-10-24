// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod image_util;
mod instances;
mod java;
mod msa;
mod skins;

use image::RgbaImage;
use neutrino_minecraft_launcher::core;
use slint::{Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

mod desktop;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();

    let ui = AppWindow::new()?;

    let game_dir = core::default_game_dir();
    let game_dir_text = game_dir.display().to_string();
    ui.global::<AppState>()
        .set_game_dir(game_dir_text.clone().into());

    let instances = instances::detect_instances(&game_dir_text)
        .into_iter()
        .map(|instance| InstanceInfo {
            name: instance.name.into(),
            path: instance.path.into(),
        })
        .collect::<Vec<_>>();

    ui.global::<AppState>()
        .set_instances(ModelRc::new(VecModel::from(instances)));

    let account_store = Rc::new(RefCell::new(accounts::Store::load_default()));
    update_account_state(&ui, &account_store.borrow());

    if let Some(info) = java::detect_java() {
        ui.global::<AppState>().set_java_info(JavaInfo {
            version: info.version.into(),
            major: info.major,
            lts: info.lts,
            vendor: info.vendor.into(),
            path: info.path.into(),
        });
    }

    ui.global::<Actions>().on_open_folder(|path| {
        if let Err(err) = open_folder(path.as_str()) {
            eprintln!("Failed to open folder: {err}");
        }
    });

    ui.global::<Actions>().on_open_url(|url| {
        if let Err(err) = open_url(url.as_str()) {
            eprintln!("Failed to open URL: {err}");
        }
    });

    let launch_handle = ui.as_weak();
    let launch_store = account_store.clone();
    ui.global::<Actions>().on_launch_instance(move |name| {
        match launch_game(&launch_store, name.as_str()) {
            Ok((message, mut child)) => {
                if let Some(ui) = launch_handle.upgrade() {
                    let state = ui.global::<AppState>();
                    state.set_launch_ok(true);
                    state.set_launch_message(message.into());
                }

                // 后台等待游戏退出，退出后清掉“已启动”提示。
                let weak = launch_handle.clone();
                std::thread::spawn(move || {
                    let _ = child.wait();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() {
                            let state = ui.global::<AppState>();
                            state.set_launch_ok(false);
                            state.set_launch_message("".into());
                        }
                    });
                });
            }
            Err(error) => {
                if let Some(ui) = launch_handle.upgrade() {
                    let state = ui.global::<AppState>();
                    state.set_launch_ok(false);
                    state.set_launch_message(error.into());
                }
            }
        }
    });

    // 实例内容（模组 / 资源包 / 光影）全部在后台线程扫描和解码，
    // UI 线程只负责把结果换成模型，点击实例不会再卡住窗口。
    let content_loader = Arc::new(InstanceContentLoader::default());
    ui.global::<Actions>().on_load_instance_content({
        let loader = content_loader.clone();
        let weak = ui.as_weak();
        move |path| loader.load(&weak, path.as_str())
    });

    let add_handle = ui.as_weak();
    let add_store = account_store.clone();
    ui.global::<Actions>().on_add_offline_account(move |name| {
        let mut store = add_store.borrow_mut();
        match store.add_offline(name.as_str()) {
            Ok(()) => {
                if let Some(ui) = add_handle.upgrade() {
                    update_account_state(&ui, &store);
                }
                "".into()
            }
            Err(error) => error.into(),
        }
    });

    let remove_handle = ui.as_weak();
    let remove_store = account_store.clone();
    ui.global::<Actions>().on_remove_account(move |name| {
        let mut store = remove_store.borrow_mut();
        if store.remove(name.as_str()).is_ok()
            && let Some(ui) = remove_handle.upgrade()
        {
            update_account_state(&ui, &store);
        }
    });

    let active_handle = ui.as_weak();
    let active_store = account_store.clone();
    ui.global::<Actions>().on_set_active_account(move |name| {
        let mut store = active_store.borrow_mut();
        if store.set_active(name.as_str()).is_ok()
            && let Some(ui) = active_handle.upgrade()
        {
            update_account_state(&ui, &store);
        }
    });

    let skin_handle = ui.as_weak();
    let skin_store = account_store.clone();
    ui.global::<Actions>()
        .on_set_default_skin(move |name, skin| {
            let skin = match skin {
                SkinType::Steve => accounts::Skin::Steve,
                SkinType::Alex => accounts::Skin::Alex,
                SkinType::Custom => return,
            };

            let mut store = skin_store.borrow_mut();
            if store.set_skin(name.as_str(), skin).is_ok()
                && let Some(ui) = skin_handle.upgrade()
            {
                update_account_state(&ui, &store);
            }
        });

    let custom_handle = ui.as_weak();
    let custom_store = account_store.clone();
    ui.global::<Actions>().on_choose_custom_skin(move |name| {
        match accounts::pick_custom_skin(name.as_str()) {
            Ok(Some(skin)) => {
                let mut store = custom_store.borrow_mut();
                if store.set_skin(name.as_str(), skin).is_ok()
                    && let Some(ui) = custom_handle.upgrade()
                {
                    update_account_state(&ui, &store);
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("选择自定义皮肤失败：{error}"),
        }
    });

    let ui_handle = ui.as_weak();
    ui.global::<Actions>().on_start_microsoft_login(move || {
        let weak = ui_handle.clone();

        if let Some(ui) = weak.upgrade() {
            let status = ui.global::<MsaState>().get_status();
            if status == MsaStatus::Starting || status == MsaStatus::Waiting {
                return;
            }

            let state = ui.global::<MsaState>();
            state.set_status(MsaStatus::Starting);
            state.set_error("".into());
        } else {
            return;
        }

        let client_id = match msa::client_id() {
            Ok(client_id) => client_id,
            Err(error) => {
                if let Some(ui) = weak.upgrade() {
                    let state = ui.global::<MsaState>();
                    state.set_status(MsaStatus::MissingClientId);
                    state.set_error(error.into());
                }
                return;
            }
        };

        std::thread::spawn(move || {
            let weak = weak.clone();
            let _ = msa::login(client_id, move |event| {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = weak.upgrade() else {
                        return;
                    };

                    let state = ui.global::<MsaState>();
                    match event {
                        msa::LoginEvent::DeviceCode {
                            verification_uri,
                            user_code,
                        } => {
                            state.set_status(MsaStatus::Waiting);
                            state.set_verification_uri(verification_uri.into());
                            state.set_user_code(user_code.into());
                        }
                        msa::LoginEvent::Success { player_name } => {
                            state.set_status(MsaStatus::Success);
                            state.set_player_name(player_name.into());
                        }
                        msa::LoginEvent::Error(error) => {
                            state.set_status(MsaStatus::Error);
                            state.set_error(error.into());
                        }
                    }
                });
            });
        });
    });

    // 发现页：版本列表与安装
    let controller = desktop::InstallController::new(ui.as_weak());

    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_refresh_versions(move || controller.refresh_versions());
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_filter_versions(move |key| controller.apply_filter(key.as_str()));
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_reset_add_instance(move || controller.reset_add_instance());
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_choose_version(move |version| controller.choose_version(version.as_str()));
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_choose_loader(move |key| controller.choose_loader(key.as_str()));
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_choose_loader_version(move |version| {
                controller.choose_loader_version(version.as_str())
            });
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_go_back(move || controller.go_back());
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_create_instance(move |name| controller.create_instance(name.as_str()));
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_open_category(move |key| controller.open_category(key.as_str()));
    }
    {
        let controller = controller.clone();
        ui.global::<Actions>()
            .on_download_project(move |id| controller.download_project(id.as_str()));
    }

    // 启动时后台拉一次版本清单，供“实例页 → 添加实例”使用
    controller.refresh_versions();

    // 发现页默认展示模组列表，启动时也预取一次
    controller.open_category("mod");

    ui.run()?;

    Ok(())
}

/// 后台读取实例内容：扫描目录、解 zip、解码图标都不在 UI 线程做。
///
/// 每次点击分配一个 generation，回写模型前检查它是否还是最新的，
/// 这样快速切换实例时旧结果会被丢弃，不会盖掉新实例的列表。
#[derive(Default)]
struct InstanceContentLoader {
    generation: AtomicU64,
    cache: Mutex<HashMap<PathBuf, CachedArchive>>,
}

/// 已经解析过的 jar / zip 信息，按文件大小 + 修改时间判断是否还有效。
struct CachedArchive {
    len: u64,
    modified: Option<SystemTime>,
    version: String,
    loader: String,
    icon: Option<Arc<RgbaImage>>,
}

/// 后台线程准备好的行数据；`icon` 是已解码的 RGBA，UI 线程只做一次拷贝。
struct PreparedFile {
    name: String,
    size: u64,
    enabled: bool,
    version: String,
    loader: String,
    icon: Option<Arc<RgbaImage>>,
}

#[derive(Default)]
struct PreparedInstance {
    mods: Vec<PreparedFile>,
    resource_packs: Vec<PreparedFile>,
    shader_packs: Vec<PreparedFile>,
}

impl InstanceContentLoader {
    /// 最多缓存多少个文件的信息；超出后整体清空，避免长时间使用后无限增长。
    const CACHE_LIMIT: usize = 1024;

    fn load(self: &Arc<Self>, ui: &slint::Weak<AppWindow>, path: &str) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        if path.is_empty() {
            // 回到“全部实例”：直接清空列表，不能拿空路径去扫进程工作目录。
            self.publish(ui, generation, PreparedInstance::default());
            return;
        }

        let this = self.clone();
        let weak = ui.clone();
        let path = path.to_string();

        std::thread::spawn(move || {
            let content = instances::detect_instance_content(&path);

            // 第一段：只带名称 / 大小 / 启用状态，先让列表立刻显示出来。
            if this.is_stale(generation) {
                return;
            }
            this.publish(&weak, generation, PreparedInstance::quick(&content));

            // 第二段：解开 jar 读版本 / 加载器 / 图标，结果到了再补一次。
            let full = PreparedInstance::full(&content, Path::new(&path), &this.cache);
            if !full.has_details() || this.is_stale(generation) {
                return;
            }
            this.publish(&weak, generation, full);
        });
    }

    fn is_stale(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != generation
    }

    fn publish(
        self: &Arc<Self>,
        ui: &slint::Weak<AppWindow>,
        generation: u64,
        prepared: PreparedInstance,
    ) {
        let this = self.clone();
        let weak = ui.clone();

        let _ = weak.upgrade_in_event_loop(move |ui| {
            if this.is_stale(generation) {
                return;
            }

            let state = ui.global::<AppState>();
            apply_files(state.get_mods(), prepared.mods, |model| {
                state.set_mods(model)
            });
            apply_files(
                state.get_resource_packs(),
                prepared.resource_packs,
                |model| state.set_resource_packs(model),
            );
            apply_files(state.get_shader_packs(), prepared.shader_packs, |model| {
                state.set_shader_packs(model)
            });
        });
    }
}

impl PreparedInstance {
    /// 扫目录就能拿到的部分（毫秒级）。
    fn quick(content: &instances::InstanceContent) -> Self {
        Self {
            mods: quick_files(&content.mods),
            resource_packs: quick_files(&content.resource_packs),
            shader_packs: quick_files(&content.shader_packs),
        }
    }

    /// 需要打开压缩包才能拿到的部分（版本号 / 加载器 / 图标）。
    fn full(
        content: &instances::InstanceContent,
        root: &Path,
        cache: &Mutex<HashMap<PathBuf, CachedArchive>>,
    ) -> Self {
        Self {
            mods: detailed_files(&content.mods, &root.join("mods"), cache),
            resource_packs: detailed_files(
                &content.resource_packs,
                &root.join("resourcepacks"),
                cache,
            ),
            shader_packs: detailed_files(&content.shader_packs, &root.join("shaderpacks"), cache),
        }
    }

    fn has_details(&self) -> bool {
        self.mods
            .iter()
            .chain(&self.resource_packs)
            .chain(&self.shader_packs)
            .any(|file| file.icon.is_some() || !file.version.is_empty() || !file.loader.is_empty())
    }
}

fn quick_files(files: &[instances::ContentFile]) -> Vec<PreparedFile> {
    files
        .iter()
        .map(|file| PreparedFile {
            name: file.name.clone(),
            size: file.size,
            enabled: file.enabled,
            version: String::new(),
            loader: String::new(),
            icon: None,
        })
        .collect()
}

fn detailed_files(
    files: &[instances::ContentFile],
    folder: &Path,
    cache: &Mutex<HashMap<PathBuf, CachedArchive>>,
) -> Vec<PreparedFile> {
    files
        .iter()
        .map(|file| {
            let storage_name = if file.enabled {
                file.name.clone()
            } else {
                format!("{}.disabled", file.name)
            };
            let (version, loader, icon) = read_archive_cached(&folder.join(storage_name), cache);

            PreparedFile {
                name: file.name.clone(),
                size: file.size,
                enabled: file.enabled,
                version,
                loader,
                icon,
            }
        })
        .collect()
}

/// 读 jar / zip 的附加信息，命中缓存时直接返回，切回同一个实例不会重新解压。
fn read_archive_cached(
    path: &Path,
    cache: &Mutex<HashMap<PathBuf, CachedArchive>>,
) -> (String, String, Option<Arc<RgbaImage>>) {
    let metadata = std::fs::metadata(path).ok();
    let len = metadata
        .as_ref()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    let modified = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok());

    if let Ok(cache) = cache.lock()
        && let Some(hit) = cache.get(path)
        && hit.len == len
        && hit.modified == modified
    {
        return (hit.version.clone(), hit.loader.clone(), hit.icon.clone());
    }

    let info = instances::read_archive_info(path);
    let icon = info
        .icon
        .as_deref()
        .and_then(image_util::decode_rgba)
        .map(Arc::new);
    let entry = CachedArchive {
        len,
        modified,
        version: info.version,
        loader: info.loader,
        icon,
    };
    let result = (
        entry.version.clone(),
        entry.loader.clone(),
        entry.icon.clone(),
    );

    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= InstanceContentLoader::CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), entry);
    }

    result
}

/// 行数没变时原地改每一行，避免第二次回填把列表滚动位置和悬停状态重置。
fn apply_files(
    existing: ModelRc<ContentFile>,
    files: Vec<PreparedFile>,
    replace: impl FnOnce(ModelRc<ContentFile>),
) {
    let rows = files
        .into_iter()
        .map(to_ui_content_file)
        .collect::<Vec<_>>();

    if let Some(model) = existing.as_any().downcast_ref::<VecModel<ContentFile>>()
        && model.row_count() == rows.len()
    {
        for (index, row) in rows.into_iter().enumerate() {
            model.set_row_data(index, row);
        }
        return;
    }

    replace(ModelRc::new(VecModel::from(rows)));
}

fn to_ui_content_file(file: PreparedFile) -> ContentFile {
    let icon = file.icon.map(|image| image_util::from_rgba(&image));
    let has_icon = icon.is_some();

    ContentFile {
        name: file.name.into(),
        size: i32::try_from(file.size).unwrap_or(i32::MAX),
        enabled: file.enabled,
        icon: icon.unwrap_or_default(),
        has_icon,
        version: file.version.into(),
        loader: file.loader.into(),
    }
}

fn update_account_state(ui: &AppWindow, store: &accounts::Store) {
    let accounts = store
        .accounts()
        .iter()
        .map(|account| {
            let (skin, avatar) = match &account.skin {
                accounts::Skin::Steve => (SkinType::Steve, slint::Image::default()),
                accounts::Skin::Alex => (SkinType::Alex, slint::Image::default()),
                accounts::Skin::Custom(path) => (SkinType::Custom, skins::custom_avatar(path)),
            };

            AccountInfo {
                name: account.name.clone().into(),
                kind: match account.kind {
                    accounts::AccountKind::Microsoft => AccountKind::Microsoft,
                    accounts::AccountKind::Offline => AccountKind::Offline,
                },
                skin,
                avatar,
            }
        })
        .collect::<Vec<_>>();

    let state = ui.global::<AppState>();
    state.set_accounts(ModelRc::new(VecModel::from(accounts)));
    state.set_active_account(store.active().unwrap_or_default().into());
}

fn launch_game(
    store: &Rc<RefCell<accounts::Store>>,
    version: &str,
) -> Result<(String, std::process::Child), String> {
    let account_name = store
        .borrow()
        .active()
        .map(str::to_string)
        .ok_or_else(|| "请先在账户页选择一个角色".to_string())?;

    let java = java::detect_java().ok_or_else(|| "未检测到 Java 运行时".to_string())?;
    let executable = java.executable();
    if !executable.is_file() {
        return Err(format!("找不到 java 可执行文件：{}", executable.display()));
    }

    if let Some(required) = required_java_major(version)
        && java.major < required as i32
    {
        eprintln!(
            "警告：{version} 需要 Java {required}，当前检测到 Java {}",
            java.major
        );
    }

    let account = core::launch::LaunchAccount::offline(&account_name);
    let options = core::launch::LaunchOptions::new(
        core::default_game_dir(),
        version.to_string(),
        account,
        executable,
    );

    let child = core::launch::spawn(&options).map_err(|error| error.to_string())?;
    Ok((format!("已启动 {version}（PID {}）", child.id()), child))
}

fn required_java_major(version: &str) -> Option<u32> {
    let path = core::GamePaths::new(core::default_game_dir()).version_json(version);
    let bytes = std::fs::read(path).ok()?;
    let json: core::VersionJson = serde_json::from_slice(&bytes).ok()?;
    json.java_version
        .map(|java| java.major_version)
        .filter(|major| *major > 0)
}

fn open_folder(path: &str) -> std::io::Result<()> {
    if path.is_empty() {
        return Ok(());
    }

    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };

    Command::new(opener).arg(path).spawn()?;
    Ok(())
}

pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unsupported URL scheme",
        ));
    }

    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };

    command.arg(url).spawn()?;
    Ok(())
}
