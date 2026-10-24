use std::io::{Read, Seek};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct InstanceFolder {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFile {
    pub name: String,
    pub size: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InstanceContent {
    pub mods: Vec<ContentFile>,
    pub resource_packs: Vec<ContentFile>,
    pub shader_packs: Vec<ContentFile>,
}

pub fn detect_instances(game_dir: &str) -> Vec<InstanceFolder> {
    let versions_dir = Path::new(game_dir).join("versions");
    let Ok(entries) = std::fs::read_dir(versions_dir) else {
        return Vec::new();
    };

    let mut instances = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            // 只把带 `<目录名>.json` 的目录当作实例；下载失败留下的半成品目录不显示。
            if !path.join(format!("{name}.json")).is_file() {
                return None;
            }

            Some(InstanceFolder {
                name,
                path: path.display().to_string(),
            })
        })
        .collect::<Vec<_>>();

    instances.sort_by_key(|a| a.name.to_lowercase());
    instances
}

pub fn detect_instance_content(instance_path: &str) -> InstanceContent {
    let root = Path::new(instance_path);
    InstanceContent {
        mods: scan_folder(&root.join("mods"), &["jar"]),
        resource_packs: scan_folder(&root.join("resourcepacks"), &["zip"]),
        shader_packs: scan_folder(&root.join("shaderpacks"), &["zip"]),
    }
}

/// 从 jar / 资源包里读出来的附加信息。
#[derive(Debug, Default)]
pub struct ArchiveInfo {
    /// 内嵌图标（原始字节）。
    pub icon: Option<Vec<u8>>,
    /// 模组版本（从 `fabric.mod.json` / `mods.toml` 读取）。
    pub version: String,
    /// 模组加载器：Fabric / Quilt / Forge / NeoForge。
    pub loader: String,
}

/// 从 mod jar / 资源包 zip / 资源包文件夹里读取图标、模组版本和加载器。
///
/// 图标优先 `pack.png`（资源包），其次根目录 `icon.png` / `logo.png`，
/// 最后 `assets/<namespace>/icon.png`（Fabric / Forge 模组常见布局）。
///
/// 只遍历一次中央目录里的文件名来挑图标，元数据文件按固定名字精确读取，
/// 避免对每个 jar 逐条目解压（这是实例页点击卡顿的主要来源之一）。
pub fn read_archive_info(path: &Path) -> ArchiveInfo {
    let mut info = ArchiveInfo::default();

    if path.is_dir() {
        info.icon = std::fs::read(path.join("pack.png")).ok();
        return info;
    }

    let Ok(file) = std::fs::File::open(path) else {
        return info;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return info;
    };

    info.icon = icon_entry_name(&archive).and_then(|name| {
        archive
            .by_name(&name)
            .ok()
            .and_then(|mut entry| read_entry(&mut entry))
    });

    let fabric = read_named_entry(&mut archive, &["fabric.mod.json"]);
    let quilt = read_named_entry(&mut archive, &["quilt.mod.json"]);
    let neoforge = read_named_entry(&mut archive, &["META-INF/neoforge.mods.toml"]);
    let forge = read_named_entry(&mut archive, &["META-INF/mods.toml"]);

    if let Some(bytes) = fabric {
        info.loader = "Fabric".to_string();
        info.version = json_value(&bytes, &["version"]).unwrap_or_default();
    } else if let Some(bytes) = quilt {
        info.loader = "Quilt".to_string();
        info.version = json_value(&bytes, &["quilt_loader", "version"])
            .or_else(|| json_value(&bytes, &["version"]))
            .unwrap_or_default();
    } else if let Some(bytes) = neoforge {
        info.loader = "NeoForge".to_string();
        info.version = toml_mod_version(&bytes);
    } else if let Some(bytes) = forge {
        info.loader = "Forge".to_string();
        info.version = toml_mod_version(&bytes);
    }

    info
}

/// 在中央目录里挑一个优先级最高的图标条目（名字，不解压）。
fn icon_entry_name<R: Read + Seek>(archive: &zip::ZipArchive<R>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;

    for name in archive.file_names() {
        let Ok(name) = name else {
            continue;
        };
        if let Some(priority) = icon_priority(&name)
            && best.as_ref().is_none_or(|(best, _)| priority < *best)
        {
            best = Some((priority, name.into_owned()));
        }
    }

    best.map(|(_, name)| name)
}

