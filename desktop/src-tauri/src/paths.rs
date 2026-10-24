//! Wo was liegt. Die App trennt zwei Ordner:
//! - **Programmcode** (`code`): Web-App, Oberfläche, `scripts/common.sh`. In der installierten App die mitgelieferten
//!   Ressourcen (schreibgeschützt), beim Entwickeln das Repository.
//! - **Datenordner** (`home`): `engine/` (Modellserver und Modelle), `data/` (Songs, Datenbank, Logs), `.venv`.
//!   Standard `~/MusicGenerator` – derselbe Ordner wie bei den Installer-Skripten, vorhandene Installationen werden genutzt.
//!
//! Bei Installationen über die Skripte sind beide Ordner derselbe (`~/MusicGenerator`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Ordnername der Installer (`~/MusicGenerator`, `%USERPROFILE%\MusicGenerator`).
const DEFAULT_DIR: &str = "MusicGenerator";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub code: PathBuf,
    pub home: PathBuf,
}

impl Layout {
    /// Programmcode und Daten getrennt (installierte App)?
    pub fn bundled(&self) -> bool {
        self.code != self.home
    }
}

fn repo_dir() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    p.canonicalize().unwrap_or(p)
}

/// Programmcode: im Debug-Build das Repository, sonst `<Ressourcen>/project`.
pub fn code_dir(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(debug_assertions) {
        candidates.push(repo_dir());
    }
    if let Some(r) = resource_dir {
        candidates.push(r.join("project"));
    }
    candidates.into_iter().find(|p| is_project(p))
}

/// Datenordner: `MG_HOME` → gespeicherte Wahl → `~/MusicGenerator`. Auch beim Entwickeln nicht das Repository,
/// damit dort keine Test-Engine eine fertige Installation vortäuscht.
pub fn home_dir(saved: Option<&Path>) -> PathBuf {
    if let Some(p) = std::env::var_os("MG_HOME").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    saved.map(Path::to_path_buf).unwrap_or_else(default_home)
}

pub fn default_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_DIR)
}

/// Ein Programmordner hat die Web-App und die gemeinsamen Einstellungen.
pub fn is_project(p: &Path) -> bool {
    p.join("app").join("main.py").is_file() && p.join("scripts").join("common.sh").is_file()
}

/// Vollständig eingerichtet: Modellserver und alle nötigen Modelle vorhanden.
pub fn is_installed(home: &Path) -> bool {
    ace_server(home).is_some() && missing_models(home).is_empty()
}

pub fn engine_dir(root: &Path) -> PathBuf {
    root.join("engine").join("acestep.cpp")
}

pub fn models_dir(root: &Path) -> PathBuf {
    engine_dir(root).join("models")
}

pub fn data_dir(root: &Path) -> PathBuf {
    root.join("data")
}

/// `build/ace-server` (Mac/Linux) bzw. `build\Release\ace-server.exe` oder `build\ace-server.exe` (Windows).
pub fn ace_server(root: &Path) -> Option<PathBuf> {
    let build = engine_dir(root).join("build");
    let candidates = if cfg!(windows) {
        vec![
            build.join("Release").join("ace-server.exe"),
            build.join("ace-server.exe"),
        ]
    } else {
        vec![build.join("ace-server")]
    };
    candidates.into_iter().find(|p| p.is_file())
}

/// Was an den Modellen fehlt (leer = vollständig). Gleiche Prüfung wie in `Music Generator ON.command`.
pub fn missing_models(root: &Path) -> Vec<String> {
    let dir = models_dir(root);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let has = |pred: &dyn Fn(&str) -> bool| names.iter().any(|n| pred(n));
    let mut missing = Vec::new();
    if has(&|n| n.ends_with(".part")) {
        missing.push("*.part".to_string());
    }
    if !has(&|n| n == "vae-BF16.gguf") {
        missing.push("vae-BF16.gguf".to_string());
    }
    if !has(&|n| n.starts_with("acestep-v15-") && n.ends_with(".gguf")) {
        missing.push("acestep-v15-*.gguf".to_string());
    }
    if !has(&|n| n.starts_with("acestep-5Hz-lm-") && n.ends_with(".gguf")) {
        missing.push("acestep-5Hz-lm-*.gguf".to_string());
    }
    missing
}

/// Zusätzliche Suchorte für Werkzeuge, wie `export PATH=…` in `common.sh` bzw. `$env:PATH` in `common.ps1`.
pub fn extra_path_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".local").join("bin"));
        v.push(home.join(".cargo").join("bin"));
    }
    if cfg!(unix) {
        for p in [
            "/opt/rocm/bin",
            "/opt/cuda/bin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
        ] {
            v.push(PathBuf::from(p));
        }
    }
    v
}

