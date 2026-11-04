# Aria {{VERSION}}

Aria is a lightweight, touch-first music player. It plays your local files and streams from **YouTube, YouTube Music and JioSaavn**, keeps playlists, liked songs and history, and learns the pace of your listening session to suggest what to play next.

`yt-dlp` {{YTDLP}} is **bundled** in every download, so streaming works the first time you open the app. Nothing needs to be installed separately.

## New in 1.0.3

- **Aria now runs in your browser too.** The same app, with the same screens, works as an installable web app on phones, tablets and PCs: https://aria-web.sharmamagi0.workers.dev. Your friends can use it without installing anything big, and a change to the app shows up on desktop and web together.
- **Your Library:** one place for Liked Songs, Downloads, Recently played and your playlists, with filter chips that only appear when you have that kind of content, Recents or A-Z sorting, and a list or grid view.
- **Editable queue:** *Add songs*, *Clear*, and up and down arrows on every queued song, also on small screens. Swipe a song to the right to add it to the queue.
- **Gestures for touch screens:** swipe the mini player left or right to change song, swipe it up to open Now Playing, swipe Now Playing down to close it, and long-press a song for its menu.
- **Album colours:** the mini player and Now Playing screen take their colour from the cover art.
- **Home:** recommendations are now swipeable rows with a *Show all* option, instead of one long wall of covers.
- **Smoother playback:** a song that cannot be played is skipped automatically (up to three in a row) when you are playing through a queue.
- **Colours that follow the music:** the accent colour, player bar and Now Playing screen now adapt to the cover art. You can switch this off in Settings (*Match the song's colours*).
- **Honest stats:** a play now counts in Stats only after you have listened to half of the song, so skipping through songs no longer inflates your numbers.
- **Search:** a clear button returns you from results to the starting screen.
- **Fixes:** the heart on search results now works, the window adapts to very small or short sizes, the sidebar scrolls, and the queue and lyrics panels respond to the mouse wheel.
- **Data folder change on Linux and macOS:** Aria now stores its library in your user data folder (`~/.local/share/Aria` on Linux, `~/Library/Application Support/Aria` on macOS) instead of the folder it was started from. If you used an older version, copy the `Aria` folder you find next to where you launched it into the new location to keep your library. Windows is unchanged.

## Trying it on Windows (quickest)

1. Download **`aria-windows-x86_64.msi`** below and double-click it. If Windows shows a blue SmartScreen box, choose *More info*, then *Run anyway* (the installer is not code-signed).
2. Open **Aria** from the Start Menu.
3. Search for a song, or use Home for suggestions. Press *Download* in a song's menu to keep it offline.

Prefer no installer? Download **`aria-windows-x86_64.exe`** instead. It is a single self-contained file with `yt-dlp` inside; just run it.

## Which file should I download?

| You use | Download | How to install |
|---|---|---|
| **Windows 10/11** (recommended) | `aria-windows-x86_64.msi` | Double-click. Installs to Program Files with a Start Menu shortcut, including `yt-dlp`. |
| Windows, single file | `aria-windows-x86_64.exe` | Run it. `yt-dlp` is built in and unpacks itself on first start. |
| Windows, no install | `aria-windows-x86_64.zip` | Unzip anywhere and run `aria.exe`. Keep `yt-dlp.exe` beside it. |
| **Ubuntu, Debian, Pop!_OS, Mint** | `aria-linux-amd64.deb` | `sudo apt install ./aria-linux-amd64.deb` |
| Other Linux | `aria-linux-x86_64.tar.gz` | Extract and run `./aria`. Needs ALSA, Wayland or X11 libraries. |
| **Mac with Apple silicon** | `aria-macos-aarch64.tar.gz` | Extract and run `./aria`. See the macOS note below. |

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
- **YouTube changes often.** If YouTube songs stop playing, use *Settings, Update yt-dlp*. Aria always uses the newest `yt-dlp` it can find.
- **Privacy:** location lookup, browser-cookie use for `yt-dlp` and regional suggestions are all off until you turn them on, and Settings explains what each one sends.
- Your data lives in `%APPDATA%\Aria` on Windows, and in the usual per-user data folder elsewhere. Set `ARIA_DATA_DIR` to put it somewhere else.

## Upgrading

Install the newer MSI or `.deb` over the old one. Your library and settings are kept.
