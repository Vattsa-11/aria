export interface EdgeCache {
  match(request: Request): Promise<Response | undefined>;
  put(request: Request, response: Response): Promise<void>;
}

export interface StoreStub {
  get(key: string): Promise<string | null>;
  set(key: string, value: string, seconds: number): Promise<void>;
}

export interface StoreNamespace {
  idFromName(name: string): unknown;
  get(id: unknown): StoreStub;
}

const SHARDS = 4;
let current: EdgeCache | undefined;

function shardOf(key: string): number {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash % SHARDS;
}

export function cacheFrom(namespace: StoreNamespace | undefined): EdgeCache | undefined {
  if (!namespace) return undefined;
  const stub = (key: string) => namespace.get(namespace.idFromName(`cache-${shardOf(key)}`));
  return {
    async match(request) {
      const found = await stub(request.url).get(request.url);
      return found === null ? undefined : new Response(found);
    },
    async put(request, response) {
      const seconds = Number(/max-age=(\d+)/.exec(response.headers.get("cache-control") ?? "")?.[1] ?? 0);
      if (seconds > 0) await stub(request.url).set(request.url, await response.text(), seconds);
    },
  };
}

export function useCache(cache: EdgeCache | undefined): void {
  current = cache;
}

export async function remembered<T>(key: string, ttl: number | ((value: T) => number), produce: () => Promise<T>, cache?: EdgeCache): Promise<T> {
  const store = cache ?? current;
  if (!store) return produce();
  const request = new Request(`https://cache.aria.internal/${encodeURIComponent(key)}`);
  try {
    const hit = await store.match(request);
    if (hit) return (await hit.json()) as T;
  } catch {}
  const value = await produce();
  const seconds = typeof ttl === "function" ? ttl(value) : ttl;
  if (seconds > 0) {
    try {
      await store.put(request, new Response(JSON.stringify(value), { headers: { "cache-control": `public, max-age=${seconds}` } }));
    } catch {}
  }
  return value;
}
