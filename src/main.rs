// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod instances;
mod java;
mod msa;

use slint::{ModelRc, VecModel};
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

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
