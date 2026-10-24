//! Desktop-Hülle für den Music Generator: startet die Server, zeigt die Web-App im eigenen Fenster und beendet
//! beim Schließen alles wieder. Beim ersten Start richtet sie Modellserver und Modelle ein (`installer.rs`).

mod config;
mod downloads;
mod i18n;
mod installer;
mod paths;
mod settings;
mod storage;
mod supervisor;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, Url, WebviewUrl, WebviewWindowBuilder, Wry,
};
use tauri_plugin_opener::OpenerExt;

use paths::Layout;
use supervisor::{Event, Failure, Role, Supervisor};

const MAIN: &str = "main";
const TRAY: &str = "main";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum Status {
    Starting {
        key: String,
    },
    Failed {
        key: String,
        args: BTreeMap<String, String>,
        log: Option<String>,
        tail: String,
    },
    /// Noch nicht eingerichtet: Einrichtungsdialog mit Vorschlag
    Setup {
        plan: installer::Plan,
    },
    /// Einrichtung läuft
    Installing {
        progress: installer::Progress,
    },
    /// Speicher & Modelle verwalten
    Storage {
        info: storage::Info,
        /// zuletzt freigegebener Platz (Bytes) für die Meldung
        freed: u64,
    },
    Ready,
}

enum Cmd {
    Start,
    Install(installer::Choice),
}

struct Shared {
    sup: Mutex<Option<Arc<Supervisor>>>,
    status: Mutex<Status>,
    lang: Mutex<String>,
    tx: Mutex<Sender<Cmd>>,
    splash_url: Mutex<Option<Url>>,
    /// Programmcode (mitgelieferte Ressourcen bzw. Repository)
    code: Option<PathBuf>,
    layout: Mutex<Option<Layout>>,
    settings: Mutex<settings::Settings>,
    settings_path: Option<PathBuf>,
    web_port: u16,
    cancel_install: AtomicBool,
}

impl Shared {
    fn lang(&self) -> String {
        self.lang.lock().unwrap().clone()
    }
    fn send(&self, cmd: Cmd) {
        let _ = self.tx.lock().unwrap().send(cmd);
    }
    fn save_settings(&self, change: impl FnOnce(&mut settings::Settings)) {
        let mut s = self.settings.lock().unwrap();
        change(&mut s);
        if let Some(p) = &self.settings_path {
            settings::save(p, &s);
        }
    }
}

// ------------------------------------------------------------------ Befehle der Startseite und der Web-App

#[derive(Serialize)]
struct SplashState {
    status: Status,
    lang: String,
    texts: i18n::Texts,
}

#[tauri::command]
fn splash_state(state: State<'_, Shared>) -> SplashState {
    let lang = state.lang();
    SplashState {
        status: state.status.lock().unwrap().clone(),
        texts: i18n::texts(&lang),
        lang,
    }
}

#[tauri::command]
fn retry(state: State<'_, Shared>) {
    state.send(Cmd::Start);
}

#[tauri::command]
fn open_logs(app: AppHandle) {
    open_dir(&app, paths::data_dir);
}

#[tauri::command]
fn open_folder(app: AppHandle) {
    open_dir(&app, |home| home.to_path_buf());
}

/// Einrichtung mit der gewählten Sprachmodellgröße (und optional XL) starten.
#[tauri::command]
fn install(state: State<'_, Shared>, choice: installer::Choice) {
    state.send(Cmd::Install(choice));
}

#[tauri::command]
fn cancel_install(state: State<'_, Shared>) {
    state.cancel_install.store(true, Ordering::SeqCst);
}

