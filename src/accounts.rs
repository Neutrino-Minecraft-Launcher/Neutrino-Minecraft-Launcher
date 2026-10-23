use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Microsoft,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skin {
    Steve,
    Alex,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub kind: AccountKind,
    pub skin: Skin,
}

#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    accounts: Vec<Account>,
    active: Option<String>,
}

impl Store {
    pub fn load_default() -> Self {
        Self::load(accounts_file())
    }

    pub fn load(path: PathBuf) -> Self {
        let (accounts, active) = read_store(&path);
        Self {
            path,
            accounts,
            active,
        }
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn add_offline(&mut self, name: &str) -> Result<(), String> {
        validate_name(name)?;
        if self
            .accounts
            .iter()
            .any(|account| account.name.eq_ignore_ascii_case(name))
        {
            return Err("该角色名已存在".into());
        }

        let previous_active = self.active.clone();
        self.accounts.push(Account {
            name: name.to_string(),
            kind: AccountKind::Offline,
            skin: Skin::Steve,
        });
        self.active = Some(name.to_string());

        if let Err(error) = self.save() {
            self.accounts.pop();
            self.active = previous_active;
            return Err(error);
        }

        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        let previous_accounts = self.accounts.clone();
        let previous_active = self.active.clone();

        self.accounts
            .retain(|account| !account.name.eq_ignore_ascii_case(name));
        if self.accounts.len() == previous_accounts.len() {
            return Err("未找到该角色".into());
        }

        if self
            .active
            .as_deref()
            .is_some_and(|active| active.eq_ignore_ascii_case(name))
        {
            self.active = self.accounts.first().map(|account| account.name.clone());
        }

        if let Err(error) = self.save() {
            self.accounts = previous_accounts;
            self.active = previous_active;
            return Err(error);
        }

        Ok(())
    }

    pub fn set_active(&mut self, name: &str) -> Result<(), String> {
        if !self
            .accounts
            .iter()
            .any(|account| account.name.eq_ignore_ascii_case(name))
        {
            return Err("未找到该角色".into());
        }

        let previous_active = self.active.clone();
        self.active = Some(name.to_string());

        if let Err(error) = self.save() {
            self.active = previous_active;
            return Err(error);
        }

        Ok(())
    }

    pub fn set_skin(&mut self, name: &str, skin: Skin) -> Result<(), String> {
        let previous_accounts = self.accounts.clone();
        let Some(account) = self
            .accounts
            .iter_mut()
            .find(|account| account.name.eq_ignore_ascii_case(name))
        else {
            return Err("未找到该角色".into());
        };

        account.skin = skin;

        if let Err(error) = self.save() {
            self.accounts = previous_accounts;
            return Err(error);
        }

        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        let accounts = self
            .accounts
            .iter()
            .map(|account| {
                json!({
                    "name": account.name,
                    "kind": match account.kind {
                        AccountKind::Microsoft => "microsoft",
                        AccountKind::Offline => "offline",
                    },
                    "skin": match &account.skin {
                        Skin::Steve => json!({ "type": "steve" }),
                        Skin::Alex => json!({ "type": "alex" }),
                        Skin::Custom(path) => json!({ "type": "custom", "path": path }),
                    },
                })
            })
            .collect::<Vec<_>>();

        let data = json!({
            "accounts": accounts,
            "active": self.active,
        });

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建账号目录失败：{error}"))?;
        }

        let text = serde_json::to_string_pretty(&data)
            .map_err(|error| format!("序列化账号数据失败：{error}"))?;
        fs::write(&self.path, text).map_err(|error| format!("保存账号数据失败：{error}"))
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let length = name.chars().count();
    if !(3..=16).contains(&length) {
        return Err("角色名长度需为 3-16 个字符".into());
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("角色名只能包含字母、数字和下划线".into());
    }
    Ok(())
}

/// Opens a native file picker and copies the chosen PNG into the app's skin
/// directory. Returns `Ok(None)` when the user cancels the dialog.
pub fn pick_custom_skin(name: &str) -> Result<Option<Skin>, String> {
    let Some(source) = rfd::FileDialog::new()
        .add_filter("PNG 图片", &["png"])
        .set_title("选择皮肤图片")
        .pick_file()
    else {
        return Ok(None);
    };

    let dir = skins_dir();
    fs::create_dir_all(&dir).map_err(|error| format!("创建皮肤目录失败：{error}"))?;

    let destination = dir.join(format!("{name}.png"));
    fs::copy(&source, &destination).map_err(|error| format!("复制皮肤文件失败：{error}"))?;

    Ok(Some(Skin::Custom(destination.display().to_string())))
}

fn accounts_file() -> PathBuf {
    data_dir().join("accounts.json")
}

fn skins_dir() -> PathBuf {
    data_dir().join("skins")
}

fn data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Neutrino")
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Library")
            .join("Application Support")
            .join("Neutrino")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local").join("share"))
            })
            .unwrap_or_default()
            .join("neutrino")
    }
}

