# Aria {{VERSION}}

Aria is a lightweight, touch-first music player. It plays your local files and streams from **YouTube, YouTube Music and JioSaavn**, keeps playlists, liked songs and history, and learns the pace of your listening session to suggest what to play next.

`yt-dlp` {{YTDLP}} is **bundled** in every desktop download, so streaming works the first time you open the app. Nothing needs to be installed separately.

## Which file should I download?

| You use | Download | How to install |
|---|---|---|
| **Windows 10/11** (recommended) | `aria-windows-x86_64.msi` | Double-click. Installs to Program Files with a Start Menu shortcut, including `yt-dlp`. |
| Windows, no install | `aria-windows-x86_64.zip` | Unzip anywhere and run `aria.exe`. Keep `yt-dlp.exe` beside it. |
| **Ubuntu, Debian, Pop!_OS, Mint** | `aria-linux-amd64.deb` | `sudo apt install ./aria-linux-amd64.deb` |
| Other Linux | `aria-linux-x86_64.tar.gz` | Extract and run `./aria`. Needs ALSA, Wayland or X11 libraries. |
| **Mac with Apple silicon** | `aria-macos-aarch64.tar.gz` | Extract and run `./aria`. See the macOS note below. |
| **Android phone** | `aria-android-arm64.apk` | Allow installing from this source, then open the file. Experimental, see below. |
| Advanced | `aria-windows-x86_64.exe`, `aria-linux-x86_64` | The bare program without `yt-dlp`, for scripts and the in-app updater. |

Every file has a matching `.sha256`. `SHA256SUMS.txt` lists them all. To check one:

```
sha256sum -c aria-linux-amd64.deb.sha256
```

## What is in version 1

- **Playback:** local files and folders, streaming from YouTube, YouTube Music and JioSaavn, queue with shuffle and repeat, sleep timer, crossfade, 10-band equalizer, loudness leveling, synced lyrics, media keys and the system volume flyout.
- **Fast starts:** a YouTube song begins playing in about a second once found, and the next song in the queue is prepared while the current one plays.
- **Cover art everywhere:** player bar, queue, lists, Home and the miniplayer.
- **Downloads:** save songs for offline listening. JioSaavn songs download directly with resume and retries; YouTube songs use `yt-dlp`.
- **Smart suggestions:** pick a mood (Chill, Focus, Workout, Party, Sad) or just listen. Aria measures the tempo and energy of what you play, remembers what you liked or skipped, and ranks the next songs to keep the same pace. Everything is computed on your device.
- **Regional trending:** optional. Choose a language, or let Aria look up your approximate state once, and Home shows what is trending there, such as Tamil songs in Tamil Nadu.
- **Imports:** Exportify CSV, plain text lists and playlist links. A YouTube Mix link becomes a saved playlist of up to 100 songs.
- **Miniplayer:** an always-on-top window in three sizes, resizable by mouse or finger.
- **Phone layout:** below 640 points wide the app switches to bottom tabs, a mini player card and a full-screen Now Playing screen.

## Things to know

- **Windows SmartScreen** may warn about an unknown publisher, because the installer is not code-signed. Choose *More info*, then *Run anyway*.
- **macOS** blocks unsigned programs the first time. Right-click the file, choose *Open*, then confirm. There is no `.app` or `.dmg` yet.
- **Android** is an early build. It installs and starts, but audio output, permissions and file picking have not been tested on a real phone, and YouTube streaming needs `yt-dlp`, which phones cannot run, so only JioSaavn songs are expected to stream there. Treat it as a preview.
- **YouTube changes often.** If YouTube songs stop playing, use *Settings, Update yt-dlp*. Aria always uses the newest `yt-dlp` it can find.
- **Privacy:** location lookup, browser-cookie use for `yt-dlp` and regional suggestions are all off until you turn them on, and Settings explains what each one sends.
- Your data lives in `%APPDATA%\Aria` on Windows, and in the usual per-user data folder elsewhere. Set `ARIA_DATA_DIR` to put it somewhere else.

## Upgrading

Install the newer MSI or `.deb` over the old one. Your library and settings are kept.
