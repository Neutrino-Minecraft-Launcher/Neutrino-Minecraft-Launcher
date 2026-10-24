//! 启动逻辑：把已安装的版本 JSON + 账号信息拼成 `java` 命令并拉起进程。
//!
//! 安装阶段（[`super::install`]）已经下载好客户端、依赖库、资源文件和旧版 natives；
//! 这里只负责：
//! - 读取 `versions/<id>/<id>.json`；
//! - 按规则筛选依赖库、拼出 classpath；
//! - 求值 `arguments.jvm` / `arguments.game`（或旧版 `minecraftArguments`）并替换占位符；
//! - 启动 Java 进程。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::error::{CoreError, Result, io_err};
use super::manifest::{Library, MavenCoordinate, Rule, VersionJson, rules_allow};
use super::paths::GamePaths;

/// 写进 `${launcher_name}` 的品牌名。
pub const LAUNCHER_NAME: &str = "Neutrino";

/// 默认最大堆内存（MB）。
pub const DEFAULT_MAX_MEMORY_MB: u32 = 2048;

/// classpath 分隔符。
fn classpath_separator() -> &'static str {
    if cfg!(windows) { ";" } else { ":" }
}

/// 启动时使用的账号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchAccount {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    /// `--userType`：离线账号用 `legacy`，微软账号用 `msa`。
    pub user_type: String,
}

impl LaunchAccount {
    /// 构造一个离线账号（UUID 按 Mojang 的 MD5 规则推导）。
    pub fn offline(name: &str) -> Self {
        Self {
            name: name.to_string(),
            uuid: offline_uuid(name),
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
        }
    }
}

/// 启动参数。
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// `.minecraft` 根目录（`libraries/`、`assets/`、`versions/`）。
    pub root_dir: PathBuf,
    /// 实例自己的游戏目录（工作目录，也是 `--gameDir`）。
    /// 默认是 `versions/<version>/`，实现版本隔离。
    pub game_dir: PathBuf,
    /// 版本 id，例如 `1.20.4`。
    pub version: String,
    pub account: LaunchAccount,
    /// `java` 可执行文件（不是 `java.home`）。
    pub java: PathBuf,
    pub max_memory_mb: u32,
    pub extra_jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
}

impl LaunchOptions {
    pub fn new(
        root_dir: impl Into<PathBuf>,
        version: impl Into<String>,
        account: LaunchAccount,
        java: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = root_dir.into();
        let version = version.into();
        let game_dir = root_dir.join("versions").join(&version);

        Self {
            root_dir,
            game_dir,
            version,
            account,
            java: java.into(),
            max_memory_mb: DEFAULT_MAX_MEMORY_MB,
            extra_jvm_args: Vec::new(),
            extra_game_args: Vec::new(),
        }
    }
}

/// 拼好的启动命令。
#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
}

impl LaunchCommand {
    /// 拉起进程，stdout/stderr 追加到 `<game_dir>/logs/launcher.log`。
    pub fn spawn(&self) -> Result<Child> {
        // 实例目录正常情况下安装时已经建好；这里兜底，避免 current_dir 失败。
        let _ = std::fs::create_dir_all(&self.working_dir);

        let mut command = Command::new(&self.java);
        command.args(&self.args).current_dir(&self.working_dir);

        let log_dir = self.working_dir.join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("launcher.log");
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            && let Ok(clone) = file.try_clone()
        {
            command.stdout(Stdio::from(file));
            command.stderr(Stdio::from(clone));
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // 不弹额外的控制台窗口。
            command.creation_flags(0x0800_0000);
        }

        command.spawn().map_err(|err| io_err(&self.java, err))
    }
}

