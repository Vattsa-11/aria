import * as saavn from "./saavn.ts";
import * as youtube from "./youtube.ts";

interface Limiter {
  limit(options: { key: string }): Promise<{ success: boolean }>;
}

export interface Env {
  ASSETS: Fetcher;
  SEARCH_LIMIT?: Limiter;
  AUDIO_LIMIT?: Limiter;
}

const ID = /^[\w-]{3,40}$/;
const CHUNK = 1024 * 1024;
const COPIED = ["content-type", "content-length", "content-range", "accept-ranges", "etag", "last-modified"];

function json(body: unknown, status = 200, cache = "no-store"): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": cache } });
}

export function boundedRange(header: string | null, limit = CHUNK): string | null {
  const match = /^bytes=(\d+)-(\d*)$/.exec(header ?? "");
  if (!match) return header === null ? `bytes=0-${limit - 1}` : header;
  const start = Number(match[1]);
  const end = match[2] === "" ? Infinity : Number(match[2]);
  return `bytes=${start}-${Math.min(end, start + limit - 1)}`;
}

async function proxy(upstream: string, request: Request, extra: Record<string, string> = {}, bound = false): Promise<Response> {
  const headers = new Headers(extra);
  const range = request.headers.get("range");
  const wanted = bound ? boundedRange(range) : range;
  if (wanted) headers.set("Range", wanted);
  const response = await fetch(upstream, { headers });
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

export async function limited(env: Env, request: Request, pathname: string): Promise<boolean> {
  const limiter = pathname === "/api/audio" ? env.AUDIO_LIMIT : env.SEARCH_LIMIT;
  if (!limiter) return false;
  const client = request.headers.get("cf-connecting-ip") ?? "unknown";
  try {
    return !(await limiter.limit({ key: `${client}:${pathname}` })).success;
  } catch {
    return false;
  }
}

async function route(request: Request, url: URL): Promise<Response> {
  const source = url.searchParams.get("source") === "youtube" ? "youtube" : "jiosaavn";
  switch (url.pathname) {
    case "/api/search": {
      const query = (url.searchParams.get("q") ?? "").trim().slice(0, 120);
      if (!query) return json([]);
      const songs = source === "youtube" ? await youtube.search(query) : await saavn.search(query);
      return json(songs, 200, "public, max-age=300");
    }
    case "/api/trending": {
      const language = (url.searchParams.get("lang") ?? "hindi").replace(/[^a-z]/gi, "").slice(0, 20) || "hindi";
      return json(await saavn.trending(language), 200, "public, max-age=900");
    }
    case "/api/audio": {
      const id = url.searchParams.get("id") ?? "";
      if (!ID.test(id)) return json({ error: "bad id" }, 400);
      if (source === "youtube") return proxy(await youtube.streamUrl(id), request, { "User-Agent": youtube.VR_AGENT }, true);
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
    if (await limited(env, request, url.pathname)) {
      const response = json({ error: "Too many requests, try again in a minute" }, 429);
      response.headers.set("retry-after", "60");
      return response;
    }
    try {
      return await route(request, url);
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : "request failed" }, 502);
    }
  },
};
