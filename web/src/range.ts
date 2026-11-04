export const CHUNK = 1024 * 1024;

export function boundedRange(header: string | null, limit = CHUNK): string | null {
  const match = /^bytes=(\d+)-(\d*)$/.exec(header ?? "");
  if (!match) return header === null ? `bytes=0-${limit - 1}` : header;
  const start = Number(match[1]);
  const end = match[2] === "" ? Infinity : Number(match[2]);
  return `bytes=${start}-${Math.min(end, start + limit - 1)}`;
}
