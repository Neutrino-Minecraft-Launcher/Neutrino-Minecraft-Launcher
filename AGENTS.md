# Neutrino Minecraft Launcher

Early-stage Slint + Rust desktop app. The UI is a navigable shell with a wallpaper background; launcher features are not implemented yet.

## Commands

- Build/check: `cargo check` (or `cargo build`)
- Run the app: `cargo run`
- Preview just the UI: `slint-viewer ui/app-window.slint`
- Check one `.slint` file: `slint-viewer --check ui/app-window.slint`
- Pre-finish checks: `cargo fmt --check`, then `cargo clippy -- -D warnings`, then `cargo test`
- CI runs the same three checks on `windows-latest` for pushes to `main` and all PRs (`.github/workflows/ci.yml`). `cargo test` currently runs 0 tests and `tests/` does not exist, so use `cargo run` or `slint-viewer` to verify UI changes manually.

## How it is wired

- `build.rs` compiles only `ui/app-window.slint`. New `.slint` files must be imported from it or explicitly added to `build.rs`, otherwise they are never compiled.
- `src/main.rs` wires the UI; `src/accounts.rs` stores offline accounts (in `%APPDATA%/Neutrino/accounts.json`) and their skins (custom skins copied into `%APPDATA%/Neutrino/skins/`), `src/skins.rs` crops custom skin PNGs into head avatars, `src/java.rs` detects Java installations, `src/instances.rs` scans the Minecraft `versions` folder, and `src/msa.rs` runs the Microsoft device-code login (requires `NEUTRINO_MSA_CLIENT_ID`). `slint::include_modules!()` generates `AppWindow`; `AppState`, `Actions`, `MsaState`, `InstanceInfo`, `ContentFile`, `AccountInfo`, `AccountKind`, `SkinType`, and `JavaInfo` are re-exported from `app-window.slint` so Rust can access them. UI entry chain: `ui/app-window.slint` -> `shell/app-shell.slint` -> `shell/top-bar.slint`; `ui/models/enums.slint` defines `Page`.
- Local Microsoft auth config: copy `.env.example` to `.env` and set `NEUTRINO_MSA_CLIENT_ID`; `.env` is ignored by Git, `.env.example` is tracked.
- Keep `slint` and `slint-build` on the same version; both are pinned to 1.18.1.
- `ui/fonts/HarmonyOS_Sans_SC_Regular.ttf` is imported by `app-window.slint` as the default UI font; `ui/images/backgrounds/Jokull.jpg` is the app shell wallpaper.
- For non-trivial `.slint` work, use the vendored `slint` skill in `.agents/skills/slint/`.

## Repo gotchas

- `.delta/` holds ignored worktrees/clones for the delta tool. Ignore it when searching or editing; it contains stale copies of this repo.
- Git prints `# is not a valid attribute name` because of the inline comment at `.gitattributes:8`; it is harmless.
- Windows MSVC builds get an 8 MB stack from `.cargo/config.toml`; keep that setting.
- Existing project convention: use Conventional Commit prefixes (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`).