/// 按名字精确读取条目内容（zip 里的名字区分大小写，所以要给出候选列表）。
fn read_named_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    names: &[&str],
) -> Option<Vec<u8>> {
    for name in names {
        if let Ok(mut entry) = archive.by_name(name) {
            return read_entry(&mut entry);
        }
    }
    None
}

fn read_entry(reader: &mut impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn json_value(bytes: &[u8], path: &[&str]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mut current = &value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

/// 从 `META-INF/mods.toml` 的 `[[mods]]` 段里取 `version`。
fn toml_mod_version(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut in_mods = false;

    for line in text.lines() {
        let line = line.trim();
        if line == "[[mods]]" {
            in_mods = true;
            continue;
        }
        if !in_mods {
            continue;
        }
        if let Some(rest) = line.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let value = unquote(rest.trim());
                // Forge 模板里可能出现 `${file.jarVersion}`，这种运行时才解析。
                if !value.is_empty() && !value.contains("${") {
                    return value;
                }
            }
        }
    }

    String::new()
}

fn unquote(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return inner.to_string();
    }
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return inner.to_string();
    }
    value.to_string()
}

fn icon_priority(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    if lower == "pack.png" {
        return Some(0);
    }
    if lower == "icon.png" || lower == "logo.png" {
        return Some(1);
    }
    if lower.starts_with("assets/")
        && (lower.ends_with("/icon.png") || lower.ends_with("/logo.png"))
    {
        return Some(2);
    }
    None
}

