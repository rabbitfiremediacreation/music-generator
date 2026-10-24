// Start-, Einrichtungs- und Fehlerseite der Desktop-App. Zustand und Texte kommen aus der Hülle (src-tauri/src/lib.rs).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (s) => document.querySelector(s);
let texts = {};
let lang = "en";
let plan = null;

const t = (key, args = {}) => (texts[key] ?? key).replace(/\{(\w+)\}/g, (m, k) => args[k] ?? m);
const gb = (mb) => `${(mb / 1024).toLocaleString(lang, { maximumFractionDigits: 1 })} GB`;
const bytes = (b) => b < 1 << 20 ? `${Math.round(b / 1024).toLocaleString(lang)} KB` : b >= 1 << 30 ? `${(b / (1 << 30)).toLocaleString(lang, { maximumFractionDigits: 2 })} GB`
  : `${Math.round(b / (1 << 20)).toLocaleString(lang)} MB`;

function show(id) {
  for (const s of ["starting", "setup", "installing", "storage", "failed"]) $("#" + s).hidden = s !== id;
}

function render(status) {
  switch (status.phase) {
    case "starting":
      show("starting");
      $("#step").textContent = t(status.key);
      break;
    case "ready":
      show("starting");
      $("#step").textContent = "";
      break;
    case "setup":
      plan = status.plan;
      show("setup");
      renderSetup();
      break;
    case "storage":
      show("storage");
      renderStorage(status.info, status.freed);
      break;
    case "installing":
      show("installing");
      renderProgress(status.progress);
      break;
    case "failed":
      show("failed");
      $("#error").textContent = t(status.key, status.args);
      $("#logfile").textContent = status.log ? t("log_file", { path: status.log }) : "";
      $("#tail").textContent = status.tail;
      $("#tail").hidden = !status.tail;
      $("#retry").focus();
      break;
  }
}

function renderSetup() {
  const p = plan, hw = p.hardware;
  $("#home").textContent = p.home;
  $("#hw").textContent = [t("setup_hw_ram", { ram: hw.ram_gb }), hw.vram_gb ? t("setup_hw_vram", { vram: hw.vram_gb }) : "",
    hw.gpus.join(", ")].filter(Boolean).join(" · ");
  const box = $("#lmChoices");
  if (!box.dataset.built) {
    box.replaceChildren(...["0.6B", "1.7B", "4B"].map((size) => {
      const l = document.createElement("label");
      l.innerHTML = `<input type="radio" name="lm" value="${size}"> <b>${size}</b><span class="tag"></span><span class="mb"></span>`;
      return l;
    }));
    box.addEventListener("change", updateSizes);
    $("#xl").addEventListener("change", updateSizes);
    box.dataset.built = "1";
    box.querySelector(`input[value="${p.choice.lm}"]`).checked = true;
    $("#xl").checked = p.choice.xl;
  }
  for (const l of box.querySelectorAll("label")) {
    const size = l.querySelector("input").value;
    l.querySelector(".tag").textContent = size === p.recommended ? t("setup_recommended") : "";
    l.querySelector(".mb").textContent = p.sizes_mb[size] ? gb(p.sizes_mb[size]) : "";
  }
  $("#setupBack").hidden = !p.installed;
  $("#quit2").hidden = p.installed;
  $("#noEngine").hidden = p.engine_available || p.engine_present;
  $("#install").disabled = !(p.engine_available || p.engine_present);
  updateSizes();
}

function choice() {
  return { lm: $("#lmChoices input:checked")?.value || plan.recommended, xl: $("#xl").checked };
}

function updateSizes() {
  const c = choice();
  const lmName = `acestep-5Hz-lm-${c.lm}-Q8_0.gguf`;
  let mb = plan.base_mb;
  if (!plan.present.includes(lmName)) mb += plan.sizes_mb[c.lm] || 0;
  if (c.xl && !plan.present.some((n) => n.includes("xl-sft"))) mb += plan.sizes_mb.XL || 0;
  const sizes = ["0.6B", "1.7B", "4B"], hw = plan.hardware;
  const warnings = [];
  if (sizes.indexOf(c.lm) > sizes.indexOf(plan.recommended)) warnings.push(t("setup_warn_big"));
  if (c.xl && (hw.vram_gb ? hw.vram_gb < 12 : hw.ram_gb < 16)) warnings.push(t("setup_warn_xl"));
  $("#warn").textContent = warnings.join(" ");
  $("#warn").hidden = !warnings.length;
  $("#sizes").textContent = `${t("setup_download", { size: gb(mb) })} · ${t("setup_free", { size: `${plan.hardware.free_gb} GB` })}`;
}

