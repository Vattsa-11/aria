# Future work

## Android APK

Status: scaffolding is in the repo, but nothing has been built successfully. The
`Android` workflow is manual only (Actions tab, Run workflow) and is not part of
the release workflow. Its one push-triggered run failed in about a minute and the
log has not been read yet.

Already in place:
- `src/lib.rs` has `android_main`; the desktop binary is a thin wrapper.
- `Cargo.toml` has Android-only `android-activity` and eframe `android-native-activity`,
  plus `[package.metadata.android]` (package `com.magi8101.aria`, min SDK 26, arm64).
- File pickers go through `src/dialogs.rs`, which returns nothing on Android.
- `.github/workflows/android.yml` builds with `cargo apk build --release --lib`
  (debug-signed, not Play Store ready).

To do:
- Run the Android workflow, read the log, and fix the compile errors (audio backend,
  souvlaki, C dependencies such as SQLite and ring).
- Streaming without `yt-dlp`: use the direct HTTP path (JioSaavn) only; hide YouTube
  sources on Android.
- Replace the stubbed file pickers (Storage Access Framework or MediaStore).
- Phone layout: bottom tab bar, touch-sized rows, no miniplayer window.
- Android media notification and lock-screen controls.
- Put the Android job back into `release.yml` once it builds.
- Proper release signing key stored as a repository secret.

## Phone-sized layout (desktop app)

A narrow-width layout with a bottom tab bar and larger touch rows.

## In-app auto-update

Check the latest GitHub release, show an "Update available" button, download the
matching asset on press, verify its `.sha256`, swap the executable and restart.
Needs a public repository or a token option. Nothing downloads without a button press.

## Remaining items from the fix list

Direct HTTP download for JioSaavn and other online tracks, heart and icon set, cover art,
search quality scoring, lyrics lookup, sleep timer modes, Spotify link import and paste,
touch keyboard, first-run yt-dlp and ffmpeg setup, README rewrite.

## Cover art follow-ups

- Embedded cover art from local files (ID3, MP4, FLAC tags). Today local files only use
  a `cover`, `folder`, `front`, `album` or `albumart` image next to the file.
- YouTube Music square album art instead of the video thumbnail.
- Disk cache size cap for `thumbs/` (memory is capped, disk is not).
