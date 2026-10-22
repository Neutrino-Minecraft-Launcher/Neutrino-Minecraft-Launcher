// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod instances;
mod java;

use slint::{ModelRc, VecModel};
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
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

fn open_url(url: &str) -> std::io::Result<()> {
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
