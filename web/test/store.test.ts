import test from "node:test";
import assert from "node:assert/strict";
import { remembered } from "../src/store.ts";

const fakeCache = () => {
  const entries = new Map<string, Response>();
  return {
    entries,
    match: async (request: Request) => entries.get(request.url)?.clone(),
    put: async (request: Request, response: Response) => void entries.set(request.url, response),
  };
};

test("a produced value is shared by later callers until it expires", async () => {
  const cache = fakeCache();
  let calls = 0;
  const produce = async () => ({ n: ++calls });
  assert.deepEqual(await remembered("k", 60, produce, cache), { n: 1 });
  assert.deepEqual(await remembered("k", 60, produce, cache), { n: 1 });
  assert.equal(calls, 1);
  assert.deepEqual(await remembered("other", 60, produce, cache), { n: 2 });
});

test("a ttl of zero is not stored, and a broken cache never breaks the request", async () => {
  const cache = fakeCache();
  let calls = 0;
  await remembered("z", () => 0, async () => ++calls, cache);
  await remembered("z", () => 0, async () => ++calls, cache);
  assert.equal(calls, 2);
  const broken = { match: async () => { throw new Error("down"); }, put: async () => { throw new Error("down"); } };
  assert.equal(await remembered("b", 60, async () => 7, broken), 7);
  assert.equal(await remembered("none", 60, async () => 9), 9);
});

import { cacheFrom } from "../src/store.ts";

test("the durable-object adapter stores text by ttl and shares it across callers", async () => {
  const data = new Map<string, { value: string; seconds: number }>();
  const stub = {
    get: async (key: string) => data.get(key)?.value ?? null,
    set: async (key: string, value: string, seconds: number) => void data.set(key, { value, seconds }),
  };
  const namespace = { idFromName: (name: string) => name, get: () => stub };
  const cache = cacheFrom(namespace)!;
  let calls = 0;
  const produce = async () => ({ n: ++calls });
  assert.deepEqual(await remembered("a", 90, produce, cache), { n: 1 });
  assert.deepEqual(await remembered("a", 90, produce, cache), { n: 1 });
  assert.equal([...data.values()][0].seconds, 90);
  assert.equal(cacheFrom(undefined), undefined);
});
