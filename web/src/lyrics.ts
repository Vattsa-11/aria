const LRCLIB = "https://lrclib.net/api";

interface Hit {
  syncedLyrics?: string | null;
  plainLyrics?: string | null;
}

export function cleanTitle(title: string): string {
  return title
    .replace(/\s*[(\[](official[^)\]]*|lyric[^)\]]*|lyrics[^)\]]*|audio[^)\]]*|video[^)\]]*|full song[^)\]]*|hd|4k)[)\]]/gi, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function pick(hit: Hit | undefined | null): string | null {
  if (!hit) return null;
  if (hit.syncedLyrics?.trim()) return hit.syncedLyrics;
  return hit.plainLyrics?.trim() ? hit.plainLyrics : null;
}

export function best(hits: Hit[]): string | null {
  const synced = hits.find((hit) => hit.syncedLyrics?.trim());
  return pick(synced ?? hits.find((hit) => pick(hit)));
}

async function get(url: string): Promise<unknown> {
  const response = await fetch(url, { headers: { "User-Agent": "Aria-Web (https://github.com/magi8101/aria)" }, signal: AbortSignal.timeout(8000) });
  return response.ok ? response.json() : null;
}

export async function find(title: string, artist: string, duration: number): Promise<string | null> {
  const name = cleanTitle(title);
  if (!name) return null;
  const exact = new URLSearchParams({ track_name: name, artist_name: artist.trim() });
  if (duration > 0) exact.set("duration", String(Math.round(duration)));
  const direct = pick((await get(`${LRCLIB}/get?${exact}`).catch(() => null)) as Hit | null);
  if (direct) return direct;
  const results = (await get(`${LRCLIB}/search?${new URLSearchParams({ q: `${artist} ${name}`.trim() })}`).catch(() => null)) as Hit[] | null;
  return Array.isArray(results) ? best(results) : null;
}
