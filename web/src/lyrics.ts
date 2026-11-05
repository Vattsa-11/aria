const LRCLIB = "https://lrclib.net/api";

interface Hit {
  syncedLyrics?: string | null;
  plainLyrics?: string | null;
  duration?: number | null;
}

const NOISE = ["official", "lyric", "video", "audio", "visualizer", "full song", "hd", "4k", "remaster", "feat", "ft.", "ft ", "with ", "from "];

export function cleanTitle(title: string): string {
  const withoutTags = title.replace(/[(\[][^)\]]*[)\]]/g, (group) => (NOISE.some((noise) => group.toLowerCase().includes(noise)) ? "" : group));
  const cut = withoutTags.includes(" | ") ? withoutTags.slice(0, withoutTags.indexOf(" | ")) : withoutTags;
  return cut.replace(/\s+/g, " ").trim();
}

function firstArtist(artist: string): string {
  const lower = artist.toLowerCase();
  let cut = artist.length;
  for (const separator of [",", " & ", " feat", " ft.", " x ", " × ", " and ", " with "]) {
    const at = lower.indexOf(separator);
    if (at >= 0) cut = Math.min(cut, at);
  }
  return artist.slice(0, cut).trim();
}

export function variants(title: string, artist: string): Array<[string, string]> {
  const clean = cleanTitle(title);
  const who = artist.trim().replace(/ - Topic$/, "").trim();
  const found: Array<[string, string]> = [];
  const add = (name: string, by: string) => {
    if (name && !found.some(([t, a]) => t.toLowerCase() === name.toLowerCase() && a.toLowerCase() === by.toLowerCase())) found.push([name, by]);
  };
  const dash = clean.indexOf(" - ");
  if (dash > 0) {
    const left = clean.slice(0, dash).trim();
    const right = clean.slice(dash + 3).trim();
    if (!who || who.toLowerCase() === left.toLowerCase() || left.toLowerCase().includes(who.toLowerCase())) {
      add(right, firstArtist(left));
      add(right, left);
    }
    add(right, who);
  }
  add(clean, who);
  add(clean, firstArtist(who));
  add(clean, "");
  return found;
}

export function pick(hit: Hit | undefined | null): string | null {
  if (!hit) return null;
  if (hit.syncedLyrics?.trim()) return hit.syncedLyrics;
  return hit.plainLyrics?.trim() ? hit.plainLyrics : null;
}

export function best(hits: Hit[], duration = 0): string | null {
  const close = (hit: Hit) => duration <= 0 || hit.duration == null || Math.abs(hit.duration - duration) <= 4;
  const synced = (hit: Hit) => Boolean(hit.syncedLyrics?.trim());
  const chosen = hits.find((hit) => close(hit) && synced(hit)) ?? hits.find((hit) => close(hit) && pick(hit)) ?? hits.find(synced) ?? hits.find((hit) => pick(hit));
  return pick(chosen);
}

async function get(url: string): Promise<unknown> {
  const response = await fetch(url, { headers: { "User-Agent": "Aria-Web (https://github.com/Vattsa-11/aria)" }, signal: AbortSignal.timeout(8000) });
  return response.ok ? response.json() : null;
}

export async function find(title: string, artist: string, duration: number): Promise<string | null> {
  const tries = variants(title, artist);
  if (tries.length === 0) return null;
  for (const [name, by] of tries.slice(0, 4)) {
    const exact = new URLSearchParams({ track_name: name, artist_name: by });
    if (duration > 0) exact.set("duration", String(Math.round(duration)));
    const direct = pick((await get(`${LRCLIB}/get?${exact}`).catch(() => null)) as Hit | null);
    if (direct) return direct;
    const fields = (await get(`${LRCLIB}/search?${new URLSearchParams({ track_name: name, artist_name: by })}`).catch(() => null)) as Hit[] | null;
    const hit = Array.isArray(fields) ? best(fields, duration) : null;
    if (hit) return hit;
  }
  const [name, by] = tries[0];
  const loose = (await get(`${LRCLIB}/search?${new URLSearchParams({ q: `${by} ${name}`.trim() })}`).catch(() => null)) as Hit[] | null;
  return Array.isArray(loose) ? best(loose, duration) : null;
}
