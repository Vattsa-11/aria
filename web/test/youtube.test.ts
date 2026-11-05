import test from "node:test";
import assert from "node:assert/strict";
import { parseClock, parsePlaylist, parseSearch, pickAudio } from "../src/youtube.ts";
import { boundedRange } from "../src/range.ts";

const row = (id: string, title: string) => ({
  musicResponsiveListItemRenderer: {
    playlistItemData: { videoId: id },
    flexColumns: [
      { musicResponsiveListItemFlexColumnRenderer: { text: { runs: [{ text: title }] } } },
      { musicResponsiveListItemFlexColumnRenderer: { text: { runs: [{ text: "Arijit Singh" }, { text: " • " }, { text: "4:22" }] } } },
    ],
  },
});

test("search rows become songs", () => {
  const songs = parseSearch({ a: [row("abc", "Tum Hi Ho"), { musicResponsiveListItemRenderer: { flexColumns: [] } }, row("def", "Kesariya")] });
  assert.equal(songs.length, 2);
  assert.deepEqual([songs[0].id, songs[0].artist, songs[0].duration], ["abc", "Arijit Singh", 262]);
  assert.equal(parseSearch({ a: [row("a", "x"), row("b", "y")] }, 1).length, 1);
  assert.equal(parseClock("1:02:03"), 3723);
  assert.equal(parseClock("nope"), 0);
});

test("picks the best AAC stream and explains refusals", () => {
  const ok = { playabilityStatus: { status: "OK" }, streamingData: { adaptiveFormats: [
    { mimeType: 'audio/webm; codecs="opus"', bitrate: 136000, url: "https://x/opus" },
    { mimeType: 'audio/mp4; codecs="mp4a.40.5"', bitrate: 50000, url: "https://x/low" },
    { mimeType: 'audio/mp4; codecs="mp4a.40.2"', bitrate: 130000, url: "https://x/aac" },
  ] } };
  assert.equal(pickAudio(ok), "https://x/aac");
  assert.throws(() => pickAudio({ playabilityStatus: { status: "LOGIN_REQUIRED", reason: "not a bot" } }), /not a bot/);
});

test("open-ended ranges are bounded so YouTube does not throttle them", () => {
  assert.equal(boundedRange("bytes=0-"), "bytes=0-393215");
  assert.equal(boundedRange(null), "bytes=0-393215");
  assert.equal(boundedRange("bytes=2000000-"), "bytes=2000000-3048575");
  assert.equal(boundedRange("bytes=10-99"), "bytes=10-99");
  assert.equal(boundedRange("bytes=0-99999999"), "bytes=0-393215");
});

test("playlist videos are read from lockup entries", () => {
  const entry = (id: string, title: string, channel: string, time: string) => ({
    lockupViewModel: {
      contentId: id,
      contentType: "LOCKUP_CONTENT_TYPE_VIDEO",
      contentImage: { thumbnailViewModel: { overlays: [{ thumbnailBottomOverlayViewModel: { badges: [{ thumbnailBadgeViewModel: { text: time } }] } }] } },
      metadata: { lockupMetadataViewModel: { title: { content: title }, metadata: { contentMetadataViewModel: { metadataRows: [{ metadataParts: [{ text: { content: channel } }] }] } } } },
    },
  });
  const songs = parsePlaylist({ contents: [entry("aaaaaaaaaaa", "Song A", "Artist A - Topic", "3:21"), entry("bbbbbbbbbbb", "", "x", "1:00")] });
  assert.equal(songs.length, 1);
  assert.deepEqual([songs[0].id, songs[0].title, songs[0].artist, songs[0].duration], ["aaaaaaaaaaa", "Song A", "Artist A", 201]);
});