/// Anderen Datenordner wählen (Engine, Modelle, Songs). Ein Ordner mit vorhandener Installation wird direkt genutzt.
#[tauri::command]
async fn choose_home(app: AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let st = app.state::<Shared>();
    let current = st.layout.lock().unwrap().as_ref().map(|l| l.home.clone());
    let mut dialog = app.dialog().file();
    if let Some(dir) = current.as_ref().and_then(|h| h.parent()) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return;
    };
    // leerer oder neuer Ordner → Unterordner „MusicGenerator“, damit nichts im gewählten Ordner verstreut wird
    let home = if paths::is_installed(&picked) || picked.ends_with("MusicGenerator") {
        picked
    } else {
        picked.join("MusicGenerator")
    };
    st.save_settings(|s| s.home = Some(home));
    st.send(Cmd::Start);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Die Web-App meldet ihre Oberflächensprache (Init-Skript, Ereignis `langchange`); Tray und Startseite folgen.
#[tauri::command]
fn set_lang(app: AppHandle, state: State<'_, Shared>, lang: String) {
    if !i18n::is_supported(&lang) || state.lang() == lang {
        return;
    }
    *state.lang.lock().unwrap() = lang.clone();
    state.save_settings(|s| s.lang = Some(lang.clone()));
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), tray_menu(&app, &lang)) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = app.emit("lang", &lang);
}

#[derive(Serialize)]
struct Saved {
    path: String,
    message: String,
    reveal_label: String,
}

/// Download aus der Web-App speichern (siehe `webapp.js`). Nur die eigene Web-App und `data:`-Adressen.
#[tauri::command]
async fn save_download(app: AppHandle, url: String, name: Option<String>) -> Result<Saved, String> {
    let st = app.state::<Shared>();
    let (port, lang) = (st.web_port, st.lang());
    let fail = |detail: String| i18n::t(&lang, "download_failed").replace("{detail}", &detail);
    let parsed = Url::parse(&url).map_err(|e| fail(e.to_string()))?;
    let dir = downloads::download_dir();
    let result = tauri::async_runtime::spawn_blocking(move || {
        if parsed.scheme() == "data" {
            return downloads::save_data_url(
                parsed.as_str(),
                &dir,
                name.as_deref().unwrap_or("download.txt"),
            );
        }
        let ours = parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("localhost") | Some("127.0.0.1"))
            && parsed.port() == Some(port);
        if !ours {
            return Err(format!("{parsed}"));
        }
        let mut path_query = parsed.path().to_string();
        if let Some(q) = parsed.query() {
            path_query = format!("{path_query}?{q}");
        }
        let fallback = name.unwrap_or_else(|| {
            parsed
                .path_segments()
                .and_then(|mut s| s.next_back())
                .unwrap_or("download")
                .to_string()
        });
        downloads::fetch(port, &path_query, &dir, &fallback)
    })
    .await
    .map_err(|e| fail(e.to_string()))?
    .map_err(fail)?;
    let file = result
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Saved {
        path: result.to_string_lossy().into_owned(),
        message: i18n::t(&lang, "download_saved").replace("{name}", &file),
        reveal_label: i18n::t(&lang, "download_reveal"),
    })
}

/// Gespeicherten Download im Dateimanager zeigen – nur Dateien im Downloads-Ordner.
#[tauri::command]
fn reveal_download(app: AppHandle, path: String) {
    let p = PathBuf::from(path);
    let inside = match (p.canonicalize(), downloads::download_dir().canonicalize()) {
        (Ok(p), Ok(d)) => p.starts_with(d),
        _ => false,
    };
    if inside {
        let _ = app.opener().reveal_item_in_dir(p);
    }
}

// ------------------------------------------------------------------ Speicher & Modelle

fn current_layout(app: &AppHandle) -> Option<Layout> {
    app.state::<Shared>().layout.lock().unwrap().clone()
}

/// Server beenden (vor dem Löschen von Modellen: Windows sperrt geöffnete Dateien, der Server hält sie offen).
fn stop_servers(app: &AppHandle) {
    let sup = app.state::<Shared>().sup.lock().unwrap().take();
    if let Some(sup) = sup {
        sup.stop();
    }
}

