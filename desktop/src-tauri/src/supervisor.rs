//! Startet und überwacht Modellserver (ace-server) und Web-App (uvicorn) – die Logik der ON/OFF-Skripte, aber mit
//! eigenen Kindprozessen statt Hintergrundprozessen ohne Besitzer. Unabhängig von Tauri, damit es testbar bleibt.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::{self, EngineConf, Mode, Ports};
use crate::paths::{self, Layout};

/// Datei, in der die Web-App im Desktop-Modus hinterlässt, was nach ihrem Ende passieren soll (`app/desktop.py`).
pub const EXIT_FILE: &str = "desktop-exit";

/// Wie lange auf die Server gewartet wird: zwei Minuten wie die ON-Skripte (der erste Start unter Windows dauert lange).
const START_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Lm,
    Synth,
    Web,
}

impl Role {
    pub fn log_name(self) -> &'static str {
        match self {
            Role::Lm => "ace-lm.log",
            Role::Synth => "ace-server.log",
            Role::Web => "server.log",
        }
    }
}

/// Schritt beim Start, für die Anzeige (Schlüssel in `locales/*.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Check,
    Lm,
    Synth,
    Web,
    Wait,
}

impl Step {
    pub fn key(self) -> &'static str {
        match self {
            Step::Check => "step_check",
            Step::Lm => "step_lm",
            Step::Synth => "step_synth",
            Step::Web => "step_web",
            Step::Wait => "step_wait",
        }
    }
}

/// Fehler mit Übersetzungsschlüssel, Platzhaltern und ggf. der passenden Logdatei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub key: &'static str,
    pub args: BTreeMap<&'static str, String>,
    pub log: Option<PathBuf>,
}

impl Failure {
    pub fn new(key: &'static str) -> Self {
        Failure {
            key,
            args: BTreeMap::new(),
            log: None,
        }
    }
    pub fn arg(mut self, k: &'static str, v: impl Into<String>) -> Self {
        self.args.insert(k, v.into());
        self
    }
    pub fn log(mut self, p: PathBuf) -> Self {
        self.log = Some(p);
        self
    }
}

/// Was beim Überwachen auffällt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Web-App hat sich beendet. `Some("quit")` / `Some("restart")` = gewollt (Desktop-Modus), `None` = Absturz.
    WebExited(Option<String>),
    /// Ein Modellserver ist abgestürzt.
    EngineExited(Role),
}

struct Proc {
    role: Role,
    child: Child,
}

pub struct Supervisor {
    layout: Layout,
    ports: Ports,
    procs: Mutex<Vec<Proc>>,
    cancel: AtomicBool,
}

impl Supervisor {
    pub fn new(layout: Layout) -> Self {
        let ports = config::load_ports(&layout.code);
        Supervisor {
            layout,
            ports,
            procs: Mutex::new(Vec::new()),
            cancel: AtomicBool::new(false),
        }
    }

    pub fn url(&self) -> String {
        // „localhost“ statt 127.0.0.1: gilt in allen Webviews als sichere Herkunft (Mikrofon)
        format!("http://localhost:{}/", self.ports.web)
    }

    pub fn data_dir(&self) -> PathBuf {
        paths::data_dir(&self.layout.home)
    }

    fn log_path(&self, role: Role) -> PathBuf {
        self.data_dir().join(role.log_name())
    }