fn scan_folder(dir: &Path, extensions: &[&str]) -> Vec<ContentFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_dir = path.is_dir();
            let file_name = entry.file_name().to_string_lossy().into_owned();

            if file_name.starts_with('.') {
                return None;
            }

            let (name, enabled) = match file_name.strip_suffix(".disabled") {
                Some(name) if !is_dir => (name.to_string(), false),
                _ => (file_name, true),
            };

            if !is_dir {
                let extension = Path::new(&name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default();
                let supported = extensions
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(extension));
                if !supported {
                    return None;
                }
            }

            let size = if is_dir {
                folder_size(&path)
            } else {
                entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
            };

            Some(ContentFile {
                name,
                size,
                enabled,
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    files
}

fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                folder_size(&path)
            } else {
                entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "neutrino-instances-test-{label}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().expect("file has parent")).expect("create parent dir");
        fs::write(path, contents).expect("write file");
    }

    #[test]
    fn detects_content_and_marks_disabled_files() {
        let temp = TempDir::new("content");
        let instance = temp.path().join("26.1-mc");

        write_file(&instance.join("mods").join("fabric-api.jar"), b"1234");
        write_file(&instance.join("mods").join("old-mod.jar.disabled"), b"12");
        write_file(&instance.join("mods").join("notes.txt"), b"ignored");
        write_file(
            &instance.join("resourcepacks").join("vanilla-tweaks.zip"),
            b"1",
        );
        write_file(
            &instance
                .join("resourcepacks")
                .join("folder-pack")
                .join("pack.mcmeta"),
            b"{}",
        );
        write_file(
            &instance
                .join("shaderpacks")
                .join("complementary.zip.disabled"),
            b"123",
        );

        let content = detect_instance_content(&instance.display().to_string());

        assert_eq!(
            content.mods,
            vec![
                ContentFile {
                    name: "fabric-api.jar".into(),
                    size: 4,
                    enabled: true,
                },
                ContentFile {
                    name: "old-mod.jar".into(),
                    size: 2,
                    enabled: false,
                },
            ]
        );

        let resource_packs = content
            .resource_packs
            .iter()
            .map(|item| (item.name.as_str(), item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            resource_packs,
            vec![("folder-pack", true), ("vanilla-tweaks.zip", true)]
        );
        assert_eq!(content.resource_packs[0].size, 2);

        assert_eq!(
            content.shader_packs,
            vec![ContentFile {
                name: "complementary.zip".into(),
                size: 3,
                enabled: false,
            }]
        );
    }

    #[test]
    fn ignores_unsupported_and_hidden_files() {
        let temp = TempDir::new("filters");
        let instance = temp.path().join("26.1-mc");

        write_file(&instance.join("mods").join(".hidden.jar"), b"1");
        write_file(&instance.join("mods").join("notes.txt"), b"1");
        write_file(&instance.join("mods").join("valid.JAR"), b"1");

        let content = detect_instance_content(&instance.display().to_string());

        assert_eq!(
            content.mods,
            vec![ContentFile {
                name: "valid.JAR".into(),
                size: 1,
                enabled: true,
            }]
        );
    }

    #[test]
    fn returns_empty_content_when_folders_are_missing() {
        let temp = TempDir::new("empty");
        let content = detect_instance_content(&temp.path().display().to_string());

        assert!(content.mods.is_empty());
        assert!(content.resource_packs.is_empty());
        assert!(content.shader_packs.is_empty());
    }

    #[test]
    fn reads_pack_icon_from_resource_pack_folder() {
        let temp = TempDir::new("pack-icon");
        let pack = temp.path().join("MyPack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("pack.png"), b"icon-bytes").unwrap();

        assert_eq!(read_archive_info(&pack).icon, Some(b"icon-bytes".to_vec()));
        assert!(
            read_archive_info(&temp.path().join("missing"))
                .icon
                .is_none()
        );
    }

    #[test]
    fn parses_forge_mod_version() {
        let toml = br#"modLoader="javafml"
loaderVersion="[47,)"
license="MIT"
[[mods]]
modId="examplemod"
version="1.2.3"
displayName="Example"
"#;
        assert_eq!(toml_mod_version(toml), "1.2.3");

        let templated = b"[[mods]]\nversion=\"${file.jarVersion}\"\n";
        assert_eq!(toml_mod_version(templated), "");
    }

    #[test]
    fn parses_fabric_and_quilt_versions() {
        let fabric = br#"{"id":"fabric-api","version":"0.120.0+1.21.8"}"#;
        assert_eq!(
            json_value(fabric, &["version"]).as_deref(),
            Some("0.120.0+1.21.8")
        );

        let quilt = br#"{"quilt_loader":{"version":"0.5.0"}}"#;
        assert_eq!(
            json_value(quilt, &["quilt_loader", "version"]).as_deref(),
            Some("0.5.0")
        );
    }

    #[test]
    fn only_directories_with_version_json_are_instances() {
        let temp = TempDir::new("instances");
        let versions = temp.path().join("versions");
        fs::create_dir_all(versions.join("real")).unwrap();
        fs::write(versions.join("real/real.json"), b"{}").unwrap();
        fs::create_dir_all(versions.join("partial")).unwrap();
        fs::write(versions.join("partial/mods.jar"), b"x").unwrap();

        let found = detect_instances(&temp.path().display().to_string());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "real");
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;

        let file = fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start entry");
            writer.write_all(bytes).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    #[test]
    fn reads_fabric_metadata_and_nested_icon_from_zip() {
        let temp = TempDir::new("zip-fabric");
        let jar = temp.path().join("example.jar");
        write_zip(
            &jar,
            &[
                ("assets/example/icon.png", b"assets-icon"),
                ("fabric.mod.json", br#"{"id":"example","version":"2.1.0"}"#),
            ],
        );

        let info = read_archive_info(&jar);

        assert_eq!(info.loader, "Fabric");
        assert_eq!(info.version, "2.1.0");
        assert_eq!(info.icon.as_deref(), Some(b"assets-icon".as_slice()));
    }

    #[test]
    fn prefers_root_icon_and_reads_forge_metadata_from_zip() {
        let temp = TempDir::new("zip-forge");
        let jar = temp.path().join("example.jar");
        write_zip(
            &jar,
            &[
                ("assets/example/icon.png", b"assets-icon"),
                ("logo.png", b"logo-icon"),
                (
                    "META-INF/mods.toml",
                    b"modLoader=\"javafml\"\n[[mods]]\nversion=\"1.2.3\"\n",
                ),
            ],
        );

        let info = read_archive_info(&jar);

        assert_eq!(info.loader, "Forge");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.icon.as_deref(), Some(b"logo-icon".as_slice()));
    }
}