fn read_store(path: &Path) -> (Vec<Account>, Option<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return (Vec::new(), None);
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return (Vec::new(), None);
    };

    let accounts = value
        .get("accounts")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let kind = match item.get("kind").and_then(Value::as_str) {
                        Some("microsoft") => AccountKind::Microsoft,
                        _ => AccountKind::Offline,
                    };
                    let skin = match item
                        .get("skin")
                        .and_then(|skin| skin.get("type"))
                        .and_then(Value::as_str)
                    {
                        Some("alex") => Skin::Alex,
                        Some("custom") => {
                            let path = item
                                .get("skin")
                                .and_then(|skin| skin.get("path"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            Skin::Custom(path)
                        }
                        _ => Skin::Steve,
                    };
                    Some(Account { name, kind, skin })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let active = value
        .get("active")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|active| {
            accounts
                .iter()
                .any(|account| account.name.eq_ignore_ascii_case(active))
        });

    (accounts, active)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "neutrino-accounts-test-{label}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn store(&self) -> Store {
            Store::load(self.path.join("accounts.json"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn adds_offline_account_and_activates_it() {
        let temp = TempDir::new("add");
        let mut store = temp.store();

        store.add_offline("Steve").expect("add account");

        assert_eq!(
            store.accounts(),
            &[Account {
                name: "Steve".into(),
                kind: AccountKind::Offline,
                skin: Skin::Steve,
            }]
        );
        assert_eq!(store.active(), Some("Steve"));

        let reloaded = temp.store();
        assert_eq!(reloaded.accounts(), store.accounts());
        assert_eq!(reloaded.active(), Some("Steve"));
    }

    #[test]
    fn rejects_invalid_and_duplicate_names() {
        let temp = TempDir::new("validate");
        let mut store = temp.store();

        assert!(store.add_offline("ab").is_err());
        assert!(store.add_offline("name with space").is_err());
        assert!(store.add_offline("名字").is_err());

        store.add_offline("Steve").expect("add account");
        assert!(store.add_offline("steve").is_err());
    }

    #[test]
    fn removing_active_account_selects_another() {
        let temp = TempDir::new("remove");
        let mut store = temp.store();

        store.add_offline("Steve").expect("add Steve");
        store.add_offline("Alex").expect("add Alex");
        assert_eq!(store.active(), Some("Alex"));

        store.remove("Alex").expect("remove Alex");

        assert_eq!(store.accounts().len(), 1);
        assert_eq!(store.active(), Some("Steve"));

        store.remove("Steve").expect("remove Steve");
        assert!(store.accounts().is_empty());
        assert_eq!(store.active(), None);
    }

    #[test]
    fn only_existing_accounts_can_be_activated() {
        let temp = TempDir::new("active");
        let mut store = temp.store();

        assert!(store.set_active("Steve").is_err());

        store.add_offline("Steve").expect("add Steve");
        store.add_offline("Alex").expect("add Alex");
        store.set_active("Steve").expect("activate Steve");

        assert_eq!(temp.store().active(), Some("Steve"));
    }

    #[test]
    fn skin_changes_persist() {
        let temp = TempDir::new("skin");
        let mut store = temp.store();

        store.add_offline("Steve").expect("add Steve");
        store
            .set_skin("Steve", Skin::Custom("/some/skin.png".into()))
            .expect("set custom skin");
        store.set_skin("Steve", Skin::Alex).expect("set alex skin");

        assert_eq!(temp.store().accounts()[0].skin, Skin::Alex);

        assert!(store.set_skin("Missing", Skin::Alex).is_err());
    }
}
