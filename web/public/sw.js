const CACHE = "aria-shell-v2";
const SHELL = ["/", "/classic", "/style.css", "/app.js", "/manifest.webmanifest", "/icon-192.png", "/icon-512.png", "/pkg/aria.js", "/pkg/aria_bg.wasm"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((names) => Promise.all(names.filter((name) => name !== CACHE).map((name) => caches.delete(name)))).then(() => self.clients.claim()),
  );
});

async function update(request) {
  const response = await fetch(request, { cache: "no-cache" });
  if (response.ok) (await caches.open(CACHE)).put(request, response.clone());
  return response;
}

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "GET" || url.origin !== location.origin || url.pathname.startsWith("/api/") || url.pathname === "/version.json" || url.pathname.startsWith("/offline/")) return;
  event.respondWith(update(event.request).catch(() => caches.match(event.request).then((hit) => hit || caches.match("/"))));
});
