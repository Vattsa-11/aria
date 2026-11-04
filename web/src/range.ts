export const CHUNK = 1024 * 1024;
export const FIRST_CHUNK = 384 * 1024;

export function boundedRange(header: string | null, limit = CHUNK, first = FIRST_CHUNK): string | null {
  const match = /^bytes=(\d+)-(\d*)$/.exec(header ?? "");
  if (!match) return header === null ? `bytes=0-${first - 1}` : header;
  const start = Number(match[1]);
  const end = match[2] === "" ? Infinity : Number(match[2]);
  const size = start === 0 ? first : limit;
  return `bytes=${start}-${Math.min(end, start + size - 1)}`;
}
