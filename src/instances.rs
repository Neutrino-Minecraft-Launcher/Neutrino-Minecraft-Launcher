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
}