fn show_storage(app: &AppHandle, freed: u64) {
    let Some(layout) = current_layout(app) else {
        return;
    };
    let manifest = installer::load_manifest(&layout.code).unwrap_or_default();
    set_status(
        app,
        Status::Storage {
            info: storage::info(&layout.home, &manifest),
            freed,
        },
    );
    show_splash(app);
}

#[tauri::command]
async fn open_storage(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || show_storage(&app, 0)).await;
}

/// Zurück zur Web-App; laufen die Server nicht mehr (nach dem Löschen), neu starten.
#[tauri::command]
fn close_storage(app: AppHandle, state: State<'_, Shared>) {
    let sup = state.sup.lock().unwrap().clone();
    match sup {
        Some(sup) if supervisor::is_web_app(state.web_port) => {
            set_status(&app, Status::Ready);
            if let (Some(w), Ok(url)) = (app.get_webview_window(MAIN), Url::parse(&sup.url())) {
                let _ = w.navigate(url);
            }
        }
        _ => state.send(Cmd::Start),
    }
}

#[derive(Serialize)]
struct StorageSummary {
    label: String,
    button: String,
    used_bytes: u64,
}

/// Für den Eintrag im Einstellungsdialog der Web-App (`webapp.js`).
#[tauri::command]
async fn storage_summary(app: AppHandle) -> StorageSummary {
    let lang = app.state::<Shared>().lang();
    let home = current_layout(&app).map(|l| l.home);
    let used_bytes = tauri::async_runtime::spawn_blocking(move || {
        home.map(|h| storage::used_bytes(&h)).unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    StorageSummary {
        label: i18n::t(&lang, "storage_title"),
        button: i18n::t(&lang, "storage_manage"),
        used_bytes,
    }
}

#[tauri::command]
async fn delete_model(app: AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let layout = current_layout(&app).ok_or("kein Datenordner")?;
        let manifest = installer::load_manifest(&layout.code)?;
        stop_servers(&app);
        let freed = storage::delete_model(&layout.home, &manifest, &name)?;
        show_storage(&app, freed);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clear_cache(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if let Some(layout) = current_layout(&app) {
            let freed = storage::clear_cache(&layout.home);
            show_storage(&app, freed);
        }
    })
    .await;
}

/// Alles außer den Songs löschen und neu einrichten.
#[tauri::command]
async fn reset_all(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let layout = current_layout(&app).ok_or("kein Datenordner")?;
        stop_servers(&app);
        storage::reset(&layout.home)?;
        app.state::<Shared>().send(Cmd::Start);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Einrichtungsdialog für eine vorhandene Installation: anderes Sprachmodell oder XL nachladen.
#[tauri::command]
async fn change_models(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if let Some(layout) = current_layout(&app) {
            if let Ok(m) = installer::load_manifest(&layout.code) {
                set_status(
                    &app,
                    Status::Setup {
                        plan: installer::plan(&layout, &m),
                    },
                );
                show_splash(&app);
            }
        }
    })
    .await;
}

// ------------------------------------------------------------------ Hilfen

fn open_dir(app: &AppHandle, which: impl Fn(&std::path::Path) -> PathBuf) {
    let layout = app.state::<Shared>().layout.lock().unwrap().clone();
    if let Some(layout) = layout {
        let dir = which(&layout.home);
        let _ = std::fs::create_dir_all(&dir);
        let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
    }
}

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Shared>().status.lock().unwrap() = status.clone();
    let _ = app.emit("status", status);
}

