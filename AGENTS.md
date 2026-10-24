# Neutrino Minecraft Launcher

Early-stage Slint + Rust desktop app. The UI is a navigable shell with a wallpaper background.
Version **installation** (vanilla, Fabric, Forge) and offline-account launching are implemented in
the shared core library; the GUI creates instances from the “实例” (instances) page, while the CLI
(`neutrino-install`) installs vanilla versions only.

## Commands

- Build/check: `cargo check` (or `cargo build`)
- Run the app: `cargo run`
- Run the installer: `cargo run --bin neutrino-install -- --list` / `cargo run --bin neutrino-install -- 1.20.4 --dry-run`
- Preview just the UI: `slint-viewer ui/app-window.slint`
- Check one `.slint` file: `slint-viewer --check ui/app-window.slint`
- Pre-finish checks: `cargo fmt --all -- --check`, then `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo test --all-targets --all-features`
- CI runs the same three checks on `windows-latest` for pushes to `main` and all PRs (`.github/workflows/ci.yml`).
  Unit tests are inline `#[cfg(test)]` modules (no `tests/` dir); UI changes still need `cargo run` or `slint-viewer` to verify manually.

## How it is wired

- `src/lib.rs` is the shared library: `core` (installer), `java` (runtime detection), `instances` (`versions/` scan).
  Both `src/main.rs` (GUI) and `src/bin/neutrino-install.rs` (CLI) depend on it, so core logic must stay UI-free.
- `core/install.rs` is the only place that mutates the game directory. Everything goes through
  `.part` file → SHA-1 verify → rename, and already-valid files are skipped, so re-running is idempotent.
  `install()` writes a vanilla version to `versions/<version>/`; `install_instance()` merges the
  vanilla JSON with an optional loader profile into a self-contained `versions/<instance>/`
  (id rewritten, `inheritsFrom` removed), with libraries/assets shared from the game root.
  It also creates the instance's `mods/`, `resourcepacks/`, `shaderpacks/`, `saves/`, `config/`
  and `logs/` folders.
- `core/loader.rs` queries Fabric Meta / Forge Maven for loader versions, downloads Fabric's
  profile JSON, and runs the official Forge installer (requires a detected Java). Forge is installed
  into the real game dir so its patching processors can write `libraries/`; the temporary generated
  version dir is removed after the JSON is read.
- `core/launch.rs` builds the `java` command from an installed version JSON (classpath, `arguments.jvm` /
  `arguments.game`, placeholder substitution) and spawns the game. It expects the merged instance
  JSONs produced by `install_instance` (no `inheritsFrom`); `${library_directory}` is substituted
  for Forge. Raw versions with `inheritsFrom` are still rejected. The game directory / working
  directory is `versions/<id>/` (version isolation), while `libraries/` and `assets/` stay shared.
  Game stdout/stderr is appended to `versions/<id>/logs/launcher.log`.
- `core/paths.rs` owns the `.minecraft` layout; never hand-join `versions/...` elsewhere.
- `src/desktop.rs` (`InstallController`) is the only GUI↔core bridge: it owns the version-list cache,
  the filter, the current discover category, and the background install / download threads. Slint
  properties may only be written on the UI thread, so background work always goes through
  `Weak::upgrade_in_event_loop`; progress callbacks therefore capture the controller
  (`Arc<Mutex<_>>` all the way down) instead of a `Weak` directly. UI callbacks
  (`on_choose_version`, `on_choose_loader`, `on_choose_loader_version`, `on_go_back`,
  `on_create_instance`, `on_filter_versions`, `on_open_category`, `on_download_project`, …) run on
  the UI thread and may touch `AppState` synchronously.
