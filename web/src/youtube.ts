import type { Song } from "./saavn.ts";
import { remembered } from "./store.ts";

const SEARCH = "https://music.youtube.com/youtubei/v1/search?prettyPrint=false";
const PLAYER = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const SONGS_FILTER = "EgWKAQIIAWoKEAkQBRAKEAMQBA==";
const VR_VERSION = "1.60.19";
export const VR_AGENT = `com.google.android.apps.youtube.vr.oculus/${VR_VERSION} (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip`;

type Json = Record<string, any>;

function runsOf(column: Json | undefined): string[] {
  const runs = column?.musicResponsiveListItemFlexColumnRenderer?.text?.runs;
  return Array.isArray(runs) ? runs.map((run: Json) => String(run.text ?? "")) : [];
}

export function parseClock(text: string): number {
  const parts = text.split(":").map((part) => Number(part.trim()));
  if (parts.length < 2 || parts.some((part) => !Number.isInteger(part))) return 0;
  return parts.reduce((total, part) => total * 60 + part, 0);
}

function collect(node: unknown, found: Json[]): void {
  if (Array.isArray(node)) {
    node.forEach((child) => collect(child, found));
  } else if (node && typeof node === "object") {
    const item = (node as Json).musicResponsiveListItemRenderer;
    if (item) found.push(item);
    Object.values(node).forEach((child) => collect(child, found));
  }
}

export function parseSearch(response: Json, limit = 20): Song[] {
  const items: Json[] = [];
  collect(response, items);
  const songs: Song[] = [];
  for (const item of items) {
    const id = item.playlistItemData?.videoId;
    const columns: Json[] = item.flexColumns ?? [];
    const title = runsOf(columns[0]).join("");
    if (!id || !title) continue;
    const sub = runsOf(columns[1]);
    songs.push({
      id,
      title,
      artist: sub[0] ?? "",
      duration: parseClock(sub[sub.length - 1] ?? ""),
      thumb: `https://i.ytimg.com/vi/${id}/hqdefault.jpg`,
      source: "youtube",
    });
    if (songs.length >= limit) break;
  }
  return songs;
}

export function pickAudio(response: Json): string {
  const status = response.playabilityStatus?.status;
  if (status !== "OK") throw new Error(`YouTube refused this video (${status}): ${response.playabilityStatus?.reason ?? "not playable"}`);
  const formats: Json[] = response.streamingData?.adaptiveFormats ?? [];
  const aac = formats.filter((format) => String(format.mimeType).startsWith("audio/mp4") && format.url);
  if (aac.length === 0) throw new Error("YouTube offered no AAC audio for this video");
  return aac.reduce((best, format) => (format.bitrate > best.bitrate ? format : best)).url;
}

export function continuationOf(response: Json): string | null {
  const text = JSON.stringify(response);
  return /"nextContinuationData":\{"continuation":"([^"]+)"/.exec(text)?.[1] ?? /"continuationCommand":\{"token":"([^"]+)"/.exec(text)?.[1] ?? null;
}

const SEARCH_HEADERS = { "Content-Type": "application/json", Origin: "https://music.youtube.com", "User-Agent": "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0" };
const WEB_REMIX = { client: { clientName: "WEB_REMIX", clientVersion: "1.20250310.01.00", hl: "en", gl: "US" } };

type Walk = { songs: Song[]; token: string | null | undefined; until: number };
const walks = new Map<string, Walk>();

export async function search(query: string, limit = 60): Promise<Song[]> {
  const key = query.toLowerCase();
  const now = Date.now();
  let walk = walks.get(key);
  if (!walk || walk.until < now) {
    if (walks.size > 60) walks.clear();
    walk = { songs: [], token: undefined, until: now + 10 * 60_000 };
    walks.set(key, walk);
  }
  const seen = new Set(walk.songs.map((song) => song.id));
  while (walk.songs.length < limit && walk.token !== null) {
    const first = walk.token === undefined;
    const token: string | undefined = walk.token;
    const url: string = first ? SEARCH : `${SEARCH}&ctoken=${encodeURIComponent(token!)}&continuation=${encodeURIComponent(token!)}`;
    const response = await fetch(url, {
      method: "POST",
      headers: SEARCH_HEADERS,
      body: JSON.stringify(first ? { context: WEB_REMIX, query, params: SONGS_FILTER } : { context: WEB_REMIX }),
    });
    if (!response.ok) {
      if (first) throw new Error(`YouTube search answered ${response.status}`);
      break;
    }
    const json: Json = await response.json();
    for (const song of parseSearch(json, 1000)) {
      if (!seen.has(song.id)) {
        seen.add(song.id);
        walk.songs.push(song);
      }
    }
    walk.token = continuationOf(json);
  }
  return walk.songs.slice(0, limit);
}

function lockups(node: unknown, found: Json[]): void {
  if (Array.isArray(node)) {
    node.forEach((child) => lockups(child, found));
  } else if (node && typeof node === "object") {
    const item = (node as Json).lockupViewModel;
    if (item?.contentType === "LOCKUP_CONTENT_TYPE_VIDEO" && item.contentId) found.push(item);
    else Object.values(node).forEach((child) => lockups(child, found));
  }
}

function badgeText(node: unknown): string {
  if (Array.isArray(node)) {
    for (const child of node) {
      const text = badgeText(child);
      if (text) return text;
    }
  } else if (node && typeof node === "object") {
    const badge = (node as Json).thumbnailBadgeViewModel;
    if (badge && typeof badge.text === "string" && /\d:\d\d/.test(badge.text)) return badge.text;
    for (const child of Object.values(node)) {
      const text = badgeText(child);
      if (text) return text;
    }
  }
  return "";
}

export function parsePlaylist(response: Json): Song[] {
  const items: Json[] = [];
  lockups(response, items);
  return items.map((item) => {
    const id = String(item.contentId);
    const title = String(item.metadata?.lockupMetadataViewModel?.title?.content ?? "").trim();
    const artist = String(item.metadata?.lockupMetadataViewModel?.metadata?.contentMetadataViewModel?.metadataRows?.[0]?.metadataParts?.[0]?.text?.content ?? "").replace(/ - Topic$/, "");
    return { id, title, artist, duration: parseClock(badgeText(item.contentImage)), thumb: `https://i.ytimg.com/vi/${id}/hqdefault.jpg`, source: "youtube" as const };
  }).filter((song) => song.title);
}

function playlistToken(response: Json): string | null {
  const text = JSON.stringify(response);
  return /"continuationCommand":\{"token":"([^"]+)"/.exec(text)?.[1] ?? null;
}