fn fail(app: &AppHandle, f: Failure) {
    let tail = f
        .log
        .as_deref()
        .map(|p| supervisor::tail(p, 30))
        .unwrap_or_default();
    set_status(
        app,
        Status::Failed {
            key: f.key.to_string(),
            args: f
                .args
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            log: f.log.map(|p| p.to_string_lossy().into_owned()),
            tail,
        },
    );
    show_splash(app);
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn show_splash(app: &AppHandle) {
    let splash = app.state::<Shared>().splash_url.lock().unwrap().clone();
    if let (Some(w), Some(url)) = (app.get_webview_window(MAIN), splash) {
        if w.url().map(|u| u != url).unwrap_or(true) {
            let _ = w.navigate(url);
        }
    }
    show_main(app);
}

fn tray_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str| {
        MenuItem::with_id(
            app,
            id,
            i18n::t(lang, &format!("tray_{id}")),
            true,
            None::<&str>,
        )
    };
    Menu::with_items(
        app,
        &[
            &item("open")?,
            &item("restart")?,
            &PredefinedMenuItem::separator(app)?,
            &item("storage")?,
            &item("logs")?,
            &item("folder")?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit")?,
        ],
    )
}

/// Nur die eigene Web-App und die Startseite bleiben im Fenster, alles andere öffnet der Systembrowser.
/// `dev_url`: Server, über den `tauri dev` die Startseite ausliefert (im fertigen Paket `None`).
fn navigation_allowed(url: &Url, web_port: u16, dev_url: Option<&Url>) -> bool {
    if dev_url.is_some_and(|d| d.origin() == url.origin()) {
        return true;
    }
    match url.scheme() {
        "tauri" | "about" | "data" | "blob" | "asset" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            Some("localhost") | Some("127.0.0.1") => url.port_or_known_default() == Some(web_port),
            _ => false,
        },
        _ => false,
    }
}

/// Linux: WebKitGTK hat Mikrofon-Zugriff ab Werk ausgeschaltet, fragt nicht selbst nach und lehnt sonst ab. Für „Idee aufnehmen“ wird es nur der
/// eigenen Web-App erlaubt (macOS und Windows fragen über das System bzw. WebView2).
fn allow_microphone(win: &tauri::WebviewWindow, web_port: u16) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    win.with_webview(move |wv| {
        use webkit2gtk::glib::ObjectExt;
        use webkit2gtk::{
            PermissionRequestExt, SettingsExt, UserMediaPermissionRequest, WebViewExt,
        };
        // WebKitGTK hat getUserMedia ab Werk aus (wry schaltet es nicht ein): ohne das kein navigator.mediaDevices
        if let Some(settings) = wv.inner().settings() {
            settings.set_enable_media_stream(true);
        }
        let ours = [
            format!("http://localhost:{web_port}/"),
            format!("http://127.0.0.1:{web_port}/"),
        ];
        wv.inner().connect_permission_request(move |view, req| {
            let from_web_app = view
                .uri()
                .is_some_and(|u| ours.iter().any(|o| u.starts_with(o.as_str())));
            if from_web_app && req.is::<UserMediaPermissionRequest>() {
                req.allow();
            } else {
                req.deny();
            }
            true
        });
    })?;
    #[cfg(not(target_os = "linux"))]
    let _ = (win, web_port);
    Ok(())
}

/// Skript für die Web-App im Hauptfenster: Sprache melden, Downloads an die Hülle leiten (`webapp.js`), unter Linux
/// Aufnahme als WAV (`wav-recorder.js`).
fn init_script(web_port: u16) -> String {
    let mut js = include_str!("webapp.js").to_string();
    if cfg!(target_os = "linux") {
        js.push_str(include_str!("wav-recorder.js")); // MediaRecorder von WebKitGTK liefert leere Aufnahmen
    }
    js.replace("__PORT__", &web_port.to_string())
}

// ------------------------------------------------------------------ Steuerung (eigener Thread)

