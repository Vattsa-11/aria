import type { Song } from "./saavn.ts";

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

export async function search(query: string, limit = 20): Promise<Song[]> {
  const response = await fetch(SEARCH, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: "https://music.youtube.com", "User-Agent": "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0" },
    body: JSON.stringify({ context: { client: { clientName: "WEB_REMIX", clientVersion: "1.20250310.01.00", hl: "en", gl: "US" } }, query, params: SONGS_FILTER }),
  });
  if (!response.ok) throw new Error(`YouTube search answered ${response.status}`);
  return parseSearch(await response.json(), limit);
}

export async function streamUrl(id: string): Promise<string> {
  const response = await fetch(PLAYER, {
    method: "POST",
    headers: { "Content-Type": "application/json", "User-Agent": VR_AGENT, "X-YouTube-Client-Name": "28", "X-YouTube-Client-Version": VR_VERSION },
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
