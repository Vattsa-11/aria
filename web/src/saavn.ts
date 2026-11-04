import des from "des.js";

const KEY = "38346591";
const API = "https://www.jiosaavn.com/api.php";
const PARAMS = "_format=json&_marker=0&api_version=4&ctx=web6dot0";

export interface Song {
  id: string;
  title: string;
  artist: string;
  duration: number;
  thumb: string;
  source: "jiosaavn" | "youtube";
}

type Json = Record<string, any>;

export function unescapeHtml(text: string): string {
  return text
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'")
    .replaceAll("&#39;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

export function decryptMedia(token: string): string {
  const raw = Uint8Array.from(atob(token), (char) => char.charCodeAt(0));
  if (raw.length === 0 || raw.length % 8 !== 0) throw new Error("unexpected token length");
  const cipher = des.DES.create({ type: "decrypt", key: Array.from(new TextEncoder().encode(KEY)), padding: false });
  const plain = Uint8Array.from([...cipher.update(Array.from(raw)), ...cipher.final()]);
  const pad = plain[plain.length - 1];
  const end = pad >= 1 && pad <= 8 && plain.length >= pad ? plain.length - pad : plain.length;
  return new TextDecoder().decode(plain.subarray(0, end));
}

export function upgradeQuality(url: string, high: boolean): string {
  return high && url.includes("_96.") ? url.replace("_96.", "_320.") : url;
}

function artistOf(entry: Json): string {
  const mapped = entry.more_info?.artistMap?.primary_artists;
  if (Array.isArray(mapped) && mapped.length > 0) return mapped.map((a: Json) => a.name).join(", ");
  if (typeof entry.primaryArtists === "string" && entry.primaryArtists) return entry.primaryArtists;
  if (typeof entry.subtitle === "string") return entry.subtitle.split(" - ")[0].trim();
  return "";
}

function entriesOf(json: Json): Json[] {
  if (Array.isArray(json)) return json;
  return json.results ?? json.data?.results ?? json.songs ?? [];
}

export function parseSearch(json: Json): Song[] {
  const songs: Song[] = [];
  for (const entry of entriesOf(json)) {
    if (!entry?.id) continue;
    const image = typeof entry.image === "string" ? entry.image : "";
    songs.push({
      id: String(entry.id),
      title: unescapeHtml(String(entry.title ?? entry.name ?? "Unknown title")),
      artist: unescapeHtml(artistOf(entry)),
      duration: Number(entry.more_info?.duration ?? entry.duration ?? 0) || 0,
      thumb: image.replace(/(50x50|150x150)/, "500x500"),
      source: "jiosaavn",
    });
  }
  return songs;
}

export function parseStreamUrl(json: Json): string {
  const entry = entriesOf(json)[0];
  const info = entry?.more_info;
  const token = info?.encrypted_media_url ?? info?.encrypted_drm_media_url;
  if (!token) throw new Error("JioSaavn returned no media URL");
  const high = info["320kbps"] === true || info["320kbps"] === "true";
  return upgradeQuality(decryptMedia(token), high);
}

async function call(query: string): Promise<Json> {
  const response = await fetch(`${API}?${query}&${PARAMS}`, { headers: { "User-Agent": "Mozilla/5.0 Aria-Web" } });
  if (!response.ok) throw new Error(`JioSaavn answered ${response.status}`);
  return JSON.parse(await response.text());
}

export async function search(query: string, limit = 20): Promise<Song[]> {
  return parseSearch(await call(`__call=search.getResults&q=${encodeURIComponent(query)}&n=${limit}`));
}

export async function trending(language: string): Promise<Song[]> {
  const json = await call(`__call=content.getTrending&entity_type=song&entity_language=${encodeURIComponent(language)}`);
  return parseSearch(json);
}

export async function streamUrl(id: string): Promise<string> {
  return parseStreamUrl(await call(`__call=song.getDetails&pids=${encodeURIComponent(id)}`));
}
