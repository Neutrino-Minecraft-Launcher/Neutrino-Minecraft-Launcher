//! Mojang 版本清单与版本 JSON 的数据模型、依赖规则求值、Maven 坐标解析。

use std::collections::HashMap;

use serde::Deserialize;

use super::error::Result;
use super::net::Downloader;

/// `version_manifest_v2.json`。
#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

/// 清单里标注的最新正式版 / 快照。
#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

/// 清单中的单个版本条目。
#[derive(Debug, Clone, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
    #[serde(rename = "complianceLevel", default)]
    pub compliance_level: u32,
}

/// 版本类型筛选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionKind {
    Release,
    Snapshot,
    All,
}

impl VersionKind {
    /// 从命令行字符串解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "release" => Some(Self::Release),
            "snapshot" => Some(Self::Snapshot),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    fn accepts(self, kind: &str) -> bool {
        match self {
            Self::All => true,
            Self::Release => kind == "release",
            Self::Snapshot => kind == "snapshot",
        }
    }
}

impl VersionManifest {
    /// 按 id 查找版本（不区分大小写）。
    pub fn find(&self, id: &str) -> Option<&VersionSummary> {
        self.versions
            .iter()
            .find(|version| version.id.eq_ignore_ascii_case(id))
    }

    /// 按类型筛选版本。
    pub fn filter(&self, kind: VersionKind) -> Vec<&VersionSummary> {
        self.versions
            .iter()
            .filter(|version| kind.accepts(&version.kind))
            .collect()
    }
}

/// 单个版本的完整 JSON（`versions/<id>/<id>.json` 的内容）。
#[derive(Debug, Clone, Deserialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "mainClass", default)]
    pub main_class: String,
    #[serde(default)]
    pub assets: String,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub arguments: Option<LaunchArguments>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
}

/// 版本 JSON 中 `assetIndex` 字段。
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(rename = "totalSize", default)]
    pub total_size: Option<u64>,
    pub url: String,
}

/// 版本 JSON 中 `downloads` 字段。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<Artifact>,
    #[serde(default)]
    pub server: Option<Artifact>,
}

/// 一个可下载文件。
#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

/// 依赖库条目。
#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// 旧版本用来声明“当前平台需要哪个 natives classifier”。
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub extract: Option<Extract>,
    /// 部分旧版本只给仓库根地址，artifact 需要按 Maven 规则拼出来。
    #[serde(default)]
    pub url: Option<String>,
}

/// 依赖库的下载项。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: HashMap<String, Artifact>,
}

/// 平台 / 特性规则。
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

/// 规则中的平台条件。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// natives 解压排除规则。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// `arguments` 字段（1.13+ 使用）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LaunchArguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

/// 版本要求的 Java 运行时。
#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    #[serde(rename = "majorVersion", default)]
    pub major_version: u32,
}

/// `assets/indexes/<id>.json`。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: HashMap<String, AssetObject>,
    #[serde(rename = "map_to_resources", default)]
    pub map_to_resources: bool,
    #[serde(rename = "virtual", default)]
    pub virtual_assets: bool,
}

/// 单个资源文件。
#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: u64,
}

impl VersionJson {
    /// 从 URL 拉取并解析版本 JSON。
    pub fn fetch(downloader: &Downloader, url: &str) -> Result<Self> {
        downloader.get_json(url, "版本信息 JSON")
    }
}

/// 当前操作系统的 Mojang 名称。
pub fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// 当前 CPU 架构的 Mojang 名称。
pub fn os_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    }
}

/// natives classifier 里 `${arch}` 的替换值（32 / 64）。
fn arch_bits() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

fn rule_matches(rule: &Rule) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name
            && !name.eq_ignore_ascii_case(os_name())
        {
            return false;
        }
        if let Some(arch) = &os.arch
            && !arch.eq_ignore_ascii_case(os_arch())
        {
            return false;
        }
        // `os.version` 在 Mojang 规范里是正则表达式，只有极少数 macOS 旧版本会用到。
        // 这里不引入正则依赖，视作匹配，真正的平台判断留到启动阶段。
    }

    if let Some(features) = &rule.features {
        // 安装阶段不启用任何特性（demo 模式、自定义分辨率等），
        // 因此任何“要求特性为 true”的规则都不应命中。
        for required in features.values() {
            if *required {
                return false;
            }
        }
    }

    true
}