const WEB_CLIENT = { client: { clientName: "WEB", clientVersion: "2.20250310.01.00", hl: "en", gl: "US" } };
const BROWSE = "https://www.youtube.com/youtubei/v1/browse?prettyPrint=false";

export async function playlist(id: string, limit = 200): Promise<Song[]> {
  const songs: Song[] = [];
  const seen = new Set<string>();
  let body: Json = { context: WEB_CLIENT, browseId: `VL${id}` };
  for (let page = 0; page < 4 && songs.length < limit; page++) {
    const response = await fetch(BROWSE, { method: "POST", headers: { "Content-Type": "application/json", "User-Agent": SEARCH_HEADERS["User-Agent"] }, body: JSON.stringify(body) });
    if (!response.ok) {
      if (page === 0) throw new Error(`YouTube answered ${response.status} for this playlist`);
      break;
    }
    const json: Json = await response.json();
    for (const song of parsePlaylist(json)) {
      if (!seen.has(song.id)) {
        seen.add(song.id);
        songs.push(song);
      }
    }
    const token = playlistToken(json);
    if (!token) break;
    body = { context: WEB_CLIENT, continuation: token };
  }
  return songs.slice(0, limit);
}

const TTL_OK = 25 * 60_000;
const TTL_REFUSED = 45_000;
const memory = new Map<string, { url?: string; error?: string; until: number }>();

export function cached(id: string, now = Date.now()): { url?: string; error?: string } | null {
  const hit = memory.get(id);
  if (!hit || hit.until <= now) {
    memory.delete(id);
    return null;
  }
  return hit;
}

export function remember(id: string, value: { url?: string; error?: string }, now = Date.now()): void {
  if (memory.size > 500) memory.clear();
  memory.set(id, { ...value, until: now + (value.url ? TTL_OK : TTL_REFUSED) });
}

export function forget(id: string): void {
  memory.delete(id);
}

async function lookup(id: string): Promise<string> {
  const response = await fetch(PLAYER, {
    method: "POST",
    headers: { "Content-Type": "application/json", "User-Agent": VR_AGENT, "X-YouTube-Client-Name": "28", "X-YouTube-Client-Version": VR_VERSION },
    signal: AbortSignal.timeout(8000),
    body: JSON.stringify({
      context: { client: { clientName: "ANDROID_VR", clientVersion: VR_VERSION, deviceMake: "Oculus", deviceModel: "Quest 3", androidSdkVersion: 32, osName: "Android", osVersion: "12L", hl: "en", gl: "US" } },
      videoId: id,
      contentCheckOk: true,
      racyCheckOk: true,
    }),
  });
  if (!response.ok) throw new Error(`YouTube answered ${response.status}`);
  return pickAudio(await response.json());
}

export async function streamUrl(id: string, fresh = false): Promise<string> {
  if (fresh) forget(id);
  const hit = cached(id);
  if (hit) {
    if (hit.url) return hit.url;
    throw new Error(hit.error ?? "YouTube refused this video");
  }
  const outcome = await (fresh
    ? lookupSafe(id)
    : remembered(`yt:${id}`, (value: { url?: string }) => (value.url ? TTL_OK / 1000 : TTL_REFUSED / 1000), () => lookupSafe(id)));
  remember(id, outcome);
  if (outcome.url) return outcome.url;
  throw new Error(outcome.error ?? "YouTube refused this video");
}

async function lookupSafe(id: string): Promise<{ url?: string; error?: string }> {
  try {
    return { url: await lookup(id) };
  } catch (error) {
    return { error: error instanceof Error ? error.message : "YouTube lookup failed" };
  }
}
