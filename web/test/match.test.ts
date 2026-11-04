import test from "node:test";
import assert from "node:assert/strict";
import { pickMatch } from "../src/match.ts";
import { boundedRange } from "../src/range.ts";
import { cached, remember, forget } from "../src/youtube.ts";
import type { Song } from "../src/saavn.ts";

const song = (id: string, title: string, artist: string, duration: number): Song => ({ id, title, artist, duration, thumb: "", source: "jiosaavn" });

test("the JioSaavn stand-in must be the same song, not just a similar title", () => {
  const found = [
    song("a", "Tum Hi Ho (Lofi Remix)", "DJ X", 190),
    song("b", "Tum Hi Ho", "Arijit Singh", 262),
    song("c", "Kesariya", "Arijit Singh", 262),
  ];
  assert.equal(pickMatch(found, "Tum Hi Ho (Official Video)", "Arijit Singh", 262)?.id, "b");
  assert.equal(pickMatch(found, "Tum Hi Ho", "Arijit Singh", 400), null);
  assert.equal(pickMatch([song("c", "Kesariya", "A", 262)], "Tum Hi Ho", "A", 262), null);
  assert.equal(pickMatch(found, "", "", 0), null);
});

test("the first audio chunk is small so playback starts quickly", () => {
  assert.equal(boundedRange("bytes=0-"), "bytes=0-393215");
  assert.equal(boundedRange(null), "bytes=0-393215");
  assert.equal(boundedRange("bytes=393216-"), "bytes=393216-1441791");
  assert.equal(boundedRange("bytes=10-99"), "bytes=10-99");
});

test("resolved stream urls are reused briefly, refusals are remembered shorter", () => {
  forget("x1");
  assert.equal(cached("x1"), null);
  remember("x1", { url: "https://gv/x" }, 1000);
  assert.equal(cached("x1", 1000 + 24 * 60_000)?.url, "https://gv/x");
  assert.equal(cached("x1", 1000 + 26 * 60_000), null);
  remember("x2", { error: "bot" }, 1000);
  assert.equal(cached("x2", 1000 + 30_000)?.error, "bot");
  assert.equal(cached("x2", 1000 + 50_000), null);
});