/// 按 Mojang 规范求值规则列表：空规则表示允许，否则“最后一条命中的规则”说了算。
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action.eq_ignore_ascii_case("allow");
        }
    }
    allowed
}

/// 取得该依赖库在当前平台需要的 natives classifier。
pub fn natives_classifier(library: &Library) -> Option<String> {
    let template = library.natives.get(os_name())?;
    Some(template.replace("${arch}", arch_bits()))
}

/// Maven 坐标（`group:artifact:version[:classifier][@ext]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoordinate {
    /// 解析依赖名。
    pub fn parse(name: &str) -> Option<Self> {
        let (name, extension) = match name.split_once('@') {
            Some((head, ext)) if !ext.is_empty() => (head, ext.to_ascii_lowercase()),
            _ => (name, "jar".to_string()),
        };

        let parts: Vec<&str> = name.split(':').collect();
        if parts.len() < 3 || parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty() {
            return None;
        }

        Some(Self {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            classifier: parts.get(3).map(|value| value.to_string()),
            extension,
        })
    }

    /// 转成 `libraries/` 下的相对路径。
    pub fn relative_path(&self) -> String {
        let mut path = format!(
            "{}/{}/{}/{}-{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version
        );

        if let Some(classifier) = &self.classifier {
            path.push('-');
            path.push_str(classifier);
        }

        path.push('.');
        path.push_str(&self.extension);
        path
    }
}

