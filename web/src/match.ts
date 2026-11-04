import type { Song } from "./saavn.ts";

const NOISE = /official|video|audio|lyrics?|full song|hd|4k|remastered|from the|original motion picture soundtrack/g;

const words = (text: string): string[] =>
  text
    .toLowerCase()
    .replace(/\(.*?\)|\[.*?\]/g, " ")
    .replace(NOISE, " ")
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);

export function cleanQuery(title: string): string {
  return words(title).join(" ");
}

export function searchQueries(title: string, artist: string): string[] {
  const clean = cleanQuery(title);
  const firstArtist = artist.split(/[,&]| feat\.?| ft\.?/i)[0]?.trim() ?? "";
  const list = [`${clean} ${firstArtist}`.trim(), clean, `${title} ${firstArtist}`.trim()];
  return [...new Set(list.filter(Boolean))];
}

function score(song: Song, wanted: Set<string>, artist: string, duration: number): { overlap: number; artistMatch: boolean; gap: number } {
  const have = new Set(words(song.title));
  const overlap = [...wanted].filter((word) => have.has(word)).length / wanted.size;
  const artistWords = words(artist);
  const songArtist = words(song.artist);
  return {
    overlap,
    artistMatch: artistWords.length > 0 && artistWords.some((word) => songArtist.includes(word)),
    gap: duration > 0 && song.duration > 0 ? Math.abs(song.duration - duration) : 0,
  };
}

export function pickMatch(songs: Song[], title: string, artist: string, duration: number): Song | null {
  const wanted = new Set(words(title));
  if (wanted.size === 0) return null;
  let best: { song: Song; rank: number } | null = null;
  for (const song of songs) {
    const found = score(song, wanted, artist, duration);
    const strict = found.overlap >= 0.6 && found.gap <= 8;
    const relaxed = found.overlap >= 0.8 && found.artistMatch && found.gap <= 25;
    if (!strict && !relaxed) continue;
    const rank = found.overlap + (found.artistMatch ? 0.3 : 0) - found.gap / 100;
    if (!best || rank > best.rank) best = { song, rank };
  }
  return best?.song ?? null;
}
