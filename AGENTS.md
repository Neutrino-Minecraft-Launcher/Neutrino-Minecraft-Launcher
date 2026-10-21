# Neutrino Minecraft Launcher

Early-stage Slint + Rust desktop app. The UI is still the template counter, not a launcher yet.

## Commands

- Build/check: `cargo check` (or `cargo build`)
- Run the app: `cargo run`
- Preview just the UI: `slint-viewer ui/app-window.slint`
- Check one `.slint` file: `slint-viewer --check ui/app-window.slint`
- Pre-finish checks: `cargo fmt --check`, then `cargo clippy -- -D warnings`, then `cargo test`
- There is no CI or test harness yet; `cargo test` runs 0 tests and `tests/` does not exist. Use `cargo run` or `slint-viewer` to verify UI changes manually.

## How it is wired

- `build.rs` compiles only `ui/app-window.slint`. New `.slint` files must be imported from it or explicitly added to `build.rs`, otherwise they are never compiled.
- `src/main.rs` is the entire Rust side. `slint::include_modules!()` generates `AppWindow`; kebab-case names become snake_case (`request-increase-value` -> `on_request_increase_value`, `counter` -> `get_counter`/`set_counter`).
- Keep `slint` and `slint-build` on the same version; both are pinned to 1.18.1.
- `ui/fonts/HarmonyOS_Sans_SC_Regular.ttf` exists but is not referenced by any `.slint` file yet.
- For non-trivial `.slint` work, use the vendored `slint` skill in `.agents/skills/slint/`.

## Repo gotchas

- `README.md` is still the upstream Slint template (ZIP download/rename instructions); do not treat it as project documentation.
- `.delta/` holds ignored worktrees/clones for the delta tool. Ignore it when searching or editing; it contains stale copies of this repo.
- Git prints `# is not a valid attribute name` because of the inline comment at `.gitattributes:8`; it is harmless.
- Windows MSVC builds get an 8 MB stack from `.cargo/config.toml`; keep that setting.
- Existing project convention: use Conventional Commit prefixes (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`).
