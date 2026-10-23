// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod instances;
mod java;
mod msa;
mod skins;

use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();

    let ui = AppWindow::new()?;

    let game_dir = official_game_dir();
    ui.global::<AppState>()
        .set_game_dir(game_dir.clone().into());

    let instances = instances::detect_instances(&game_dir)
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

    let content_handle = ui.as_weak();
    ui.global::<Actions>()
        .on_load_instance_content(move |path| {
            let Some(ui) = content_handle.upgrade() else {
                return;
            };

            let content = instances::detect_instance_content(path.as_str());
            let state = ui.global::<AppState>();
            state.set_mods(ModelRc::new(VecModel::from(to_ui_content_files(
                content.mods,
            ))));
            state.set_resource_packs(ModelRc::new(VecModel::from(to_ui_content_files(
                content.resource_packs,
            ))));
            state.set_shader_packs(ModelRc::new(VecModel::from(to_ui_content_files(
                content.shader_packs,
            ))));
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

    ui.run()?;

    Ok(())
}

fn to_ui_content_files(files: Vec<instances::ContentFile>) -> Vec<ContentFile> {
    files
        .into_iter()
        .map(|file| ContentFile {
            name: file.name.into(),
            size: i32::try_from(file.size).unwrap_or(i32::MAX),
            enabled: file.enabled,
        })
        .collect()
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

fn official_game_dir() -> String {
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
    .display()
    .to_string()
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
