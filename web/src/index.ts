import * as saavn from "./saavn.ts";
import * as youtube from "./youtube.ts";
import { retryAfter, type Namespace } from "./limit.ts";
import { boundedRange, CHUNK } from "./range.ts";
import { pickMatch, searchQueries } from "./match.ts";
import * as lyrics from "./lyrics.ts";
import { cacheFrom, remembered, useCache, type StoreNamespace } from "./store.ts";
import { allowedImage, MAX_IMAGE_BYTES } from "./image.ts";

export { RateLimiter } from "./limiter.ts";
export { EdgeStore } from "./edge.ts";

interface Env {
  ASSETS: Fetcher;
  LIMITER?: Namespace;
  STORE?: StoreNamespace;
}

const ID = /^[\w-]{3,40}$/;
const COPIED = ["content-type", "content-length", "content-range", "accept-ranges", "etag", "last-modified"];

function json(body: unknown, status = 200, cache = "no-store"): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": cache } });
}

async function proxy(upstream: string, request: Request, extra: Record<string, string> = {}, bound = false): Promise<Response> {
  const headers = new Headers(extra);
  const range = request.headers.get("range");
  const wanted = bound ? boundedRange(range) : range;
  if (wanted) headers.set("Range", wanted);
  const response = await fetch(upstream, { headers, signal: AbortSignal.timeout(25000) });
  const out = new Headers();
  for (const name of COPIED) {
    const value = response.headers.get(name);
    if (value) out.set(name, value);
  }
  if (!out.has("content-type")) out.set("content-type", "audio/mp4");
  out.set("accept-ranges", "bytes");
  out.set("cache-control", "private, max-age=600");
  return new Response(response.body, { status: response.status, headers: out });
}

const standIn = new Map<string, { url: string; until: number }>();

interface Resolved {
  url: string;
  source: "jiosaavn" | "youtube" | "embed";
}

async function standInFor(id: string, title: string, artist: string, duration: number): Promise<string> {
  const found = await remembered(`stand:${id}`, (value: { url: string }) => (value.url ? 1200 : 120), async () => {
    for (const query of searchQueries(title, artist)) {
      const match = pickMatch(await saavn.search(query, 10).catch(() => []), title, artist, duration);
      if (match) return { url: await saavn.streamUrl(match.id) };
    }
    return { url: "" };
  });
  return found.url;
}

async function resolve(source: string, id: string, url: URL): Promise<Resolved | null> {
  if (source !== "youtube") {
    const direct = await remembered(`direct:${id}`, 1200, async () => ({ url: await saavn.streamUrl(id) }));
    return { url: direct.url, source: "jiosaavn" };
  }
  try {
    await youtube.streamUrl(id);
    return { url: `/api/audio?${new URLSearchParams({ source: "youtube", id, t: url.searchParams.get("t") ?? "", a: url.searchParams.get("a") ?? "", d: url.searchParams.get("d") ?? "" })}`, source: "youtube" };
  } catch {
    const title = url.searchParams.get("t") ?? "";
    const stand = title ? await standInFor(id, title, url.searchParams.get("a") ?? "", Number(url.searchParams.get("d") ?? 0) || 0).catch(() => "") : "";
    return stand ? { url: stand, source: "jiosaavn" } : { url: `yt:${id}`, source: "embed" };
  }
}

async function youtubeAudio(request: Request, id: string, url: URL): Promise<Response> {
  const agent = { "User-Agent": youtube.VR_AGENT };
  for (const fresh of [false, true]) {
    try {
      const upstream = await youtube.streamUrl(id, fresh);
      const response = await proxy(upstream, request, agent, true);
      if (response.ok) {
        response.headers.set("x-aria-source", "youtube");
        return response;
      }
      youtube.forget(id);
      await response.body?.cancel();
    } catch (error) {
      if (!fresh && youtube.cached(id)?.error) break;
    }
  }
  const title = url.searchParams.get("t") ?? "";
  if (title) {
    try {
      let stand = standIn.get(id);
      if (!stand || stand.until < Date.now()) {
        const artist = url.searchParams.get("a") ?? "";
        const duration = Number(url.searchParams.get("d") ?? 0) || 0;
        const found = await remembered(`stand:${id}`, (value: { url: string }) => (value.url ? 1200 : 120), async () => {
          const match = pickMatch(await saavn.search(`${title} ${artist}`.trim(), 8), title, artist, duration);
          return { url: match ? await saavn.streamUrl(match.id) : "" };
        });
        stand = { url: found.url, until: Date.now() + 20 * 60_000 };
        if (standIn.size > 300) standIn.clear();
        standIn.set(id, stand);
      }
      if (stand.url) {
        const response = await proxy(stand.url, request, {}, true);
        response.headers.set("x-aria-source", "jiosaavn");
        return response;
      }
    } catch {}
  }
  return json({ error: youtube.cached(id)?.error ?? "YouTube would not provide this song" }, 502);
}