fn controller(app: AppHandle, rx: Receiver<Cmd>) {
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Cmd::Start) => start(&app),
            Ok(Cmd::Install(choice)) => install_now(&app, &choice),
            Err(RecvTimeoutError::Timeout) => watch(&app),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn start(app: &AppHandle) {
    let st = app.state::<Shared>();
    let old = st.sup.lock().unwrap().take();
    if let Some(old) = old {
        old.stop();
    }
    show_splash(app);
    set_status(
        app,
        Status::Starting {
            key: "step_check".into(),
        },
    );

    let Some(code) = st.code.clone() else {
        fail(app, Failure::new("err_no_project").arg("path", "project"));
        return;
    };
    let saved_home = st.settings.lock().unwrap().home.clone();
    let layout = Layout {
        code: code.clone(),
        home: paths::home_dir(saved_home.as_deref()),
    };
    *st.layout.lock().unwrap() = Some(layout.clone());

    if !paths::is_installed(&layout.home) {
        match installer::load_manifest(&code) {
            Ok(m) => set_status(
                app,
                Status::Setup {
                    plan: installer::plan(&layout, &m),
                },
            ),
            Err(e) => fail(app, Failure::new("err_spawn").arg("detail", e)),
        }
        return;
    }
    let sup = Arc::new(Supervisor::new(layout));
    *st.sup.lock().unwrap() = Some(sup.clone());

    match sup.start(&|step| {
        set_status(
            app,
            Status::Starting {
                key: step.key().into(),
            },
        )
    }) {
        Ok(()) => {
            set_status(app, Status::Ready);
            if let (Some(w), Ok(url)) = (app.get_webview_window(MAIN), Url::parse(&sup.url())) {
                let _ = w.navigate(url);
            }
        }
        Err(f) if f.key == "err_cancelled" => {}
        Err(f) => {
            sup.stop();
            *st.sup.lock().unwrap() = None;
            fail(app, f);
        }
    }
}

fn install_now(app: &AppHandle, choice: &installer::Choice) {
    let st = app.state::<Shared>();
    let Some(layout) = st.layout.lock().unwrap().clone() else {
        return;
    };
    let manifest = match installer::load_manifest(&layout.code) {
        Ok(m) => m,
        Err(e) => return fail(app, Failure::new("err_spawn").arg("detail", e)),
    };
    // laufende Server belegen Grafikspeicher, den der Grafiktest braucht
    stop_servers(app);
    st.cancel_install.store(false, Ordering::SeqCst);
    let report = |progress: installer::Progress| set_status(app, Status::Installing { progress });
    let inst = installer::Installer {
        layout: &layout,
        manifest: &manifest,
        cancel: &st.cancel_install,
        report: &report,
    };
    match inst.run(choice) {
        // fertig → normal starten; abgebrochen → zurück zum Einrichtungsdialog (Downloads setzen später fort)
        Ok(()) => start(app),
        Err(e) if e.key == "setup_err_cancelled" => start(app),
        Err(e) => fail(
            app,
            Failure::new(e.key)
                .arg("detail", e.detail)
                .log(paths::data_dir(&layout.home).join("setup.log")),
        ),
    }
}

fn watch(app: &AppHandle) {
    let st = app.state::<Shared>();
    if !matches!(*st.status.lock().unwrap(), Status::Ready) {
        return;
    }
    let Some(sup) = st.sup.lock().unwrap().clone() else {
        return;
    };
    let log = |r: Role| sup.data_dir().join(r.log_name());
    let failure = match sup.poll() {
        None => return,
        Some(Event::WebExited(Some(a))) if a == "quit" => {
            app.exit(0);
            return;
        }
        Some(Event::WebExited(Some(a))) if a == "restart" => match sup.restart_web() {
            Ok(()) => return,
            Err(f) => f,
        },
        Some(Event::WebExited(_)) => Failure::new("err_web_crashed").log(log(Role::Web)),
        Some(Event::EngineExited(r)) => Failure::new("err_engine_crashed").log(log(r)),
    };
    sup.stop();
    *st.sup.lock().unwrap() = None;
    fail(app, failure);
}

