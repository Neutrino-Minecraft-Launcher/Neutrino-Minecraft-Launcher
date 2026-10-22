use std::path::Path;

#[derive(Debug, Clone)]
pub struct InstanceFolder {
    pub name: String,
    pub path: String,
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
