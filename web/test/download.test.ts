import test from "node:test";
import assert from "node:assert/strict";
// @ts-ignore plain browser module
import { downloadAudio } from "../public/download.js";

const bytes = (n: number) => new Uint8Array(n).fill(7).buffer;

function fakeCache() {
  const stored: { url: string; size: number }[] = [];
  return { stored, put: async (request: Request, response: Response) => void stored.push({ url: request.url, size: (await response.arrayBuffer()).byteLength }) };
}

const reply = (status: number, size: number, range?: string) => ({ status, headers: { get: (name: string) => (name === "content-range" ? range ?? null : null) }, arrayBuffer: async () => bytes(size) });

test("a flaky connection is retried chunk by chunk instead of failing the whole download", async () => {
  const cache = fakeCache();
  let calls = 0;
  const fetchImpl = async (_url: string, init: { headers: { Range: string } }) => {
    calls++;
    if (calls === 1) throw new TypeError("Failed to fetch");
    if (calls === 2) return reply(502, 0);
    const start = Number(/bytes=(\d+)-/.exec(init.headers.Range)![1]);
    const total = 300_000;
    const size = Math.min(100_000, total - start);
    return reply(206, size, `bytes ${start}-${start + size - 1}/${total}`);
  };
  const seen: number[] = [];
  const size = await downloadAudio({ id: 5, url: "/api/audio?id=x", progress: (done: number) => seen.push(done), cache, fetchImpl, wait: async () => {} });
  assert.equal(size, 300_000);
  assert.deepEqual(seen, [100_000, 200_000, 300_000]);
  assert.equal(cache.stored.length, 1);
  assert.equal(cache.stored[0].size, 300_000);
});

test("a stalled connection gives a clear timeout message after the retries are used up", async () => {
  const fetchImpl = (_url: string, init: { signal: AbortSignal }) => new Promise((_resolve, reject) => init.signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError"))));
  await assert.rejects(
    downloadAudio({ id: 1, url: "https://cdn/x.mp4", progress: () => {}, cache: fakeCache(), fetchImpl, wait: async () => {}, timeoutMs: 20, tries: 2 }),
    /timed out/,
  );
});

test("direct downloads ask for bounded chunks and an empty answer is rejected", async () => {
  const ranges: string[] = [];
  const fetchImpl = async (_url: string, init: { headers: { Range: string } }) => {
    ranges.push(init.headers.Range);
    return reply(206, 100, "bytes 0-99/100");
  };
  await assert.rejects(downloadAudio({ id: 2, url: "https://cdn/x.mp4", progress: () => {}, cache: fakeCache(), fetchImpl, wait: async () => {} }), /empty/);
  assert.deepEqual(ranges, ["bytes=0-2097151"]);
});

test("a CDN that hides Content-Range still finishes: a short chunk or a 416 means the file is complete", async () => {
  const cache = fakeCache();
  let calls = 0;
  const fetchImpl = async (_url: string, init: { headers: { Range: string } }) => {
    calls++;
    const start = Number(/bytes=(\d+)-/.exec(init.headers.Range)![1]);
    if (start >= 2 * 2097152 + 500_000) return reply(416, 0);
    return reply(206, start < 2 * 2097152 ? 2097152 : 500_000);
  };
  const size = await downloadAudio({ id: 3, url: "https://cdn/x.mp4", progress: () => {}, cache, fetchImpl, wait: async () => {} });
  assert.equal(size, 2 * 2097152 + 500_000);
  assert.equal(calls, 3);
});