    /// Alles starten, was noch nicht läuft, und warten, bis es antwortet. Blockiert; `stop()` bricht ab.
    pub fn start(&self, report: &dyn Fn(Step)) -> Result<(), Failure> {
        self.cancel.store(false, Ordering::SeqCst);
        report(Step::Check);
        let home = &self.layout.home;
        let ace = paths::ace_server(home)
            .ok_or_else(|| Failure::new("err_not_installed").arg("missing", "ace-server"))?;
        let missing = paths::missing_models(home);
        if !missing.is_empty() {
            return Err(Failure::new("err_not_installed").arg("missing", missing.join(", ")));
        }
        let uv = paths::uv().ok_or_else(|| Failure::new("err_no_uv"))?;
        std::fs::create_dir_all(self.data_dir())
            .map_err(|e| Failure::new("err_spawn").arg("detail", e.to_string()))?;
        let conf = config::load_engine_conf(home);

        // Belegte Ports: Läuft dort schon unser Server (z. B. per ON-Skript gestartet), wird er übernommen.
        let synth_up = port_open(self.ports.synth);
        let lm_up = conf.mode == Mode::Split && port_open(self.ports.lm);
        let web_up = port_open(self.ports.web);
        if web_up && !is_web_app(self.ports.web) {
            return Err(Failure::new("err_port_foreign").arg("port", self.ports.web.to_string()));
        }
        if !synth_up && !lm_up {
            repair_mac_rpaths(&self.layout);
        }

        let base = server_args(&paths::models_dir(home), &conf);
        if conf.mode == Mode::Split && !lm_up {
            report(Step::Lm);
            let mut args = vec![
                OsString::from("--host"),
                "127.0.0.1".into(),
                "--port".into(),
                self.ports.lm.to_string().into(),
            ];
            args.extend(base.iter().cloned());
            // Neuere Programmdateien wählen das Gerät mit --device und ignorieren GGML_BACKEND (wie ON.ps1)
            if help_mentions(&ace, "--device") {
                args.extend(["--device".into(), "CPU".into()]);
            }
            self.spawn(Role::Lm, &ace, &args, &[("GGML_BACKEND", "CPU".into())])?;
        }
        if !synth_up {
            report(Step::Synth);
            let mut args = vec![
                OsString::from("--host"),
                "127.0.0.1".into(),
                "--port".into(),
                self.ports.synth.to_string().into(),
            ];
            args.extend(base.iter().cloned());
            self.spawn(Role::Synth, &ace, &args, &[])?;
        }
        if !web_up {
            report(Step::Web);
            self.spawn_web(&uv)?;
        }

        report(Step::Wait);
        let deadline = Instant::now() + START_TIMEOUT;
        let mut waiting: Vec<(Role, u16)> =
            vec![(Role::Synth, self.ports.synth), (Role::Web, self.ports.web)];
        if conf.mode == Mode::Split {
            waiting.insert(0, (Role::Lm, self.ports.lm));
        }
        while !waiting.is_empty() {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Failure::new("err_cancelled"));
            }
            if let Some(role) = self.exited_role() {
                return Err(Failure::new(timeout_key(role)).log(self.log_path(role)));
            }
            waiting.retain(|(role, port)| match role {
                Role::Web => !is_web_app(*port),
                _ => !port_open(*port),
            });
            if waiting.is_empty() {
                break;
            }
            if Instant::now() > deadline {
                let role = waiting[0].0;
                return Err(Failure::new(timeout_key(role)).log(self.log_path(role)));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }

    fn spawn_web(&self, uv: &Path) -> Result<(), Failure> {
        let _ = std::fs::remove_file(self.data_dir().join(EXIT_FILE));
        let mut args = uv_run_args(&self.layout);
        args.extend(
            ["uvicorn", "app.main:app", "--host", "127.0.0.1", "--port"]
                .iter()
                .map(OsString::from),
        );
        args.push(self.ports.web.to_string().into());
        let mut env = python_env(&self.layout);
        env.push(("MG_DESKTOP", "1".into()));
        self.spawn(Role::Web, uv, &args, &env)
    }

