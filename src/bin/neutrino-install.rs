//! Neutrino 命令行版本安装器。
//!
//! 一个独立的可执行文件，复用启动器同一套 `core` 安装逻辑：
//!
//! ```text
//! # 查看可用版本
//! cargo run --bin neutrino-install -- --list
//!
//! # 安装 1.20.4 到官方目录（含资源文件）
//! cargo run --bin neutrino-install -- 1.20.4
//!
//! # 只装最小可启动集合（不下载 assets），装到指定目录
//! cargo run --bin neutrino-install -- 1.20.4 --dir D:/mctest --no-assets
//!
//! # 预演：只统计要下多少东西，不写盘
//! cargo run --bin neutrino-install -- 1.20.4 --dry-run
//! ```

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use neutrino_minecraft_launcher::core::manifest::VersionKind;
use neutrino_minecraft_launcher::core::progress::{Progress, ProgressSnapshot};
use neutrino_minecraft_launcher::core::{
    CoreError, Downloader, InstallOptions, InstallReport, VersionManifest, default_game_dir,
    install,
};

const HELP: &str = "\
Neutrino 命令行版本安装器

用法:
  neutrino-install <版本号> [选项]
  neutrino-install --list [release|snapshot|all] [选项]

选项:
  -d, --dir <路径>       .minecraft 目录（默认取官方目录）
  -t, --threads <数量>   并发下载线程数（默认 8）
      --retries <次数>   单文件失败重试次数（默认 3）
      --manifest <URL>   自定义版本清单地址（换镜像时用）
      --no-assets        不下载资源文件（只装最小可启动集合）
      --virtual-assets   为 1.6 及更早版本建立 virtual 资源目录
      --dry-run          只统计下载量，不写盘
  -h, --help             显示帮助

示例:
  neutrino-install 1.20.4
  neutrino-install 1.12.2 -d D:/mctest --threads 16
  neutrino-install --list release
";

fn main() {
    // 直接把 exe 双击打开时没有参数，窗口会一闪而过，所以最后等一次回车
    let launched_without_args = std::env::args_os().len() <= 1;

    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("\n出错了：{err}");
            1
        }
    };

    if launched_without_args {
        wait_for_enter();
    }

    if code != 0 {
        std::process::exit(code);
    }
}

