use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct JavaInstallation {
    pub version: String,
    pub major: i32,
    pub lts: bool,
    pub vendor: String,
    pub path: String,
}

impl JavaInstallation {
    /// `java.home` 下的 `java` / `java.exe` 可执行文件。
    pub fn executable(&self) -> PathBuf {
        Path::new(&self.path).join("bin").join(java_binary())
    }
}

pub fn detect_java() -> Option<JavaInstallation> {
    let mut seen = HashSet::new();

    for candidate in java_candidates() {
        if !candidate.is_file() {
            continue;
        }

        let candidate = candidate.canonicalize().unwrap_or(candidate);
        if !seen.insert(candidate.clone()) {
            continue;
        }

        if let Some(info) = read_java_info(&candidate) {
            return Some(info);
        }
    }

    None
}

fn java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(java_home) = env::var_os("JAVA_HOME") {
        let home = PathBuf::from(java_home);
        candidates.push(home.join("bin").join(java_binary()));
    }

    #[cfg(windows)]
    candidates.extend(windows_java_homes());

    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            candidates.push(dir.join(java_binary()));
        }
    }

    candidates
}

#[cfg(windows)]
fn windows_java_homes() -> Vec<PathBuf> {
    const VENDORS: [&str; 7] = [
        "Java",
        "Eclipse Adoptium",
        "Microsoft",
        "Amazon Corretto",
        "Zulu",
        "BellSoft",
        "Semeru",
    ];

    let mut homes = Vec::new();
    let bases = [
        env::var_os("ProgramFiles").map(PathBuf::from),
        env::var_os("ProgramFiles(x86)").map(PathBuf::from),
    ];

    for base in bases.into_iter().flatten() {
        for vendor in VENDORS {
            let Ok(entries) = std::fs::read_dir(base.join(vendor)) else {
                continue;
            };

            for entry in entries.flatten() {
                let home = entry.path();
                if home.join("bin").join(java_binary()).is_file() {
                    homes.push(home);
                }
            }
        }
    }

    homes
}

fn java_binary() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn read_java_info(exe: &Path) -> Option<JavaInstallation> {
    let mut command = Command::new(exe);
    command.args(["-XshowSettings:properties", "-version"]);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut version = None;
    let mut vendor = None;
    let mut home = None;

    for line in text.lines() {
        let line = line.trim();

        if let Some(value) = line.strip_prefix("java.version = ") {
            version = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("java.vendor = ") {
            vendor = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("java.home = ") {
            home = Some(value.trim().to_string());
        }
    }

    let path = home.unwrap_or_else(|| {
        exe.parent()
            .and_then(Path::parent)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| exe.display().to_string())
    });

    let version = version?;
    let major = major_version(&version);

    Some(JavaInstallation {
        version,
        major,
        lts: matches!(major, 8 | 11 | 17 | 21 | 25),
        vendor: vendor.unwrap_or_else(|| "Unknown".to_string()),
        path,
    })
}

fn major_version(version: &str) -> i32 {
    let version = version.strip_prefix("1.").unwrap_or(version);

    version
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|major| major.parse().ok())
        .unwrap_or(0)
}
