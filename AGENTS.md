# Handoff: splitting src/app.rs (branch `dev`)

Aria is a touch-first music player in Rust (egui 0.37, rodio, rusqlite). The same crate builds for desktop and `wasm32-unknown-unknown` (web). `src/app.rs` (~3.6k lines) has an `App` struct that owns almost everything. The goal is to move related state and logic into small structs that `App` owns, one step at a time, with no change in behaviour.

## Rules for every change
- No code comments. Short explanations go in the chat or commit message.
- No `Co-Authored-By` or AI attribution in commits or PRs.
- This file (`AGENTS.md`) is committed on the `dev` branch only, on purpose. Do not merge it into `main`, and never commit `CLAUDE.md` or `.claude/`.
- Tests must use `ARIA_DATA_DIR` (a temp folder), never the real data folder: `export ARIA_DATA_DIR=$(mktemp -d)`.
- Work on `dev` only. Never push to `main` without the owner saying so.
- Do not run `rustup update`.
- Behaviour must not change in a refactor step. Move code, rename, add tests. Do not "improve" logic in the same commit.

## Checks to run after every step (all must pass)
```
export ARIA_DATA_DIR=$(mktemp -d)
cargo clippy --all-targets          # 0 warnings (CI uses -D warnings)
cargo test                          # run twice when touching DB tests
cargo check --target wasm32-unknown-unknown
```
If you changed anything the web build uses, also: `npm --prefix web run build:wasm` (about 2 minutes).

## Done so far
- Step 1: warming and radio state moved to `src/prefetch.rs`.
  - `Warmup` (ready streams, requested ids) replaces `App.warm` and `App.warm_requested`.
  - `Radio` (`pending`, `append`, `tried`, `label`) replaces the four `radio_*` fields.
  - `warm_targets` and `warm_is_usable` moved there as `targets` and `is_usable`, with their tests.

- Step 2: search paging moved to `src/search_state.rs` (`SearchState` with `begin`, `next_page`, `receive`, `change_source`, tests for merging and exhaustion).

## Remaining steps, in order
1. (done) Search paging moved to `src/search_state.rs`.
2. Sleep timer: already a type in `src/sleep.rs`. Move the `App` methods that drive it next to it.
3. Listen meter: `src/listen.rs` exists. Move `count_listening` and `remember_position` into small functions that take a snapshot and return actions.
4. UI-only state into one struct `UiState`: `queue_open`, `lyrics_open`, `now_playing_open`, `adding_songs`, `toast`, `toast_action`, `lyrics_paste`, `page`. Many UI files read these, so do it as a mechanical rename (`app.queue_open` to `app.ui.queue_open`) in its own commit.
5. Split `agent_execute` (a very long `match`) into one function per command group (playback, queue, playlists, downloads, settings, diagnostics). The command list lives in `src/agent.rs` (`COMMANDS`); every command there needs an arm.
6. Later: library/playlists, downloads, lyrics, media controls.

## How to do a step
1. List every use: `grep -n "field_name" src/app.rs src/ui -r`.
2. Create the new struct in its own file with the fields, small methods for what `App` does with them, and unit tests for the pure parts.
3. Replace the fields in `App` and every use, compile, and fix.
4. Run the checks above. Commit with a message describing what moved, not why.

## Gotchas
- `App::new` and the `Default`-style struct literal both list every field; update both.
- Web has stand-ins for several native modules (`src/web/`); anything you move must compile for both targets. Native-only code goes behind `#[cfg(not(target_arch = "wasm32"))]`.
- `Event` and `Job` (in `src/jobs.rs`) are shared with the web runner in `src/web/jobs.rs`; a new variant needs an arm in both.
- Queue tests that shuffle must set `queue.rng` first; the default seed is random.
- Do not make the refactor depend on the miniplayer, updater, or agent code changing at the same time.