async function route(request: Request, url: URL): Promise<Response> {
  const source = url.searchParams.get("source") === "youtube" ? "youtube" : "jiosaavn";
  switch (url.pathname) {
    case "/api/search": {
      const query = (url.searchParams.get("q") ?? "").trim().slice(0, 120);
      if (!query) return json([]);
      const limit = Math.min(150, Math.max(1, Math.floor(Number(url.searchParams.get("limit"))) || 20));
      const songs = await remembered(`search:${source}:${limit}:${query.toLowerCase()}`, 300, () => (source === "youtube" ? youtube.search(query, limit) : saavn.search(query, limit)));
      return json(songs, 200, "public, max-age=300");
    }
    case "/api/trending": {
      const language = (url.searchParams.get("lang") ?? "hindi").replace(/[^a-z]/gi, "").slice(0, 20) || "hindi";
      return json(await remembered(`trending:${language}`, 900, () => saavn.trending(language)), 200, "public, max-age=900");
    }
    case "/api/region": {
      const place = (request as Request & { cf?: { country?: string; region?: string } }).cf;
      return json({ country_code: place?.country ?? "", region: place?.region ?? "" }, 200, "private, max-age=3600");
    }
    case "/api/resolve": {
      const id = url.searchParams.get("id") ?? "";
      if (!ID.test(id)) return json({ error: "bad id" }, 400);
      const found = await resolve(source, id, url);
      return found ? json(found) : json({ error: "YouTube is not allowing this server to play that song and JioSaavn does not have it yet" }, 502);
    }
    case "/api/lyrics": {
      const title = (url.searchParams.get("t") ?? "").slice(0, 200);
      const artist = (url.searchParams.get("a") ?? "").slice(0, 200);
      const length = Number(url.searchParams.get("d") ?? 0) || 0;
      const found = title
        ? await remembered(`lyrics:${title}|${artist}|${Math.round(length / 5)}`, (value: { lyrics: string | null }) => (value.lyrics ? 604800 : 600), async () => ({ lyrics: await lyrics.find(title, artist, length) }))
        : { lyrics: null };
      return json(found, 200, found.lyrics ? "public, max-age=86400" : "public, max-age=300");
    }
    case "/api/image": {
      const target = allowedImage(url.searchParams.get("u") ?? "");
      if (!target) return json({ error: "image host not allowed" }, 400);
      const upstream = await fetch(target.toString(), { headers: { "User-Agent": "Mozilla/5.0 Aria-Web" } });
      const type = upstream.headers.get("content-type") ?? "";
      const size = Number(upstream.headers.get("content-length") ?? 0);
      if (!upstream.ok || !type.startsWith("image/") || size > MAX_IMAGE_BYTES) return json({ error: "image unavailable" }, 502);
      return new Response(upstream.body, { headers: { "content-type": type, "cache-control": "public, max-age=604800" } });
    }
    case "/api/audio": {
      const id = url.searchParams.get("id") ?? "";
      if (!ID.test(id)) return json({ error: "bad id" }, 400);
      if (source === "youtube") return youtubeAudio(request, id, url);
      return proxy(await saavn.streamUrl(id), request);
    }
    default:
      return json({ error: "not found" }, 404);
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (!url.pathname.startsWith("/api/")) return env.ASSETS.fetch(request);
    if (request.method !== "GET" && request.method !== "HEAD") return json({ error: "method not allowed" }, 405);
    useCache(cacheFrom(env.STORE));
    const wait = await retryAfter(env.LIMITER, request, url.pathname);
    if (wait > 0) {
      const response = json({ error: `Too many requests, try again in ${wait} seconds` }, 429);
      response.headers.set("retry-after", String(wait));
      return response;
    }
    try {
      return await route(request, url);
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : "request failed" }, 502);
    }
  },
};
