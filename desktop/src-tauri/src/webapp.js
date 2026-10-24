// Läuft in jeder Seite des Hauptfensters, wirkt aber nur in der Web-App (localhost:__PORT__). Eingebunden von lib.rs.
// 1. Meldet die Oberflächensprache an die Hülle (Tray und Startseite folgen).
// 2. Leitet Downloads (Song, ZIP, Lyrics) an die Hülle, die sie im Downloads-Ordner speichert.
// 3. Ergänzt im Einstellungsdialog den Eintrag „Speicher & Modelle“.
(() => {
  if (!["localhost", "127.0.0.1"].includes(location.hostname) || location.port !== "__PORT__") return;
  window.__MG_DESKTOP__ = true;
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

  const sendLang = () => {
    try {
      const l = typeof LANG === "string" ? LANG : localStorage.getItem("ui_lang");
      if (l) invoke("set_lang", { lang: l });
    } catch (e) {}
  };
  document.addEventListener("langchange", () => { sendLang(); refreshStorage(); });
  window.addEventListener("load", sendLang);

  // --- Meldung unten in der Mitte, für Screenreader als Statusmeldung
  let box, timer;
  function toast(text, opts = {}) {
    if (!box) {
      box = document.createElement("div");
      box.setAttribute("role", "status");
      box.style.cssText = "position:fixed;left:50%;bottom:24px;transform:translateX(-50%);z-index:2147483647;" +
        "display:flex;gap:12px;align-items:center;max-width:min(92vw,640px);padding:10px 16px;border-radius:999px;" +
        "background:#1a1d24;color:#eef0f4;border:1px solid #2a2e38;box-shadow:0 4px 18px rgba(0,0,0,.5);font:14px/1.4 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif";
      document.body.appendChild(box);
    }
    box.replaceChildren();
    const span = document.createElement("span");
    span.textContent = text;
    span.style.cssText = "overflow-wrap:anywhere" + (opts.error ? ";color:#ff6b6b" : "");
    box.appendChild(span);
    if (opts.action) {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = opts.action.label;
      b.style.cssText = "font:inherit;color:#111214;background:#b09f84;border:0;border-radius:999px;padding:4px 12px;cursor:pointer;white-space:nowrap";
      b.onclick = opts.action.run;
      box.appendChild(b);
    }
    box.hidden = false;
    clearTimeout(timer);
    if (!opts.sticky) timer = setTimeout(() => { box.hidden = true; }, opts.error ? 10000 : 7000);
  }

  // --- Downloads: nach den Handlern der Web-App (sie verhindert z. B. ein leeres ZIP), daher Bubble-Phase am window
  const isDownload = (a) => a.hasAttribute("download") ||
    (a.origin === location.origin && (a.pathname === "/api/download" || new URLSearchParams(a.search).get("download") === "1"));
  window.addEventListener("click", async (e) => {
    const a = e.target instanceof Element && e.target.closest("a[href]");
    if (!a || e.defaultPrevented || !isDownload(a)) return;
    e.preventDefault();
    toast("↓ " + (a.getAttribute("download") || a.textContent.trim() || "…"), { sticky: true });
    try {
      const r = await invoke("save_download", { url: a.href, name: a.getAttribute("download") });
      toast(r.message, { action: { label: r.reveal_label, run: () => invoke("reveal_download", { path: r.path }) } });
    } catch (err) {
      toast(String(err), { error: true });
    }
  });

  // --- „Speicher & Modelle“ im Einstellungsdialog, vor „Server“ (Texte und Größe aus der Hülle)
  let storageRow;
  const gbText = (b) => `${(b / (1 << 30)).toLocaleString(document.documentElement.lang || undefined, { maximumFractionDigits: 1 })} GB`;
  async function refreshStorage() {
    if (!storageRow) return;
    try {
      const s = await invoke("storage_summary");
      storageRow.querySelector(".lbl").textContent = s.label;
      storageRow.querySelector("button").textContent = s.button;
      storageRow.querySelector(".small").textContent = gbText(s.used_bytes);
    } catch (e) {}
  }
  function addStorageRow() {
    const stop = document.getElementById("stopBtn");
    const field = stop && stop.closest(".field");
    if (!field || storageRow) return;
    storageRow = document.createElement("div");
    storageRow.className = "field";
    storageRow.innerHTML = '<span class="lbl"></span><div class="upd-row"><button type="button" class="sek"></button><span class="small"></span></div>';
    storageRow.querySelector("button").onclick = () => {
      document.getElementById("settingsDlg")?.close();
      invoke("open_storage");
    };
    field.before(storageRow);
    refreshStorage();
  }
  window.addEventListener("DOMContentLoaded", () => {
    addStorageRow();
    document.getElementById("btnSettings")?.addEventListener("click", refreshStorage);
  });
})();
