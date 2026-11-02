# Aria

A desktop music player for Windows built with Rust, egui and rodio. It plays local
files and streams from YouTube, YouTube Music and JioSaavn, and keeps playlists,
liked songs, history and the queue in SQLite.

## Build and run

Requires a Rust toolchain (stable) and, for streaming, `yt-dlp` (the app can
download it on first run).

```bash
cargo build --release
./target/release/aria.exe
```

Development build:

```bash
cargo run
```

Checks:

```bash
cargo test
cargo clippy --all-targets
```

## Data location

Everything lives in `%APPDATA%\Aria` unless `ARIA_DATA_DIR` is set:

- `settings.json` - theme, accent, volume, EQ, crossfade, options
- `library.db` - tracks, playlists, liked, history, queue, downloads, lyrics, plays
- `downloads/` - offline copies of tracks
- `aria.log` - error log

Useful environment variables:

- `ARIA_DATA_DIR` - override the whole data folder (used by tests and benchmarks)
- `ARIA_JIOSAAVN_BASE` - override the JioSaavn API base URL
- `aria.exe --stats` - print live memory usage to stdout

## Features

- Themes: dark and AMOLED, accent presets and hex entry
- Local file playback, folder scanning, drag and drop
- Streaming from YouTube, YouTube Music and JioSaavn with a provider fallback chain
- Two-layer queue (user queue plus context queue), shuffle, repeat one/all
- Media keys, volume flyout, keyboard shortcuts (F1 shows the list)
- Playlists, Liked Songs, history, listening stats, SQLite persistence with restore
- 10-band equalizer with preamp, built-in and named custom presets
- Miniplayer in Large, Medium and Compact layouts with position memory
- Downloads per track and per playlist, Downloaded view, cancel and retry
- Offline mode with automatic detection and skipping of unavailable tracks
- Autoplay radio when the queue ends, smart match picker with "Fix match"
- Loudness leveling, crossfade, synced lyrics (LRCLIB), sleep timer
- Imports from Exportify CSV, plain text lists and playlist URLs

## Known limitations

- Crossfade overlaps tracks but is not sample-exact gapless playback.
- Loudness leveling adapts while playing instead of using stored ReplayGain tags.
- JioSaavn and YouTube results depend on third-party endpoints that can change.
- Streaming needs `yt-dlp`; without it only local files and downloads play.
- Windows only (tray-free background playback is not implemented).