/// 由仓库根地址拼出完整的 Maven 下载地址。
pub fn maven_url(base: &str, coordinate: &MavenCoordinate) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        coordinate.relative_path()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_VERSION_JSON: &str = r#"{
      "id": "1.20.4",
      "type": "release",
      "mainClass": "net.minecraft.client.main.Main",
      "assets": "12",
      "assetIndex": {
        "id": "12",
        "sha1": "0f0e0a0b",
        "size": 123,
        "totalSize": 456789,
        "url": "https://example.invalid/12.json"
      },
      "downloads": {
        "client": { "sha1": "abcdef", "size": 1024, "url": "https://example.invalid/client.jar" }
      },
      "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 17 },
      "libraries": [
        {
          "name": "com.mojang:logging:1.1.1",
          "downloads": {
            "artifact": {
              "path": "com/mojang/logging/1.1.1/logging-1.1.1.jar",
              "sha1": "aa",
              "size": 5,
              "url": "https://example.invalid/logging.jar"
            }
          }
        },
        {
          "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
          "downloads": {
            "classifiers": {
              "natives-windows": {
                "path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows.jar",
                "sha1": "bb",
                "size": 6,
                "url": "https://example.invalid/natives.jar"
              }
            }
          },
          "natives": { "windows": "natives-windows", "linux": "natives-linux", "osx": "natives-osx" },
          "extract": { "exclude": ["META-INF/"] }
        }
      ]
    }"#;

    fn rule(action: &str, os: Option<&str>) -> Rule {
        Rule {
            action: action.to_string(),
            os: os.map(|name| OsRule {
                name: Some(name.to_string()),
                arch: None,
                version: None,
            }),
            features: None,
        }
    }

    #[test]
    fn parses_version_json() {
        let json: VersionJson = serde_json::from_str(SAMPLE_VERSION_JSON).unwrap();

        assert_eq!(json.id, "1.20.4");
        assert_eq!(json.main_class, "net.minecraft.client.main.Main");
        assert_eq!(json.assets, "12");
        assert_eq!(json.asset_index.as_ref().unwrap().id, "12");
        assert_eq!(json.downloads.client.as_ref().unwrap().size, Some(1024));
        assert_eq!(json.java_version.as_ref().unwrap().major_version, 17);
        assert_eq!(json.libraries.len(), 2);
        assert!(json.inherits_from.is_none());
    }

    #[test]
    fn empty_rules_allow_by_default() {
        assert!(rules_allow(&[]));
    }

    #[test]
    fn deny_on_current_os_wins() {
        let denied = vec![rule("disallow", Some(os_name()))];
        assert!(!rules_allow(&denied));
    }

    #[test]
    fn allow_on_other_os_only_denies_here() {
        let other = if os_name() == "windows" {
            "linux"
        } else {
            "windows"
        };
        let rules = vec![rule("allow", Some(other))];
        assert!(!rules_allow(&rules));
    }

    #[test]
    fn last_matching_rule_wins() {
        // allow windows -> disallow windows：后者生效
        let rules = vec![
            rule("allow", Some(os_name())),
            rule("disallow", Some(os_name())),
        ];
        assert!(!rules_allow(&rules));

        // disallow windows -> allow windows：重新放行
        let rules = vec![
            rule("disallow", Some(os_name())),
            rule("allow", Some(os_name())),
        ];
        assert!(rules_allow(&rules));
    }

    #[test]
    fn feature_rules_are_not_matched() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: None,
            features: Some(HashMap::from([("is_demo_user".to_string(), true)])),
        }];
        assert!(!rules_allow(&rules));
    }

    #[test]
    fn natives_classifier_follows_current_os() {
        let json: VersionJson = serde_json::from_str(SAMPLE_VERSION_JSON).unwrap();
        let library = &json.libraries[1];
        let classifier = natives_classifier(library);
        assert_eq!(classifier, Some(format!("natives-{}", os_name())));
        assert!(json.libraries[0].natives.is_empty());
        assert_eq!(natives_classifier(&json.libraries[0]), None);
    }

    #[test]
    fn natives_classifier_replaces_arch_placeholder() {
        let library = Library {
            name: "example:native:1.0".to_string(),
            downloads: None,
            natives: HashMap::from([(os_name().to_string(), "natives-${arch}".to_string())]),
            rules: Vec::new(),
            extract: None,
            url: None,
        };
        let classifier = natives_classifier(&library).unwrap();
        assert!(classifier == "natives-64" || classifier == "natives-32");
    }

    #[test]
    fn parses_maven_coordinates() {
        let plain = MavenCoordinate::parse("com.mojang:logging:1.1.1").unwrap();
        assert_eq!(plain.group, "com.mojang");
        assert_eq!(plain.artifact, "logging");
        assert_eq!(plain.version, "1.1.1");
        assert_eq!(plain.classifier, None);
        assert_eq!(plain.extension, "jar");
        assert_eq!(
            plain.relative_path(),
            "com/mojang/logging/1.1.1/logging-1.1.1.jar"
        );

        let natives = MavenCoordinate::parse("org.lwjgl:lwjgl:3.3.2:natives-windows").unwrap();
        assert_eq!(natives.classifier.as_deref(), Some("natives-windows"));
        assert_eq!(
            natives.relative_path(),
            "org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-windows.jar"
        );

        let with_ext = MavenCoordinate::parse("net.java:jinput:2.0.5@zip").unwrap();
        assert_eq!(with_ext.extension, "zip");
        assert_eq!(
            with_ext.relative_path(),
            "net/java/jinput/2.0.5/jinput-2.0.5.zip"
        );
    }

    #[test]
    fn rejects_broken_maven_coordinates() {
        assert!(MavenCoordinate::parse("only:two").is_none());
        assert!(MavenCoordinate::parse("").is_none());
        assert!(MavenCoordinate::parse("::").is_none());
    }

    #[test]
    fn builds_maven_url() {
        let coordinate = MavenCoordinate::parse("com.mojang:logging:1.1.1").unwrap();
        assert_eq!(
            maven_url("https://libraries.minecraft.net/", &coordinate),
            "https://libraries.minecraft.net/com/mojang/logging/1.1.1/logging-1.1.1.jar"
        );
    }

    #[test]
    fn filters_manifest_by_kind() {
        let manifest = VersionManifest {
            latest: LatestVersions {
                release: "1.20.4".to_string(),
                snapshot: "24w03a".to_string(),
            },
            versions: vec![
                VersionSummary {
                    id: "1.20.4".to_string(),
                    kind: "release".to_string(),
                    url: "a".to_string(),
                    release_time: String::new(),
                    compliance_level: 1,
                },
                VersionSummary {
                    id: "24w03a".to_string(),
                    kind: "snapshot".to_string(),
                    url: "b".to_string(),
                    release_time: String::new(),
                    compliance_level: 1,
                },
            ],
        };

        assert_eq!(manifest.filter(VersionKind::Release).len(), 1);
        assert_eq!(manifest.filter(VersionKind::Snapshot).len(), 1);
        assert_eq!(manifest.filter(VersionKind::All).len(), 2);
        assert_eq!(manifest.find("1.20.4").unwrap().id, "1.20.4");
        assert!(manifest.find("nope").is_none());
    }
}