fn wait_for_enter() {
    if !std::io::stdin().is_terminal() {
        return;
    }
    println!("\n按回车键退出…");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

fn run() -> Result<(), CoreError> {
    let args = Args::parse(std::env::args().skip(1))?;

    match args.command.clone() {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::List(kind) => run_list(&args, kind),
        Command::Install(version) => run_install(&args, &version),
    }
}

#[derive(Debug, Clone)]
enum Command {
    Help,
    List(VersionKind),
    Install(String),
}

#[derive(Debug, Clone)]
struct Args {
    game_dir: PathBuf,
    manifest_url: Option<String>,
    threads: usize,
    retries: u32,
    assets: bool,
    virtual_assets: bool,
    dry_run: bool,
    command: Command,
}

impl Args {
    fn parse<I: IntoIterator<Item = String>>(raw: I) -> Result<Self, CoreError> {
        let tokens: Vec<String> = raw.into_iter().collect();
        let mut args = Args {
            game_dir: default_game_dir(),
            manifest_url: None,
            threads: 8,
            retries: 3,
            assets: true,
            virtual_assets: false,
            dry_run: false,
            command: Command::Help,
        };

        let mut version: Option<String> = None;
        let mut list: Option<VersionKind> = None;
        let mut index = 0;

        while index < tokens.len() {
            let token = tokens[index].clone();

            match token.as_str() {
                "-h" | "--help" => {
                    args.command = Command::Help;
                    return Ok(args);
                }
                "-d" | "--dir" => {
                    index += 1;
                    args.game_dir = PathBuf::from(next_value(&tokens, index, "--dir")?);
                }
                "-t" | "--threads" => {
                    index += 1;
                    args.threads =
                        parse_number(&next_value(&tokens, index, "--threads")?, "--threads")?;
                }
                "--retries" => {
                    index += 1;
                    args.retries =
                        parse_number(&next_value(&tokens, index, "--retries")?, "--retries")?;
                }
                "--manifest" => {
                    index += 1;
                    args.manifest_url = Some(next_value(&tokens, index, "--manifest")?);
                }
                "--no-assets" => args.assets = false,
                "--virtual-assets" => args.virtual_assets = true,
                "--dry-run" => args.dry_run = true,
                "--list" => {
                    // `--list` 的值是可选的：后面跟 release/snapshot/all 时当作筛选条件
                    match tokens
                        .get(index + 1)
                        .and_then(|value| VersionKind::parse(value))
                    {
                        Some(kind) => {
                            index += 1;
                            list = Some(kind);
                        }
                        None => list = Some(VersionKind::Release),
                    }
                }
                other if other.starts_with('-') => {
                    return Err(usage(format!("未知参数 {other}")));
                }
                other => {
                    if version.is_some() {
                        return Err(usage(format!("一次只能安装一个版本，多余参数：{other}")));
                    }
                    version = Some(other.to_string());
                }
            }

            index += 1;
        }

        if let Some(kind) = list {
            args.command = Command::List(kind);
            return Ok(args);
        }

        match version {
            Some(version) => {
                args.command = Command::Install(version);
                Ok(args)
            }
            None => Ok(args),
        }
    }

    fn manifest(&self) -> &str {
        self.manifest_url
            .as_deref()
            .unwrap_or(neutrino_minecraft_launcher::core::DEFAULT_MANIFEST_URL)
    }
}

fn usage(message: String) -> CoreError {
    CoreError::Other(format!("{message}\n\n{HELP}"))
}

fn next_value(tokens: &[String], index: usize, flag: &str) -> Result<String, CoreError> {
    tokens
        .get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| usage(format!("{flag} 需要一个值")))
}

fn parse_number<T: std::str::FromStr>(value: &str, flag: &str) -> Result<T, CoreError> {
    value
        .parse::<T>()
        .map_err(|_| usage(format!("{flag} 的值 “{value}” 不是合法数字")))
}

fn run_list(args: &Args, kind: VersionKind) -> Result<(), CoreError> {
    let downloader = Downloader::new(Progress::silent());
    let manifest: VersionManifest = downloader.get_json(args.manifest(), "版本清单")?;
    let versions = manifest.filter(kind);

    println!(
        "版本清单共 {} 个（最新正式版 {}，最新快照 {}）\n",
        versions.len(),
        manifest.latest.release,
        manifest.latest.snapshot
    );
    println!("版本              类型        发布时间");
    println!("{}", "-".repeat(50));

    for version in versions {
        println!(
            "{:<18}{:<12}{}",
            version.id,
            version.kind,
            version
                .release_time
                .get(..19)
                .unwrap_or(version.release_time.as_str()),
        );
    }

    println!("\n用 `neutrino-install <版本号>` 安装。");
    Ok(())
}

fn run_install(args: &Args, version: &str) -> Result<(), CoreError> {
    let mut options = InstallOptions::new(&args.game_dir, version);
    options.manifest_url = args.manifest().to_string();
    options.assets = args.assets;
    options.virtual_assets = args.virtual_assets;
    options.threads = args.threads;
    options.retries = args.retries;
    options.dry_run = args.dry_run;

    println!("Neutrino 安装器 v{}", env!("CARGO_PKG_VERSION"));
    println!("游戏目录  {}", options.game_dir.display());
    println!("目标版本  {}", options.version);
    println!("下载线程  {}", options.threads);
    if options.dry_run {
        println!("模式      预演（只统计，不写盘）");
    } else if !options.assets {
        println!("模式      跳过资源文件（assets）");
    }
    println!();

    let printer = Arc::new(CliProgress::default());
    let progress = Progress::new(printer.clone().sink());

    let report = install(&options, &progress)?;

    printer.finish();
    print_report(&report);

    Ok(())
}

