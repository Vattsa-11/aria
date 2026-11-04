import test from "node:test";
import assert from "node:assert/strict";
import { decryptMedia, parseSearch, parseStreamUrl, upgradeQuality } from "../src/saavn.ts";

const TOKEN = "ID2ieOjCrwfgWvL5sXl4B1ImC5QfbsDy/1hrAoFVeiTYsLR09FjcMgF48Zl3V1EPt4oAnp6wVpuEZ9KOfCiKyhw7tS9a8Gtq";

test("decrypts the same token the desktop app decrypts", () => {
  const url = decryptMedia(TOKEN);
  assert.ok(url.startsWith("https://aac.saavncdn.com/475/"), url);
  assert.ok(url.endsWith(".mp4"), url);
});

test("rejects tokens of the wrong length", () => {
  assert.throws(() => decryptMedia("AAAA"));
});

test("stream details become a 320kbps url when flagged", () => {
  const url = parseStreamUrl({ songs: [{ id: "1", more_info: { "320kbps": "true", encrypted_media_url: TOKEN } }] });
  assert.ok(url.includes("_320.mp4"), url);
  assert.throws(() => parseStreamUrl({ songs: [{ id: "1", more_info: {} }] }));
});

test("search results map to songs and skip entries without ids", () => {
  const songs = parseSearch({
    results: [
      { id: "a1", title: "Kesariya &amp; More", subtitle: "Arijit Singh - Brahmastra", image: "https://c.saavncdn.com/x-150x150.jpg", more_info: { duration: "254" } },
      { title: "no id" },
    ],
  });
  assert.equal(songs.length, 1);
  assert.deepEqual([songs[0].title, songs[0].artist, songs[0].duration], ["Kesariya & More", "Arijit Singh", 254]);
  assert.ok(songs[0].thumb.includes("500x500"));
});

test("quality upgrade only touches the bitrate marker", () => {
  assert.equal(upgradeQuality("https://x/song_96.mp4", true), "https://x/song_320.mp4");
  assert.equal(upgradeQuality("https://x/song_96.mp4", false), "https://x/song_96.mp4");
});
