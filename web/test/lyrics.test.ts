import test from "node:test";
import assert from "node:assert/strict";
import { best, cleanTitle, pick } from "../src/lyrics.ts";

test("noise is stripped from video titles before searching", () => {
  assert.equal(cleanTitle("Tum Hi Ho (Official Video)"), "Tum Hi Ho");
  assert.equal(cleanTitle("Kesariya [Official Music Video] (4K)"), "Kesariya");
  assert.equal(cleanTitle("Shape of You"), "Shape of You");
});

test("synced lyrics win over plain ones, plain is the fallback", () => {
  assert.equal(pick({ syncedLyrics: "[00:01.00] hi", plainLyrics: "hi" }), "[00:01.00] hi");
  assert.equal(pick({ syncedLyrics: " ", plainLyrics: "hi" }), "hi");
  assert.equal(pick({ syncedLyrics: null, plainLyrics: "" }), null);
  assert.equal(best([{ plainLyrics: "plain" }, { syncedLyrics: "[00:01.00] sync" }]), "[00:01.00] sync");
  assert.equal(best([{ plainLyrics: "plain" }]), "plain");
  assert.equal(best([]), null);
});