// Zweistufiges Bestätigen wie in der Web-App: erster Klick „Wirklich?“, zweiter führt aus, sonst nach 4 s zurück
function confirmClick(btn, run) {
  if (btn.classList.contains("armed")) {
    btn.classList.remove("armed");
    btn.disabled = true;
    run();
    return;
  }
  const label = btn.textContent;
  btn.classList.add("armed");
  btn.textContent = t("btn_sure");
  setTimeout(() => { if (btn.classList.contains("armed")) { btn.classList.remove("armed"); btn.textContent = label; } }, 4000);
}

function row(cells) {
  const tr = document.createElement("tr");
  for (const c of cells) {
    const td = document.createElement("td");
    if (c.cls) td.className = c.cls;
    if (c.node) td.append(c.node); else td.textContent = c.text ?? "";
    tr.append(td);
  }
  return tr;
}

function renderStorage(info, freed) {
  $("#stHome").textContent = info.home;
  $("#stTotal").textContent = t("st_total", { size: bytes(info.total_bytes), free: `${info.free_gb} GB` });
  $("#stFreed").textContent = freed ? t("st_freed", { size: bytes(freed) }) : "";
  $("#stModels").replaceChildren(...info.models.map((m) => {
    const name = document.createElement("span");
    name.textContent = t("kind_" + m.kind, { size: m.size });
    const file = document.createElement("span");
    file.className = "file";
    file.textContent = m.name;
    const label = document.createElement("div");
    label.append(name, file);
    const state = document.createElement("span");
    state.className = m.active ? "on" : "muted";
    state.textContent = t(m.active ? "st_active" : "st_unused");
    let act = document.createTextNode("");
    if (m.removable) {
      act = document.createElement("button");
      act.className = "small danger";
      act.textContent = t("btn_delete");
      act.onclick = () => confirmClick(act, () => invoke("delete_model", { name: m.name }).catch((e) => { $("#stFreed").textContent = String(e); }));
    }
    return row([{ node: label }, { node: state }, { text: bytes(m.bytes), cls: "num" }, { node: act, cls: "act" }]);
  }));
  const clear = document.createElement("button");
  clear.className = "small";
  clear.textContent = t("btn_clear");
  clear.disabled = !info.cache_bytes;
  clear.onclick = () => { clear.disabled = true; invoke("clear_cache"); };
  $("#stOther").replaceChildren(
    row([{ text: t("st_engine") }, { text: "" }, { text: bytes(info.engine_bytes), cls: "num" }, { text: "", cls: "act" }]),
    row([{ text: t("st_venv") }, { text: "" }, { text: bytes(info.venv_bytes), cls: "num" }, { text: "", cls: "act" }]),
    row([{ text: t("st_cache") }, { text: "" }, { text: bytes(info.cache_bytes), cls: "num" }, { node: clear, cls: "act" }]),
    row([{ text: t("st_songs") }, { text: "" }, { text: bytes(info.songs_bytes), cls: "num" }, { text: "", cls: "act" }]),
  );
  for (const b of ["#stBack", "#stChange", "#stReset"]) $(b).disabled = false;
  $("#stReset").textContent = t("btn_reset");
}

function renderProgress(p) {
  $("#instStep").textContent = t(p.key, { index: p.index, count: p.count, file: p.file });
  const bar = $(".bar"), pct = p.total ? Math.min(100, (p.done / p.total) * 100) : 0;
  bar.classList.toggle("busy", !p.total);
  bar.setAttribute("aria-valuenow", String(Math.round(pct)));
  $("#barFill").style.width = p.total ? `${pct}%` : "";
  $("#instDetail").textContent = p.total
    ? `${bytes(p.done)} / ${bytes(p.total)}${p.bytes_per_sec ? ` · ${bytes(p.bytes_per_sec)}/s` : ""}`
    : "";
}

function applyTexts() {
  document.documentElement.lang = lang;
  document.querySelectorAll("[data-t]").forEach((e) => { e.textContent = t(e.dataset.t); });
}

let last = { phase: "starting", key: "step_check" };
async function load() {
  const s = await invoke("splash_state");
  texts = s.texts;
  lang = s.lang;
  applyTexts();
  render(last = s.status);
}

listen("status", (e) => render(last = e.payload));
listen("lang", load);
$("#retry").onclick = () => invoke("retry");
$("#logs").onclick = () => invoke("open_logs");
$("#folder").onclick = () => invoke("open_folder");
$("#quit").onclick = $("#quit2").onclick = () => invoke("quit");
$("#stBack").onclick = $("#setupBack").onclick = () => invoke("close_storage");
$("#stChange").onclick = () => invoke("change_models");
$("#stReset").onclick = () => confirmClick($("#stReset"), () => invoke("reset_all"));
$("#changeHome").onclick = () => invoke("choose_home");
$("#install").onclick = () => { $("#install").disabled = true; invoke("install", { choice: choice() }); };
$("#cancel").onclick = () => { $("#cancel").disabled = true; invoke("cancel_install"); };
listen("status", (e) => { if (e.payload.phase !== "installing") $("#cancel").disabled = false; });
load();
