import type { Song } from "./saavn.ts";

const words = (text: string): string[] =>
  text
    .toLowerCase()
    .replace(/\(.*?\)|\[.*?\]/g, " ")
    .replace(/official|video|audio|lyrics?|full song|hd|4k/g, " ")
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);

export function pickMatch(songs: Song[], title: string, artist: string, duration: number): Song | null {
  const wanted = new Set(words(title));
  if (wanted.size === 0) return null;
  let best: { song: Song; score: number } | null = null;
  for (const song of songs) {
    const have = new Set(words(song.title));
    const overlap = [...wanted].filter((word) => have.has(word)).length / wanted.size;
    const gap = duration > 0 && song.duration > 0 ? Math.abs(song.duration - duration) : 0;
    if (overlap < 0.6 || gap > 8) continue;
    const artistBonus = artist && words(artist).some((word) => words(song.artist).includes(word)) ? 0.3 : 0;
    const score = overlap + artistBonus - gap / 100;
    if (!best || score > best.score) best = { song, score };
  }
  return best?.song ?? null;
}
