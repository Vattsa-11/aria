const $ = (id) => document.getElementById(id);
const audio = $("audio");
const store = {
  get(key, fallback) {
    try {
      return JSON.parse(localStorage.getItem(key)) ?? fallback;
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {}
  },
};

let source = store.get("source", "jiosaavn");
let songs = [];
let index = -1;
let liked = new Set(store.get("liked", []));

const key = (song) => `${song.source}:${song.id}`;
const clock = (secs) => (Number.isFinite(secs) && secs > 0 ? `${Math.floor(secs / 60)}:${String(Math.floor(secs % 60)).padStart(2, "0")}` : "0:00");
const status = (text) => ($("status").textContent = text);

async function api(path) {
  const response = await fetch(path);
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Request failed (${response.status})`);
  return body;
}

function row(song, position) {
  const item = document.createElement("li");
  item.className = position === index ? "now" : "";
  const cover = document.createElement("img");
  cover.loading = "lazy";
  cover.alt = "";
  cover.src = song.thumb;
  const info = document.createElement("div");
  info.className = "info";
  const title = document.createElement("div");
  title.className = "t";
  title.textContent = song.title;
  const artist = document.createElement("div");
  artist.className = "a";
  artist.textContent = song.artist;
  info.append(title, artist);
  const length = document.createElement("span");
  length.className = "d";
  length.textContent = song.duration ? clock(song.duration) : "";
  const heart = document.createElement("button");
  heart.className = `heart${liked.has(key(song)) ? " on" : ""}`;
  heart.setAttribute("aria-label", "Like");
  heart.textContent = liked.has(key(song)) ? "♥" : "♡";
  heart.addEventListener("click", (event) => {
    event.stopPropagation();
    liked.has(key(song)) ? liked.delete(key(song)) : liked.add(key(song));
    store.set("liked", [...liked]);
    render();
  });
  item.append(cover, info, length, heart);
  item.addEventListener("click", () => play(position));
  return item;
}

function render() {
  $("list").replaceChildren(...songs.map(row));
}

function show(list, message) {
  songs = list;
  index = -1;
  render();
  status(message);
}

async function load(path, label) {
  status("Loading...");
  try {
    const list = await api(path);
    show(list, list.length ? label : "Nothing found");
  } catch (error) {
    show([], error.message);
  }
}

const trending = () => load(`/api/trending?lang=${encodeURIComponent($("lang").value)}`, "Trending");

async function explain() {
  const song = songs[index];
  if (!song) return;
  try {
    const response = await fetch(`/api/audio?source=${song.source}&id=${encodeURIComponent(song.id)}`, { headers: { Range: "bytes=0-0" } });
    if (!response.ok) status(`Cannot play: ${(await response.json()).error}`);
  } catch {
    status("Cannot play: you may be offline");
  }
}

function play(position) {
  if (position < 0 || position >= songs.length) return;
  index = position;
  const song = songs[index];
  audio.src = `/api/audio?source=${song.source}&id=${encodeURIComponent(song.id)}`;
  audio.play().catch(() => {});
  $("player").hidden = false;
  $("title").textContent = song.title;
  $("artist").textContent = song.artist;
  $("cover").src = song.thumb;
  document.title = `${song.title} · Aria`;
  if ("mediaSession" in navigator) {
    navigator.mediaSession.metadata = new MediaMetadata({ title: song.title, artist: song.artist, artwork: [{ src: song.thumb, sizes: "512x512" }] });
  }
  render();
}

$("search").addEventListener("submit", (event) => {
  event.preventDefault();
  const query = $("q").value.trim();
  if (!query) return trending();
  store.set("query", query);
  load(`/api/search?source=${source}&q=${encodeURIComponent(query)}`, `Results for "${query}"`);
});

$("sources").addEventListener("click", (event) => {
  const chip = event.target.closest("[data-source]");
  if (!chip) return;
  source = chip.dataset.source;
  store.set("source", source);
  for (const other of document.querySelectorAll("[data-source]")) other.classList.toggle("on", other === chip);
  if ($("q").value.trim()) $("search").requestSubmit();
});

$("lang").value = store.get("lang", "hindi");
$("lang").addEventListener("change", () => {
  store.set("lang", $("lang").value);
  trending();
});

$("toggle").addEventListener("click", () => (audio.paused ? audio.play() : audio.pause()));
$("next").addEventListener("click", () => play(index + 1));
$("prev").addEventListener("click", () => (audio.currentTime > 4 ? (audio.currentTime = 0) : play(index - 1)));
$("bar").addEventListener("input", () => {
  if (audio.duration) audio.currentTime = (Number($("bar").value) / 1000) * audio.duration;
});

audio.addEventListener("play", () => ($("toggle").textContent = "⏸"));
audio.addEventListener("pause", () => ($("toggle").textContent = "▶"));
audio.addEventListener("ended", () => play(index + 1));
audio.addEventListener("error", explain);
audio.addEventListener("timeupdate", () => {
  $("now").textContent = clock(audio.currentTime);
  $("total").textContent = clock(audio.duration);
  if (audio.duration) $("bar").value = Math.round((audio.currentTime / audio.duration) * 1000);
});

if ("mediaSession" in navigator) {
  navigator.mediaSession.setActionHandler("play", () => audio.play());
  navigator.mediaSession.setActionHandler("pause", () => audio.pause());
  navigator.mediaSession.setActionHandler("nexttrack", () => play(index + 1));
  navigator.mediaSession.setActionHandler("previoustrack", () => play(index - 1));
}

document.addEventListener("keydown", (event) => {
  if (event.target.matches("input, select") || event.code !== "Space" || $("player").hidden) return;
  event.preventDefault();
  audio.paused ? audio.play() : audio.pause();
});

for (const chip of document.querySelectorAll("[data-source]")) chip.classList.toggle("on", chip.dataset.source === source);
$("q").value = store.get("query", "");
trending();

if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js").catch(() => {});