// ------------------------------------------------------------------ Einstieg

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            splash_state,
            retry,
            open_logs,
            open_folder,
            quit,
            set_lang,
            save_download,
            reveal_download,
            install,
            cancel_install,
            choose_home,
            open_storage,
            close_storage,
            storage_summary,
            delete_model,
            clear_cache,
            reset_all,
            change_models
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let settings_path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("settings.json"));
            let saved = settings_path
                .as_deref()
                .map(settings::load)
                .unwrap_or_default();
            let lang = i18n::detect(saved.lang.as_deref());
            let code = paths::code_dir(app.path().resource_dir().ok().as_deref());
            let web_port = code
                .as_deref()
                .map(config::load_ports)
                .unwrap_or_default()
                .web;

            let (tx, rx) = mpsc::channel();
            app.manage(Shared {
                sup: Mutex::new(None),
                status: Mutex::new(Status::Starting {
                    key: "step_check".into(),
                }),
                lang: Mutex::new(lang.clone()),
                tx: Mutex::new(tx),
                splash_url: Mutex::new(None),
                code,
                layout: Mutex::new(None),
                settings: Mutex::new(saved),
                settings_path,
                web_port,
                cancel_install: AtomicBool::new(false),
            });

            let nav_app = handle.clone();
            let dev_url = app.config().build.dev_url.clone();
            let win = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
                .title("Music Generator")
                .inner_size(1280.0, 860.0)
                .min_inner_size(720.0, 540.0)
                .initialization_script(init_script(web_port))
                .on_navigation(move |url| {
                    if navigation_allowed(url, web_port, dev_url.as_ref()) {
                        return true;
                    }
                    let _ = nav_app.opener().open_url(url.as_str(), None::<&str>);
                    false
                })
                .build()?;
            allow_microphone(&win, web_port)?;
            *app.state::<Shared>().splash_url.lock().unwrap() = win.url().ok();

            TrayIconBuilder::with_id(TRAY)
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .expect("Fenster-Icon fehlt"),
                )
                .tooltip("Music Generator")
                .menu(&tray_menu(&handle, &lang)?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_main(app),
                    "restart" => app.state::<Shared>().send(Cmd::Start),
                    "storage" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn_blocking(move || show_storage(&app, 0));
                    }
                    "logs" => open_dir(app, paths::data_dir),
                    "folder" => open_dir(app, |home| home.to_path_buf()),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            // SIGTERM/SIGINT/SIGHUP (Abmelden, kill, Strg+C) wie „Beenden“ behandeln, damit die Server mit enden
            #[cfg(unix)]
            {
                use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
                let mut signals = signal_hook::iterator::Signals::new([SIGTERM, SIGINT, SIGHUP])?;
                let h = handle.clone();
                std::thread::spawn(move || {
                    if signals.forever().next().is_some() {
                        h.exit(0);
                    }
                });
            }

            std::thread::spawn(move || controller(handle, rx));
            app.state::<Shared>().send(Cmd::Start);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri-App konnte nicht erstellt werden");

    app.run(|app, ev| {
        if let RunEvent::Exit = ev {
            let sup = app.state::<Shared>().sup.lock().unwrap().take();
            if let Some(sup) = sup {
                sup.stop();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_rules() {
        let ok = |s: &str| navigation_allowed(&Url::parse(s).unwrap(), 8765, None);
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/index.html"));
        assert!(ok("http://localhost:8765/"));
        assert!(ok("http://127.0.0.1:8765/api/songs/1/audio?download=1"));
        assert!(!ok("http://localhost:8085/"));
        assert!(!ok(
            "https://github.com/rabbitfiremediacreation/music-generator"
        ));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("http://127.0.0.1:1430/"));
        let dev = Url::parse("http://127.0.0.1:1430").unwrap();
        assert!(navigation_allowed(
            &Url::parse("http://127.0.0.1:1430/").unwrap(),
            8765,
            Some(&dev)
        ));
    }
}