fn print_report(report: &InstallReport) {
    println!();
    println!("版本目录    {}", report.version_dir.display());
    println!("主类        {}", report.main_class);

    if let Some(jar) = &report.client_jar {
        println!("客户端 jar  {}", jar.display());
    }

    if report.dry_run {
        println!(
            "依赖库      {} 个，约 {}",
            report.library_count,
            human(report.library_bytes)
        );
        println!(
            "资源文件    {} 个，约 {}",
            report.asset_count,
            human(report.asset_bytes)
        );
        println!("\n预演结束，未写入任何文件。去掉 --dry-run 即可正式安装。");
        return;
    }

    println!(
        "依赖库      {} 个（本次下载 {} 个，命中缓存 {} 个，{}）",
        report.library_count,
        report
            .library_stats
            .files
            .saturating_sub(report.library_stats.cached),
        report.library_stats.cached,
        human(report.library_bytes)
    );

    if report.native_library_count > 0 {
        println!(
            "natives     {} 个 jar，解压出 {} 个文件 → {}",
            report.native_library_count,
            report.native_file_count,
            report.natives_dir().display()
        );
    }

    if report.asset_count > 0 {
        println!(
            "资源文件    {} 个，{}",
            report.asset_count,
            human(report.asset_bytes)
        );
    }

    println!("耗时        {:.1}s", report.elapsed.as_secs_f64());

    match (report.java_required, report.java_detected) {
        (Some(required), Some(detected)) => {
            if detected >= required as i32 {
                println!("Java        本机 {} ≥ 要求 {} ✔", detected, required);
            } else {
                println!(
                    "Java        本机 {} < 要求 {} ✘ 请安装 JDK {} 或更高版本",
                    detected, required, required
                );
            }
        }
        (Some(required), None) => {
            println!("Java        未检测到，请安装 JDK {required} 或更高版本");
        }
        (None, Some(detected)) => println!("Java        本机 {detected}（该版本未声明要求）"),
        (None, None) => {}
    }

    println!("\n安装完成，可以在启动器的“实例”页看到该版本。");
}

#[derive(Default)]
struct CliProgress {
    state: Mutex<ProgressState>,
}

#[derive(Default)]
struct ProgressState {
    stage: String,
    printed: bool,
    last_line: String,
}

impl CliProgress {
    fn sink(self: &Arc<Self>) -> impl Fn(ProgressSnapshot) + Send + Sync + 'static {
        let this = Arc::clone(self);
        move |snapshot| this.render(&snapshot)
    }

    fn render(&self, snapshot: &ProgressSnapshot) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if state.stage != snapshot.stage {
            if state.printed {
                println!();
            }
            state.stage.clone_from(&snapshot.stage);
            state.printed = false;
            state.last_line.clear();
        }

        let line = if snapshot.total == 0 {
            format!("{} …", pad(&snapshot.stage, 14))
        } else {
            let width = 22;
            let filled = (snapshot.ratio() * width as f64).round() as usize;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
            format!(
                "{} [{bar}] {:>9} / {:<9} {:>5.1}%",
                pad(&snapshot.stage, 14),
                human(snapshot.done),
                human(snapshot.total),
                snapshot.ratio() * 100.0
            )
        };

        let padded = format!("{line:<72}");
        if padded == state.last_line {
            return;
        }
        state.last_line.clone_from(&padded);

        print!("\r{padded}");
        let _ = std::io::stdout().flush();
        state.printed = true;
    }

    fn finish(&self) {
        if let Ok(mut state) = self.state.lock()
            && state.printed
        {
            println!();
            state.printed = false;
        }
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 按显示宽度补空格（中文字符占两列）。
fn pad(text: &str, width: usize) -> String {
    let current = display_width(text);
    let mut out = text.to_string();
    for _ in current..width {
        out.push(' ');
    }
    out
}

fn display_width(text: &str) -> usize {
    text.chars().map(|ch| if is_wide(ch) { 2 } else { 1 }).sum()
}

fn is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6)
}