    fn spawn(
        &self,
        role: Role,
        program: &Path,
        args: &[OsString],
        env: &[(&str, OsString)],
    ) -> Result<(), Failure> {
        let log = self.log_path(role);
        let fail = |e: std::io::Error| {
            Failure::new("err_spawn")
                .arg("detail", format!("{}: {e}", program.display()))
                .log(log.clone())
        };
        let out = File::create(&log).map_err(fail)?;
        let err = out.try_clone().map_err(fail)?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&self.layout.code)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        paths::prepare(&mut cmd);
        for (k, v) in engine_env() {
            cmd.env(k, v);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        hide_console(&mut cmd);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // Endet der startende Thread (der Steuer-Thread lebt so lange wie die App), bekommt das Kind SIGTERM –
            // auch wenn die App abstürzt oder hart beendet wird. uv reicht das Signal an Python weiter.
            // SAFETY: prctl ist zwischen fork und exec erlaubt (async-signal-safe).
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
        }
        let child = cmd.spawn().map_err(fail)?;
        self.procs.lock().unwrap().push(Proc { role, child });
        Ok(())
    }

    /// Erster eigener Prozess, der sich beendet hat (ohne ihn aus der Liste zu nehmen).
    fn exited_role(&self) -> Option<Role> {
        let mut procs = self.procs.lock().unwrap();
        procs
            .iter_mut()
            .find_map(|p| matches!(p.child.try_wait(), Ok(Some(_))).then_some(p.role))
    }

    /// Prüft die eigenen Prozesse. Beendete werden aus der Liste genommen.
    pub fn poll(&self) -> Option<Event> {
        let mut procs = self.procs.lock().unwrap();
        let idx = procs
            .iter_mut()
            .position(|p| matches!(p.child.try_wait(), Ok(Some(_))))?;
        let role = procs.remove(idx).role;
        drop(procs);
        Some(match role {
            Role::Web => {
                let f = self.data_dir().join(EXIT_FILE);
                let action = std::fs::read_to_string(&f)
                    .ok()
                    .map(|s| s.trim().to_string());
                let _ = std::fs::remove_file(f);
                Event::WebExited(action)
            }
            r => Event::EngineExited(r),
        })
    }

    /// Nur die Web-App neu starten (nach einem Update aus der App heraus).
    pub fn restart_web(&self) -> Result<(), Failure> {
        let uv = paths::uv().ok_or_else(|| Failure::new("err_no_uv"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while port_open(self.ports.web) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        self.spawn_web(&uv)?;
        let deadline = Instant::now() + START_TIMEOUT;
        while !is_web_app(self.ports.web) {
            if Instant::now() > deadline || self.exited_role() == Some(Role::Web) {
                return Err(Failure::new("err_timeout_web").log(self.log_path(Role::Web)));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }

    /// Eigene Prozesse beenden (Web-App zuerst). Übernommene, fremd gestartete Server bleiben unberührt.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let mut procs = std::mem::take(&mut *self.procs.lock().unwrap());
        procs.sort_by_key(|p| std::cmp::Reverse(p.role));
        for p in procs.iter_mut() {
            terminate(&mut p.child);
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn timeout_key(role: Role) -> &'static str {
    match role {
        Role::Lm => "err_timeout_lm",
        Role::Synth => "err_timeout_synth",
        Role::Web => "err_timeout_web",
    }
}

/// Gemeinsame Server-Optionen wie in den ON-Skripten.
pub fn server_args(models: &Path, conf: &EngineConf) -> Vec<OsString> {
    let mut v: Vec<OsString> = vec![
        "--models".into(),
        models.as_os_str().to_owned(),
        "--max-batch".into(),
        "1".into(),
    ];
    if let Some(chunk) = &conf.vae_chunk {
        v.push("--vae-chunk".into());
        v.push(chunk.into());
    }
    v
}

/// Umgebung für ace-server wie in `common.sh` / `arch_linux.sh`.
fn engine_env() -> Vec<(&'static str, OsString)> {
    let mut v = Vec::new();
    if cfg!(target_os = "linux") {
        // Kein GGML_VK_VISIBLE_DEVICES=0 wie in arch_linux.sh: Bei Intel + NVIDIA ist Gerät 0 die Onboard-Grafik.
        // acestep.cpp wählt ohne Vorgabe selbst die dedizierte Karte; eine selbst gesetzte Variable wird vererbt.
        if Path::new("/opt/rocm/lib").is_dir() {
            let mut p = OsString::from("/opt/rocm/lib");
            if let Some(old) = std::env::var_os("LD_LIBRARY_PATH") {
                p.push(":");
                p.push(old);
            }
            v.push(("LD_LIBRARY_PATH", p));
        }
    }
    v
}

fn help_mentions(ace: &Path, flag: &str) -> bool {
    let mut cmd = Command::new(ace);
    cmd.arg("--help").stdin(Stdio::null());
    paths::prepare(&mut cmd);
    hide_console(&mut cmd);
    cmd.output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).contains(flag)
                || String::from_utf8_lossy(&o.stderr).contains(flag)
        })
        .unwrap_or(false)
}

/// `uv run` im Programmordner. Installierte App: Lockdatei nicht anfassen (Programmordner ist schreibgeschützt).
pub fn uv_run_args(layout: &Layout) -> Vec<OsString> {
    let mut v = vec![OsString::from("run")];
    if layout.bundled() {
        v.push("--frozen".into());
    }
    v
}

/// Umgebung für Python: Datenordner und Python-Umgebung liegen bei getrennten Ordnern im Datenordner.
pub fn python_env(layout: &Layout) -> Vec<(&'static str, OsString)> {
    if !layout.bundled() {
        return Vec::new();
    }
    vec![
        ("MG_HOME", layout.home.clone().into_os_string()),
        (
            "UV_PROJECT_ENVIRONMENT",
            layout.home.join(".venv").into_os_string(),
        ),
        ("PYTHONDONTWRITEBYTECODE", "1".into()),
    ]
}

/// Mac: verschobener Datenordner → rpath der Engine reparieren (`pfade_reparieren` aus `common.sh`).
fn repair_mac_rpaths(layout: &Layout) {
    if cfg!(target_os = "macos") {
        let _ = Command::new("/bin/bash")
            .args([
                "-c",
                "source scripts/common.sh && ENGINE=\"$MG_ENGINE\" && pfade_reparieren",
            ])
            .env("MG_ENGINE", paths::engine_dir(&layout.home))
            .current_dir(&layout.code)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

pub fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(&addr(port), Duration::from_millis(300)).is_ok()
}

/// Antwortet auf dem Port wirklich unsere Web-App? (`/api/queue` liefert JSON mit `running`.)
pub fn is_web_app(port: u16) -> bool {
    http_get(port, "/api/queue")
        .is_some_and(|(status, body)| status == 200 && body.contains("\"running\""))
}

/// Minimaler HTTP/1.0-GET ohne Abhängigkeiten. Gibt Statuscode und Körper zurück.
pub fn http_get(port: u16, path: &str) -> Option<(u16, String)> {
    let mut s = TcpStream::connect_timeout(&addr(port), Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    write!(
        s,
        "GET {path} HTTP/1.0\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let status = text.split_whitespace().nth(1)?.parse().ok()?;
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    Some((status, body))
}

/// Letzte Zeilen einer Logdatei für die Fehleranzeige.
pub fn tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Unix: eigene Prozessgruppe, damit `uv` samt Python-Kind gemeinsam beendet wird.
/// Windows: kein Konsolenfenster.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

fn terminate(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    #[cfg(unix)]
    {
        let pgid = child.id() as libc::pid_t;
        // SAFETY: Signal an die eigene, beim Start angelegte Prozessgruppe
        unsafe { libc::kill(-pgid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                // Enkel (z. B. Python unter uv) können die Gruppe überleben
                unsafe { libc::kill(-pgid, libc::SIGKILL) };
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        // ganzer Prozessbaum (uv → python)
        let mut cmd = Command::new("taskkill");
        cmd.args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        let _ = cmd.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_with_and_without_vae_chunk() {
        let m = Path::new("/m");
        let c = EngineConf {
            mode: Mode::Single,
            vae_chunk: None,
            lm_cpu: false,
        };
        assert_eq!(
            server_args(m, &c),
            ["--models", "/m", "--max-batch", "1"].map(OsString::from)
        );
        let c = EngineConf {
            vae_chunk: Some("256".into()),
            ..c
        };
        assert_eq!(server_args(m, &c).len(), 6);
    }

    #[test]
    fn tail_returns_last_lines() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("x.log");
        std::fs::write(&f, "a\nb\nc\nd\n").unwrap();
        assert_eq!(tail(&f, 2), "c\nd");
        assert_eq!(tail(&t.path().join("fehlt.log"), 5), "");
    }

    /// Projektordner aus dem Repository (Web-App, Python-Umgebung) mit Ersatz-ace-server und freien Ports.
    #[cfg(unix)]
    fn fake_project(ace: &str) -> (tempfile::TempDir, Ports) {
        use std::os::unix::fs::PermissionsExt;
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let t = tempfile::tempdir().unwrap();
        // kopieren, nicht verlinken: die Web-App findet ihren Ordner über den aufgelösten Pfad ihrer Dateien
        fn copy(from: &Path, to: &Path) {
            if from.is_dir() {
                std::fs::create_dir_all(to).unwrap();
                for e in std::fs::read_dir(from).unwrap() {
                    let e = e.unwrap();
                    if e.file_name() != "__pycache__" {
                        copy(&e.path(), &to.join(e.file_name()));
                    }
                }
            } else {
                std::fs::copy(from, to).unwrap();
            }
        }
        for name in ["app", "static", "pyproject.toml", "uv.lock"] {
            copy(&repo.join(name), &t.path().join(name));
        }
        let free = || {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let ports = Ports {
            web: free(),
            synth: free(),
            lm: free(),
        };
        std::fs::create_dir_all(t.path().join("scripts")).unwrap();
        std::fs::write(
            t.path().join("scripts/common.sh"),
            format!(
                "PORT={}\nSYNTH_PORT={}\nLM_PORT={}\n",
                ports.web, ports.synth, ports.lm
            ),
        )
        .unwrap();
        let build = paths::engine_dir(t.path()).join("build");
        let models = paths::models_dir(t.path());
        std::fs::create_dir_all(&build).unwrap();
        std::fs::create_dir_all(&models).unwrap();
        for m in [
            "vae-BF16.gguf",
            "acestep-v15-sft-Q8_0.gguf",
            "acestep-5Hz-lm-0.6B-Q8_0.gguf",
        ] {
            std::fs::write(models.join(m), b"").unwrap();
        }
        let exe = build.join("ace-server");
        std::fs::write(&exe, ace).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        (t, ports)
    }

    #[cfg(unix)]
    fn post(port: u16, path: &str) {
        let mut s = TcpStream::connect(addr(port)).unwrap();
        write!(s, "POST {path} HTTP/1.0\r\nHost: localhost:{port}\r\nX-Lang: de\r\nContent-Length: 0\r\n\r\n").unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        assert!(buf.contains("\"desktop\":true"), "{buf}");
    }

    /// Ganzer Ablauf mit echter Web-App: starten, Modus split mit CPU-Sprachmodell, Beenden über /api/shutdown.
    #[cfg(unix)]
    #[test]
    fn start_and_desktop_shutdown() {
        if paths::uv().is_none() {
            eprintln!("uv fehlt, Test übersprungen");
            return;
        }
        let fake = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fake-ace-server.py")
            .canonicalize()
            .unwrap();
        let (t, ports) = fake_project(&format!(
            "#!/bin/sh\nexec python3 '{}' \"$@\"\n",
            fake.display()
        ));
        let sup = Supervisor::new(Layout {
            code: t.path().to_path_buf(),
            home: t.path().to_path_buf(),
        });
        let steps = Mutex::new(Vec::new());
        sup.start(&|s| steps.lock().unwrap().push(s))
            .unwrap_or_else(|f| {
                panic!(
                    "{f:?}\n{}",
                    f.log.as_deref().map(|p| tail(p, 30)).unwrap_or_default()
                )
            });
        assert_eq!(
            *steps.lock().unwrap(),
            [Step::Check, Step::Lm, Step::Synth, Step::Web, Step::Wait]
        );
        assert!(is_web_app(ports.web));
        let (_, props) = http_get(ports.lm, "/props").unwrap();
        assert!(
            props.contains("\"backend\": \"CPU\"") && props.contains("--device"),
            "{props}"
        );

        post(ports.web, "/api/shutdown");
        let deadline = Instant::now() + Duration::from_secs(15);
        let ev = loop {
            if let Some(ev) = sup.poll() {
                break ev;
            }
            assert!(Instant::now() < deadline, "Web-App hat sich nicht beendet");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(ev, Event::WebExited(Some("quit".into())));
        sup.stop();
        assert!(!port_open(ports.synth) && !port_open(ports.lm) && !port_open(ports.web));
    }

    /// Stürzt ace-server sofort ab, kommt ein Fehler mit seiner Logdatei, nicht erst nach dem Timeout.
    #[cfg(unix)]
    #[test]
    fn crashing_engine_reports_log() {
        let (t, _) = fake_project("#!/bin/sh\necho kaputt >&2\nexit 3\n");
        let sup = Supervisor::new(Layout {
            code: t.path().to_path_buf(),
            home: t.path().to_path_buf(),
        });
        let started = Instant::now();
        let f = sup.start(&|_| {}).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(20));
        assert!(f.key.starts_with("err_timeout_"), "{f:?}");
        assert!(tail(f.log.as_deref().unwrap(), 5).contains("kaputt"));
        sup.stop();
    }

    #[test]
    fn closed_port_is_not_web_app() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        assert!(!port_open(port));
        assert!(!is_web_app(port));
    }
}
