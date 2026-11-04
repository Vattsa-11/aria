(function () {
  const ua = navigator.userAgent || "";
  const ios = /iphone|ipad|ipod/i.test(ua) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
  const android = /android/i.test(ua);
  const inApp = /FBAN|FBAV|Instagram|WhatsApp|Line\/|MicroMessenger|Snapchat|Telegram|GSA\//i.test(ua);
  const params = new URLSearchParams(location.search);
  const standalone = matchMedia("(display-mode: standalone)").matches || matchMedia("(display-mode: fullscreen)").matches || navigator.standalone === true;
  let bypass = params.get("browser") === "1";
  try { if (bypass) sessionStorage.setItem("aria.browser", "1"); bypass = bypass || sessionStorage.getItem("aria.browser") === "1"; } catch {}
  const mobile = ios || android;
  window.__ariaBlocked = mobile && !standalone && !bypass;
  window.__ariaPlatform = ios ? "ios" : android ? "android" : "pc";
  if (!window.__ariaBlocked) return;

  let prompt = window.__ariaPrompt || null;
  window.addEventListener("beforeinstallprompt", (event) => { event.preventDefault(); prompt = event; window.__ariaPrompt = event; render(); });
  window.addEventListener("appinstalled", () => { prompt = null; installed = true; render(); });
  let installed = false;

  const style = document.createElement("style");
  style.textContent = `
    #wall{position:fixed;inset:0;z-index:99999;background:#0e0f13;color:#f2f3f5;font:16px/1.45 system-ui,-apple-system,sans-serif;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;padding:28px 24px;text-align:center;overflow:auto}
    #wall img{width:92px;height:92px;border-radius:22px}
    #wall h1{margin:6px 0 0;font-size:1.5rem}
    #wall p{margin:0;max-width:340px;color:#9aa0ab}
    #wall ol{margin:6px 0 0;padding:0;list-style:none;text-align:left;max-width:340px;display:flex;flex-direction:column;gap:10px}
    #wall li{background:#16181e;border:1px solid #262a33;border-radius:14px;padding:12px 14px}
    #wall li b{color:#4f8cff}
    #wall button{font:inherit;color:#fff;background:#4f8cff;border:0;border-radius:14px;padding:14px 28px;min-height:48px;font-weight:600;cursor:pointer}
    #wall small{color:#6b7280;max-width:320px}
  `;
  document.head.append(style);
  const wall = document.createElement("div");
  wall.id = "wall";
  document.addEventListener("DOMContentLoaded", () => document.body.append(wall));
  if (document.body) document.body.append(wall);

  function steps(items) {
    return `<ol>${items.map((item, i) => `<li><b>${i + 1}.</b> ${item}</li>`).join("")}</ol>`;
  }

  function render() {
    let body;
    if (installed) {
      body = `<h1>Aria is installed</h1><p>Open <b>Aria</b> from your home screen to start listening.</p>`;
    } else if (inApp) {
      body = `<h1>Open in your browser</h1><p>This link was opened inside another app, which cannot install Aria.</p>` +
        steps([ios ? "Tap the <b>Share</b> or <b>···</b> button" : "Tap the <b>⋮</b> menu", ios ? "Choose <b>Open in Safari</b>" : "Choose <b>Open in Chrome</b>", "Then install Aria from there"]);
    } else if (prompt) {
      body = `<h1>Install Aria</h1><p>Aria works as an app: it opens full screen, plays in the background and keeps your music on this device.</p><button id="install">Install Aria</button>`;
    } else if (ios) {
      body = `<h1>Add Aria to your Home Screen</h1><p>Aria runs as an app on your iPhone. It takes a few seconds:</p>` +
        steps(["Tap the <b>Share</b> button (the square with an arrow) in Safari", "Scroll down and tap <b>Add to Home Screen</b>", "Tap <b>Add</b>, then open <b>Aria</b> from your Home Screen"]);
    } else {
      body = `<h1>Install Aria</h1><p>Aria runs as an app on your phone:</p>` +
        steps(["Open your browser menu <b>⋮</b>", "Tap <b>Install app</b> or <b>Add to Home screen</b>", "Open <b>Aria</b> from your home screen"]);
    }
    wall.innerHTML = `<img src="/icon-192.png" alt="">${body}<small>This installs Aria like a normal app. It uses no store, and you can remove it any time.</small>`;
    const button = document.getElementById("install");
    if (button) button.onclick = async () => {
      prompt.prompt();
      const choice = await prompt.userChoice;
      if (choice.outcome === "accepted") { installed = true; prompt = null; }
      render();
    };
  }
  render();
  matchMedia("(display-mode: standalone)").addEventListener?.("change", (event) => { if (event.matches) location.reload(); });
})();
