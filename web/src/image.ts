const HOSTS = ["saavncdn.com", "ytimg.com", "ggpht.com", "googleusercontent.com"];
export const MAX_IMAGE_BYTES = 2 * 1024 * 1024;

export function allowedImage(raw: string): URL | null {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" || url.username || url.password || url.port) return null;
  const host = url.hostname.toLowerCase();
  return HOSTS.some((allowed) => host === allowed || host.endsWith(`.${allowed}`)) ? url : null;
}