/// PATH für Kindprozesse: Zusatzorte vor dem geerbten PATH (ohne Ordner im AppImage).
pub fn child_path() -> OsString {
    let appdir = appimage_dir();
    let mut dirs = extra_path_dirs();
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(
            std::env::split_paths(&p).filter(|d| appdir.as_ref().is_none_or(|a| !d.starts_with(a))),
        );
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// Läuft die App als AppImage, dessen eingehängter Ordner (`APPDIR`, nur wenn die App wirklich darin liegt).
pub fn appimage_dir() -> Option<PathBuf> {
    let appdir = PathBuf::from(std::env::var_os("APPDIR")?);
    let exe = std::env::current_exe().ok()?;
    exe.starts_with(&appdir).then_some(appdir)
}

/// Umgebung eines Kindprozesses (Python, uv, Modellserver) vorbereiten: erweiterter PATH, und im AppImage alle Pfade
/// entfernen, die in das AppImage zeigen. Dessen Starter setzt u. a. `PYTHONHOME`, `PYTHONPATH` und `LD_LIBRARY_PATH`
/// darauf; Python fände sonst seine Standardbibliothek nicht („Failed to import encodings module“).
pub fn prepare(cmd: &mut Command) {
    if let Some(appdir) = appimage_dir() {
        for (k, v) in appimage_env_fixes(std::env::vars_os(), &appdir) {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
    }
    cmd.env("PATH", child_path());
}

/// Änderungen an der Umgebung: `None` = Variable entfernen, `Some` = neuer Wert ohne die AppImage-Pfade.
pub fn appimage_env_fixes(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    appdir: &Path,
) -> Vec<(OsString, Option<OsString>)> {
    let inside = appdir.to_string_lossy().into_owned();
    let mut out = Vec::new();
    for (k, v) in vars {
        if k == "PYTHONHOME" || k == "PYTHONPATH" {
            out.push((k, None));
            continue;
        }
        if !v.to_string_lossy().contains(inside.as_str()) {
            continue;
        }
        let kept: Vec<PathBuf> = std::env::split_paths(&v)
            .filter(|p| !p.as_os_str().is_empty() && !p.starts_with(appdir))
            .collect();
        let value = (!kept.is_empty()).then(|| std::env::join_paths(kept).unwrap_or_default());
        out.push((k, value));
    }
    out
}

/// `uv`: zuerst das mit der App ausgelieferte `mg-uv` (neben der ausführbaren Datei; eigener Name, damit es unter
/// Linux nicht mit einem `uv` aus der Paketverwaltung in `/usr/bin` kollidiert), dann `uv` aus dem PATH.
pub fn uv() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "mg-uv.exe" } else { "mg-uv" };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(exe)))
        .filter(|p| p.is_file())
        .or_else(|| which("uv"))
}

/// Programm im erweiterten PATH suchen.
pub fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::split_paths(&child_path())
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"").unwrap();
    }

    #[test]
    fn models_complete_and_incomplete() {
        let t = tempfile::tempdir().unwrap();
        let m = models_dir(t.path());
        assert_eq!(missing_models(t.path()).len(), 3);
        touch(&m.join("vae-BF16.gguf"));
        touch(&m.join("acestep-v15-sft-Q8_0.gguf"));
        touch(&m.join("acestep-5Hz-lm-0.6B-Q8_0.gguf"));
        assert!(missing_models(t.path()).is_empty());
        touch(&m.join("acestep-5Hz-lm-4B-Q8_0.gguf.part"));
        assert_eq!(missing_models(t.path()), vec!["*.part"]);
    }

    #[test]
    fn ace_server_location() {
        let t = tempfile::tempdir().unwrap();
        assert!(ace_server(t.path()).is_none());
        let build = engine_dir(t.path()).join("build");
        let p = if cfg!(windows) {
            build.join("Release").join("ace-server.exe")
        } else {
            build.join("ace-server")
        };
        touch(&p);
        assert_eq!(ace_server(t.path()), Some(p));
    }

    #[test]
    fn repo_is_project() {
        assert!(is_project(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
        ));
        assert!(!is_project(Path::new(env!("CARGO_MANIFEST_DIR"))));
        assert_eq!(code_dir(None), Some(repo_dir()));
    }

    #[test]
    fn layout_bundled() {
        let l = Layout {
            code: "/a".into(),
            home: "/a".into(),
        };
        assert!(!l.bundled());
        assert!(Layout {
            home: "/b".into(),
            ..l
        }
        .bundled());
    }

    // AppImage gibt es nur unter Linux; die Beispielpfade nutzen „:“ als Trenner (Windows: „;“)
    #[cfg(unix)]
    #[test]
    fn appimage_paths_are_removed() {
        let a = Path::new("/tmp/.mount_Music123");
        let vars = [
            ("PYTHONHOME", "/tmp/.mount_Music123/usr/"),
            ("PYTHONPATH", "/tmp/.mount_Music123/usr/share/pyshared/:"),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_Music123//usr/lib/:/opt/rocm/lib",
            ),
            (
                "GST_PLUGIN_SYSTEM_PATH_1_0",
                "/tmp/.mount_Music123/usr/lib/gstreamer-1.0:",
            ),
            ("HOME", "/home/x"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let fixes: std::collections::HashMap<_, _> =
            appimage_env_fixes(vars, a).into_iter().collect();
        assert_eq!(fixes[&OsString::from("PYTHONHOME")], None);
        assert_eq!(fixes[&OsString::from("PYTHONPATH")], None);
        assert_eq!(
            fixes[&OsString::from("LD_LIBRARY_PATH")],
            Some(OsString::from("/opt/rocm/lib"))
        );
        assert_eq!(fixes[&OsString::from("GST_PLUGIN_SYSTEM_PATH_1_0")], None);
        assert!(
            !fixes.contains_key(&OsString::from("HOME")),
            "fremde Variablen bleiben"
        );
    }
}
