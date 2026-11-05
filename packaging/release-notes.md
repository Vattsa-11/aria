# Aria {{VERSION}}

Aria is a lightweight, touch-first music player. It plays your local files and streams from **YouTube, YouTube Music and JioSaavn**, keeps playlists, liked songs and history, and learns the pace of your listening session to suggest what to play next.

`yt-dlp` {{YTDLP}} is **bundled** in every download, so streaming works the first time you open the app. Nothing needs to be installed separately.

## New in 1.0.5

**Miniplayer (desktop)**
- Rebuilt from scratch in three sizes: a slim bar, a medium card and a full card. They no longer fade into each other while you resize.
- Fixed dragging the edges and corners (it used to fight with moving the window), fixed the close cross for finger and pen taps on Windows touch screens, and kept all controls clear of the resize edges.
- Clicking the cover returns you to the main window; holding it likes the song; swipe left or right to change song.
- It remembers whether it was open, and its size and position.

**Search and suggestions**
- **Endless search:** results load 15 at a time as you scroll, on desktop and web, instead of stopping after a handful.
- **Your artists:** a new Home shelf with artist mixes built from the artists you play and like most.
- **Songs prepared in advance:** the top trending and your most recent songs are looked up before you tap them, so they start faster.
- **Suggestions never stop:** while the last song in your queue plays, Aria adds similar songs to Up Next, so there is no silence and no wait.

**Transitions**
- **The next song is prepared earlier** on desktop and web, and the crossfade waits until it is ready.
- **Smart fade length:** the crossfade is shorter when the next song is much louder or at a very different tempo, and stays long when two songs are alike (only for songs Aria has analysed).
- **Crossfade on phones** (Android and Chrome): songs now blend instead of cutting, while background playback keeps working. iPhone still switches songs without a fade.

**Lyrics**
- **Better matching:** titles are cleaned (no "Official Video", "From ..." or channel names), "Artist - Title" names are split, joined artists are tried one at a time, and the lyrics closest in length win.
- **Untimed lyrics** now show instead of "No lyrics".
- **Your own lyrics:** when none are found, use *Paste lyrics*, or on desktop *Choose .lrc or .txt file*. A `.lrc` file with the same name next to a local song is picked up automatically.

**Import**
- **YouTube playlist links work in the web app.** Paste a link that contains `list=...`. Mix links, private and empty playlists show a clear message.

**Sound**
- Equalizer settings are checked when loaded (bad numbers can no longer break playback), changing a band no longer clicks, and loud boosts are softened instead of clipped. A "Peaks softened" note appears next to the preamp when that happens.

**Updates and safety**
- Updates now come only from the Aria project on GitHub, and a download is installed only if it matches the SHA-256 checksum published with the release.
- The audio engine reports a clear error if it cannot start and stops when the app closes. Shuffle is properly random.

**Agent mode and health checks (desktop)**
- *Settings, Agent mode, Run diagnostics* now also checks the database, audio device, YouTube Music, JioSaavn and GitHub reachability and speed, the downloads folder, update checksums and how many plays are failing.
- Every agent command now has a description, a scope and an argument schema (`commands`). Destructive commands (clear the queue, delete playlists or downloads, change settings) are refused unless you tick *Allow destructive commands*. New commands: `health`, `history`, `events`, `audit`.

## How to use the new things

1. **Endless search:** search, then keep scrolling. More songs appear as you reach the bottom.
2. **Artist mixes:** open Home and tap a card under *Your artists*.
3. **Your own lyrics:** open the lyrics panel on a song with no lyrics, press *Paste lyrics*, paste, then *Save lyrics*. On desktop you can also press *Choose .lrc or .txt file*.
4. **Import a YouTube playlist (web):** *More, Import*, paste the playlist link, press *Import URL*, review the matches, then *Add to playlist*.
5. **Miniplayer:** press the miniplayer button in the player bar. Drag anywhere to move it, drag an edge or corner to resize it, tap the cross to close it.
6. **Crossfade length:** *Settings, Equalizer and crossfade*. Set it to 0 for gapless playback without fading.

## Earlier: new in 1.0.4

**Playback**
- **Music no longer stops when the queue runs out.** While your last song plays, Aria fetches similar songs and adds them to Up Next, so the next one starts straight away. Turn it off with *Settings, Autoplay*.
- **Pause for as long as you like.** If the player was dropped while paused (common on phones), pressing play now continues from where you stopped instead of starting the song again.
- **Gapless on phones:** songs now hand over without cutting off the last seconds.
- **Music keeps playing with the screen off or the app in the background** in the web app on phones (lock-screen play, pause, next and previous work), and *Settings, Keep screen on* can hold the screen awake while music plays.
- **Songs YouTube blocks for other apps** play in the official YouTube player inside Aria, which you can drag around, hide with *Hide*, and which hides itself when the song is paused. Aria still controls it (play, pause, seek, volume, queue). These songs cannot be downloaded.

**Queue**
- **Remove any upcoming song** with the cross on its row, including songs that came from a search, album or radio list, and use **Clear** to empty either list. Removing the song that is playing is not allowed.

**Miniplayer (desktop)**
- Fixed dragging the edges and corners: resizing no longer fights moving the window, and it now works with a finger or pen on Windows touch screens.
- Fixed the close cross not reacting to a finger or pen tap.
- The miniplayer remembers whether it was open and its size and position, and comes back the same way the next time you start Aria.

**Web app**
- **Import works in the browser:** paste a list of songs (one per line, such as `Kesariya - Arijit Singh`) or choose a CSV or text file, check the matches, then add them to a playlist.
- **Home:** swipe a row of songs sideways to open *Show all*. Scrolling up and down is no longer blocked by the rows.
- **Update banner:** when a new version is available the app tells you and updates on the next open.
- The Import screen wraps properly on narrow screens.

## How to use the 1.0.4 things

1. **Radio after your queue:** just keep listening. When the last song in Up Next is playing, similar songs appear under Up Next within a few seconds.
2. **Remove from the queue:** open the queue (the list button in the player), tap the cross on a song. *Clear* sits at the top of each list.
3. **Miniplayer:** press the miniplayer button in the player bar. Drag anywhere to move it, drag an edge or corner to resize it, and use the cross at the top right to close it. Swipe left or right to change song and swipe up for Now Playing.
4. **Install the web app:** open https://aria-web.sharmamagi0.workers.dev on your phone. On Android use *Install app*, on iPhone use Share, then *Add to Home Screen*. On a PC you can use it in the browser or install it from the address bar.
5. **Import songs:** *More, Import*. Paste your list or choose a file, press *Find matches*, fix any wrong match with its alternatives, pick a playlist name and press *Add to playlist*. The desktop app also accepts Exportify CSVs and playlist links.
6. **YouTube player songs on web:** if a song shows the YouTube player, use the buttons in Aria as usual. Press *Hide* to tuck the player away, and drag it by its top edge to move it.
7. **Agent mode (desktop):** *Settings, Agent mode, On*. A program on the same computer can then control Aria with the token file shown on that card (`%APPDATA%\Aria\agent\token` on Windows) over `127.0.0.1:47474`. *Run diagnostics* checks your install. It is off by default and never listens on the network.

## Earlier: new in 1.0.3

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
