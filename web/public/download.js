const DIRECT_CHUNK = 2 * 1024 * 1024;

export function cacheKey(id) {
  return typeof location === "undefined" ? `http://localhost/offline/${id}` : `/offline/${id}`;
}

export async function downloadAudio({ id, url, progress, cache, fetchImpl = fetch, wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms)), timeoutMs = 30000, tries = 4 }) {
  const proxied = url.startsWith("/api/audio");
  const parts = [];
  let done = 0;
  let size = 0;

  const get = async (range) => {
    for (let attempt = 0; ; attempt++) {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), timeoutMs);
      try {
        const response = await fetchImpl(url, { headers: { Range: range }, signal: controller.signal });
        if (response.status === 416 && parts.length > 0) return { status: 416, range: null, buffer: new ArrayBuffer(0) };
        if (response.status !== 206 && response.status !== 200) throw new Error(`Server answered ${response.status}`);
        const buffer = await response.arrayBuffer();
        return { status: response.status, range: response.headers.get("content-range"), buffer };
      } catch (error) {
        if (attempt >= tries - 1) {
          throw new Error(controller.signal.aborted ? "The connection timed out. Check your signal and try again." : error.message);
        }
        await wait(700 * 2 ** attempt);
      } finally {
        clearTimeout(timer);
      }
    }
  };

  for (;;) {
    const end = proxied ? "" : done + DIRECT_CHUNK - 1;
    const result = await get(`bytes=${done}-${end}`);
    if (result.status === 200 && parts.length > 0) throw new Error("This server does not support resuming a download");
    const total = Number((result.range || "").split("/")[1]) || 0;
    if (total) size = total;
    if (result.buffer.byteLength === 0) break;
    parts.push(result.buffer);
    done += result.buffer.byteLength;
    progress(done, size);
    if (result.status === 200 || (size && done >= size) || (!proxied && result.buffer.byteLength < DIRECT_CHUNK)) break;
  }
  if (done < 50000) throw new Error("The download came back empty");
  const blob = new Blob(parts, { type: "audio/mp4" });
  await cache.put(new Request(cacheKey(id)), new Response(blob, { headers: { "content-type": "audio/mp4", "content-length": String(blob.size) } }));
  return blob.size;
}