- The instances page's “添加实例” wizard drives the Mojang version list (`refresh-versions` /
  `filter-versions` / `choose-version` / `choose-loader` / `choose-loader-version` / `create-instance`);
  the version list no longer checks local install state. The instance name is validated for Windows
  illegal characters, reserved names and existing `versions/<name>/` directories. The discover page
  has four categories `mod` / `shader` / `resourcepack` / `modpack` (Modrinth, via `core/modrinth.rs`).
  Downloads land in `mods/`, `shaderpacks/`, `resourcepacks/`; modpack one-click install is
  intentionally not implemented yet. Only one install/download task may run at a time.
- All `AppState` properties are `in` (Rust-set only). UI-local state (e.g. the discover filter) lives
  on the view component and resets when the app restarts.
- `core/net.rs` exposes `Downloader` (`Clone`) and `download_all` (scoped threads). Progress goes through
  `core/progress.rs`, which throttles callbacks and is safe to share across threads.
- `build.rs` compiles only `ui/app-window.slint`. New `.slint` files must be imported from it or explicitly added to `build.rs`, otherwise they are never compiled.
- `src/main.rs` wires the UI; `src/accounts.rs` stores offline accounts (in `%APPDATA%/Neutrino/accounts.json`) and their skins (custom skins copied into `%APPDATA%/Neutrino/skins/`), `src/skins.rs` crops custom skin PNGs into head avatars, `src/java.rs` detects Java installations, `src/instances.rs` scans the Minecraft `versions` folder, and `src/msa.rs` runs the Microsoft device-code login (requires `NEUTRINO_MSA_CLIENT_ID`). `slint::include_modules!()` generates `AppWindow`; `AppState`, `Actions`, `MsaState`, `InstanceInfo`, `ContentFile`, `AccountInfo`, `AccountKind`, `SkinType`, and `JavaInfo` are re-exported from `app-window.slint` so Rust can access them. UI entry chain: `ui/app-window.slint` -> `shell/app-shell.slint` -> `shell/top-bar.slint`; `ui/models/enums.slint` defines `Page`.
- 实例内容的扫描 / 解压 / 图标解码都在 `main.rs` 的 `InstanceContentLoader` 后台线程里做（先回填名称+大小，再异步补版本/加载器/图标），UI 线程只负责换模型；不要再把这类 IO 放回 `load-instance-content` 回调，否则点实例会整窗卡住。
- Local Microsoft auth config: copy `.env.example` to `.env` and set `NEUTRINO_MSA_CLIENT_ID`; `.env` is ignored by Git, `.env.example` is tracked.
- Keep `slint` and `slint-build` on the same version; both are pinned to 1.18.1.
- `ui/fonts/HarmonyOS_Sans_SC_Regular.ttf` is imported by `app-window.slint` as the default UI font; `ui/images/backgrounds/Jokull.jpg` is the app shell wallpaper.
- For non-trivial `.slint` work, use the vendored `slint` skill in `.agents/skills/slint/`.

## Repo gotchas

- **不要给 `slint` 加回 `accessibility` 特性**（`Cargo.toml` 里用的是 `default-features = false`）。
  本机 Windows 11 + Slint 1.18.1 + accesskit_windows 0.34 下，窗口创建时
  `SetPropW` 返回 `ERROR_NOT_ENOUGH_MEMORY`，accesskit 直接 panic，表现为"双击 exe 一闪就退"。
  去掉该特性后界面正常；要恢复读屏支持，需要先确认上游修好（accesskit_windows > 0.34 或
  Slint 换版本），不能只改 Cargo 特性。
- `.delta/` holds ignored worktrees/clones for the delta tool. Ignore it when searching or editing; it contains stale copies of this repo.
- Git prints `# is not a valid attribute name` because of the inline comment at `.gitattributes:8`; it is harmless.
- Windows MSVC builds get an 8 MB stack from `.cargo/config.toml`; keep that setting.
- Existing project convention: use Conventional Commit prefixes (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`).
- 从 agent/脚本里启动 GUI 做验证时，注意用 `tasklist` 确认进程存活而不是看 shell 是否返回；
  `(./app.exe &)` 在 shell 退出后进程可能被带走。