/// 拼出启动命令（不启动进程）。
pub fn build(options: &LaunchOptions) -> Result<LaunchCommand> {
    let paths = GamePaths::new(&options.root_dir);
    let version = load_version(&paths, &options.version)?;

    if version.inherits_from.is_some() {
        return Err(CoreError::Other(
            "暂不支持继承版本（Forge / Fabric）启动".to_string(),
        ));
    }
    if version.main_class.is_empty() {
        return Err(CoreError::MissingField("mainClass"));
    }

    let client_jar = paths.client_jar(&options.version);
    if !client_jar.is_file() {
        return Err(CoreError::Other(format!(
            "缺少客户端 jar：{}（请先安装该版本）",
            client_jar.display()
        )));
    }

    let classpath = build_classpath(&paths, &version, &client_jar);
    let classpath_string = classpath
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(classpath_separator());

    let assets_index = if version.assets.is_empty() {
        version
            .asset_index
            .as_ref()
            .map(|index| index.id.clone())
            .unwrap_or_default()
    } else {
        version.assets.clone()
    };
    let game_assets = paths
        .virtual_assets_dir(&assets_index)
        .display()
        .to_string();

    let context = PlaceholderContext {
        account: &options.account,
        version: &options.version,
        version_type: version.kind.clone(),
        game_dir: options.game_dir.display().to_string(),
        assets_root: paths.assets_dir().display().to_string(),
        assets_index,
        game_assets,
        library_directory: paths.libraries_dir().display().to_string(),
        natives_dir: paths.natives_dir(&options.version).display().to_string(),
        classpath: classpath_string,
        launcher_name: LAUNCHER_NAME.to_string(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let mut jvm_args = vec![format!("-Xmx{}M", options.max_memory_mb)];
    jvm_args.extend(options.extra_jvm_args.iter().cloned());

    match &version.arguments {
        Some(arguments) => jvm_args.extend(evaluate_arguments(&arguments.jvm, &context)),
        None => {
            jvm_args.push(format!("-Djava.library.path={}", context.natives_dir));
            jvm_args.push("-cp".to_string());
            jvm_args.push(context.classpath.clone());
        }
    }

    // 保证一定带上 classpath（部分版本的 jvm 参数可能没有 `-cp`）。
    if !jvm_args
        .iter()
        .any(|arg| arg == "-cp" || arg == "-classpath")
    {
        jvm_args.push("-cp".to_string());
        jvm_args.push(context.classpath.clone());
    }

    let mut game_args = match &version.arguments {
        Some(arguments) => evaluate_arguments(&arguments.game, &context),
        None => version
            .minecraft_arguments
            .as_deref()
            .map(|legacy| {
                legacy
                    .split_whitespace()
                    .map(|token| substitute(token, &context))
                    .collect()
            })
            .unwrap_or_default(),
    };
    game_args.extend(options.extra_game_args.iter().cloned());

    let mut args = jvm_args;
    args.push(version.main_class.clone());
    args.extend(game_args);

    Ok(LaunchCommand {
        java: options.java.clone(),
        args,
        working_dir: options.game_dir.clone(),
    })
}

/// 拼出启动命令并立刻拉起进程。
pub fn spawn(options: &LaunchOptions) -> Result<Child> {
    build(options)?.spawn()
}

fn load_version(paths: &GamePaths, version: &str) -> Result<VersionJson> {
    let path = paths.version_json(version);
    let bytes = std::fs::read(&path).map_err(|err| io_err(&path, err))?;
    serde_json::from_slice(&bytes).map_err(|source| CoreError::Json {
        context: "版本信息 JSON".to_string(),
        source,
    })
}

/// 按规则筛选依赖库，返回最终 classpath（依赖库 + 客户端 jar）。
///
/// 1.19+ 的 natives 以独立 `natives-*` classifier 依赖直接放在 classpath 上，
/// 由 LWJGL 自己解压到 `${natives_directory}`，因此这里一律收录 `downloads.artifact`。
fn build_classpath(paths: &GamePaths, version: &VersionJson, client_jar: &Path) -> Vec<PathBuf> {
    let mut classpath = Vec::new();
    let mut seen = HashSet::new();

    for library in &version.libraries {
        if !rules_allow(&library.rules) {
            continue;
        }

        let Some(relative) = artifact_relative_path(library) else {
            continue;
        };

        let path = paths.libraries_dir().join(relative);
        if seen.insert(path.clone()) {
            classpath.push(path);
        }
    }

    classpath.push(client_jar.to_path_buf());
    classpath
}

/// 依赖库落盘的相对路径（与安装阶段保持一致）。
fn artifact_relative_path(library: &Library) -> Option<String> {
    match library.downloads.as_ref() {
        Some(downloads) => {
            let artifact = downloads.artifact.as_ref()?;
            let path = artifact.path.clone().or_else(|| {
                MavenCoordinate::parse(&library.name).map(|coord| coord.relative_path())
            })?;
            (!path.is_empty()).then_some(path)
        }
        // 极老的版本只给仓库根地址和 Maven 坐标
        None => MavenCoordinate::parse(&library.name).map(|coord| coord.relative_path()),
    }
}

/// 占位符替换所需的上下文。
struct PlaceholderContext<'a> {
    account: &'a LaunchAccount,
    version: &'a str,
    version_type: String,
    game_dir: String,
    assets_root: String,
    assets_index: String,
    game_assets: String,
    library_directory: String,
    natives_dir: String,
    classpath: String,
    launcher_name: String,
    launcher_version: String,
}

/// 求值 `arguments.jvm` / `arguments.game`。
///
/// 每个元素要么是字符串（始终生效），要么是带 `rules` 的对象（规则允许才生效）。
fn evaluate_arguments(values: &[serde_json::Value], context: &PlaceholderContext) -> Vec<String> {
    let mut arguments = Vec::new();

    for value in values {
        match value {
            serde_json::Value::String(text) => arguments.push(substitute(text, context)),
            serde_json::Value::Object(object) => {
                let rules = object
                    .get("rules")
                    .and_then(|rules| serde_json::from_value::<Vec<Rule>>(rules.clone()).ok())
                    .unwrap_or_default();
                if !rules_allow(&rules) {
                    continue;
                }

                match object.get("value") {
                    Some(serde_json::Value::String(text)) => {
                        arguments.push(substitute(text, context));
                    }
                    Some(serde_json::Value::Array(values)) => {
                        for value in values {
                            if let serde_json::Value::String(text) = value {
                                arguments.push(substitute(text, context));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    arguments
}

fn substitute(text: &str, context: &PlaceholderContext) -> String {
    text.replace("${auth_player_name}", &context.account.name)
        .replace("${auth_uuid}", &context.account.uuid)
        .replace("${auth_access_token}", &context.account.access_token)
        .replace("${auth_session}", &context.account.access_token)
        .replace("${user_type}", &context.account.user_type)
        .replace("${version_name}", context.version)
        .replace("${version_type}", &context.version_type)
        .replace("${game_directory}", &context.game_dir)
        .replace("${assets_root}", &context.assets_root)
        .replace("${assets_index_name}", &context.assets_index)
        .replace("${game_assets}", &context.game_assets)
        .replace("${library_directory}", &context.library_directory)
        .replace("${natives_directory}", &context.natives_dir)
        .replace("${classpath}", &context.classpath)
        .replace("${classpath_separator}", classpath_separator())
        .replace("${launcher_name}", &context.launcher_name)
        .replace("${launcher_version}", &context.launcher_version)
        .replace("${user_properties}", "{}")
        // 离线启动没有这些身份信息，给空串以免把占位符原样传给游戏。
        .replace("${clientid}", "")
        .replace("${auth_xuid}", "")
}

/// Mojang 的离线 UUID：`MD5("OfflinePlayer:<name>")`，再把版本号/变体位改成 UUID v3。
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};

    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = hasher.finalize();

    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "neutrino-launch-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_version(game_dir: &Path, id: &str, json: &str) {
        let version_dir = game_dir.join("versions").join(id);
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join(format!("{id}.json")), json).unwrap();
        std::fs::write(version_dir.join(format!("{id}.jar")), b"jar").unwrap();
    }

    #[test]
    fn offline_uuid_is_deterministic_and_v3() {
        let first = offline_uuid("Steve");
        let second = offline_uuid("Steve");
        assert_eq!(first, second);
        assert_eq!(first.len(), 36);
        assert_eq!(first.as_bytes()[14], b'3', "版本位应为 3：{first}");
        assert!(matches!(first.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(first, offline_uuid("Alex"));
    }

    #[test]
    fn builds_modern_launch_command() {
        let game_dir = temp_dir("modern");
        write_version(
            &game_dir,
            "test",
            r#"{
              "id": "test",
              "type": "release",
              "mainClass": "net.minecraft.client.main.Main",
              "assets": "12",
              "libraries": [
                { "name": "com.example:lib:1.0",
                  "downloads": { "artifact": { "path": "com/example/lib/1.0/lib-1.0.jar", "url": "https://example.invalid/lib.jar" } } }
              ],
              "arguments": {
                "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                "game": ["--username", "${auth_player_name}", "--uuid", "${auth_uuid}",
                         "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}",
                         "--assetIndex", "${assets_index_name}", "--accessToken", "${auth_access_token}",
                         "--userType", "${user_type}", "--version", "${version_name}"]
              }
            }"#,
        );

        let options = LaunchOptions::new(
            &game_dir,
            "test",
            LaunchAccount::offline("Steve"),
            game_dir.join("java.exe"),
        );
        let command = build(&options).expect("build command");

        let classpath_index = command
            .args
            .iter()
            .position(|arg| arg == "-cp")
            .expect("-cp present");
        let classpath = &command.args[classpath_index + 1];
        assert!(classpath.contains("lib-1.0.jar"));
        assert!(classpath.contains("test.jar"));
        assert!(classpath.contains(classpath_separator()));

        let main_index = command
            .args
            .iter()
            .position(|arg| arg == "net.minecraft.client.main.Main")
            .expect("main class present");
        assert!(classpath_index < main_index);
        assert!(main_index + 1 < command.args.len());

        let username = command
            .args
            .iter()
            .position(|arg| arg == "--username")
            .unwrap();
        assert_eq!(command.args[username + 1], "Steve");
        assert!(command.args.contains(&offline_uuid("Steve")));
        assert!(command.args.contains(&"-Xmx2048M".to_string()));

        // 版本隔离：游戏目录和 assets 目录要分开。
        let game_dir_index = command
            .args
            .iter()
            .position(|arg| arg == "--gameDir")
            .expect("--gameDir present");
        let instance_dir = game_dir.join("versions").join("test");
        assert_eq!(
            command.args[game_dir_index + 1],
            instance_dir.display().to_string()
        );
        assert_eq!(command.working_dir, instance_dir);

        let assets_index = command
            .args
            .iter()
            .position(|arg| arg == "--assetsDir")
            .expect("--assetsDir present");
        assert_eq!(
            command.args[assets_index + 1],
            game_dir.join("assets").display().to_string()
        );

        let _ = std::fs::remove_dir_all(&game_dir);
    }

    #[test]
    fn supports_legacy_minecraft_arguments() {
        let game_dir = temp_dir("legacy");
        write_version(
            &game_dir,
            "old",
            r#"{
              "id": "old",
              "type": "release",
              "mainClass": "net.minecraft.client.Minecraft",
              "minecraftArguments": "--username ${auth_player_name} --gameDir ${game_directory}",
              "libraries": []
            }"#,
        );

        let options = LaunchOptions::new(
            &game_dir,
            "old",
            LaunchAccount::offline("Steve"),
            game_dir.join("java.exe"),
        );
        let command = build(&options).expect("build command");

        assert!(command.args.contains(&"-cp".to_string()));
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--username".to_string(), "Steve".to_string()])
        );

        let _ = std::fs::remove_dir_all(&game_dir);
    }

    #[test]
    fn rejects_missing_client_jar() {
        let game_dir = temp_dir("missing");
        let version_dir = game_dir.join("versions").join("test");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("test.json"),
            r#"{"id":"test","mainClass":"a.B","libraries":[]}"#,
        )
        .unwrap();

        let options = LaunchOptions::new(
            &game_dir,
            "test",
            LaunchAccount::offline("Steve"),
            game_dir.join("java.exe"),
        );
        assert!(build(&options).is_err());

        let _ = std::fs::remove_dir_all(&game_dir);
    }
}
