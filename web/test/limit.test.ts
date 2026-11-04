import test from "node:test";
import assert from "node:assert/strict";
import { SlidingWindow } from "../src/window.ts";
import { retryAfter } from "../src/limit.ts";

test("the window allows the limit, then blocks, then frees up as hits age out", () => {
  const window = new SlidingWindow();
  for (let i = 0; i < 3; i++) assert.equal(window.hit(3, 60_000, 1000 + i).allowed, true);
  const blocked = window.hit(3, 60_000, 5000);
  assert.equal(blocked.allowed, false);
  assert.equal(blocked.retryAfter, 56);
  assert.equal(window.hit(3, 60_000, 61_001).allowed, true);
  assert.equal(window.hit(3, 60_000, 61_002).allowed, true);
  assert.equal(window.hit(3, 60_000, 61_003).allowed, true);
  assert.equal(window.hit(3, 60_000, 61_004).allowed, false);
});

function namespace(windows: Map<string, SlidingWindow>) {
  return {
    idFromName: (name: string) => name,
    get: (id: unknown) => {
      const key = String(id);
      if (!windows.has(key)) windows.set(key, new SlidingWindow());
      return { hit: async (limit: number, windowMs: number) => windows.get(key)!.hit(limit, windowMs) };
    },
  };
}

test("each client and route class has its own budget", async () => {
  const windows = new Map<string, SlidingWindow>();
  const ns = namespace(windows);
  const mine = new Request("https://x", { headers: { "cf-connecting-ip": "1.1.1.1" } });
  const other = new Request("https://x", { headers: { "cf-connecting-ip": "2.2.2.2" } });
  let blocked = 0;
  for (let i = 0; i < 35; i++) if ((await retryAfter(ns, mine, "/api/search")) > 0) blocked++;
  assert.equal(blocked, 5);
  assert.equal(await retryAfter(ns, other, "/api/search"), 0);
  assert.equal(await retryAfter(ns, mine, "/api/audio"), 0);
  assert.ok([...windows.keys()].includes("1.1.1.1:search") && windows.has("1.1.1.1:audio"));
});

test("without a limiter, or when it fails, requests are allowed", async () => {
  const request = new Request("https://x");
  assert.equal(await retryAfter(undefined, request, "/api/search"), 0);
  const broken = { idFromName: () => 1, get: () => ({ hit: async () => { throw new Error("down"); } }) };
  assert.equal(await retryAfter(broken, request, "/api/search"), 0);
});
