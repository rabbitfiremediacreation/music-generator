//! Speicher im Datenordner: Modelle anzeigen und löschen, Cache leeren, alles zurücksetzen. Songs bleiben immer.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::installer::{self, Manifest};
use crate::paths;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelItem {
    pub name: String,
    /// lm, dit, xl, vae, embedding, other (Anzeige: `kind_*` in den Texten)
    pub kind: &'static str,
    /// Sprachmodellgröße (0.6B …), sonst leer
    pub size: String,
    pub bytes: u64,
    /// vom aktuellen Betrieb gebraucht
    pub active: bool,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub home: String,
    pub models: Vec<ModelItem>,
    pub engine_bytes: u64,
    pub venv_bytes: u64,
    pub cache_bytes: u64,
    pub songs_bytes: u64,
    pub total_bytes: u64,
    pub free_gb: u64,
}

/// Sprachmodell aus `data/engine.conf` (`LM_FILE`), sonst das erste vorhandene.
fn active_lm(home: &Path, present: &[String]) -> Option<String> {
    let conf =
        std::fs::read_to_string(paths::data_dir(home).join("engine.conf")).unwrap_or_default();
    conf.lines()
        .filter_map(|l| l.strip_prefix("LM_FILE="))
        .map(|v| v.trim().to_string())
        .rfind(|v| !v.is_empty())
        .or_else(|| {
            present
                .iter()
                .find(|n| n.starts_with("acestep-5Hz-lm-"))
                .cloned()
        })
}

fn classify(name: &str) -> (&'static str, String) {
    if let Some(size) = installer::LM_SIZES
        .iter()
        .find(|s| name == installer::lm_file(s))
    {
        return ("lm", size.to_string());
    }
    let kind = if name.contains("xl-sft") {
        "xl"
    } else if name.starts_with("acestep-v15-") {
        "dit"
    } else if name.starts_with("vae-") {
        "vae"
    } else if name.starts_with("Qwen3-Embedding") {
        "embedding"
    } else if name.starts_with("acestep-5Hz-lm-") {
        "lm"
    } else {
        "other"
    };
    (kind, String::new())
}

pub fn info(home: &Path, _manifest: &Manifest) -> Info {
    let dir = paths::models_dir(home);
    let mut present: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".gguf"))
                .collect()
        })
        .unwrap_or_default();
    present.sort();
    let lm = active_lm(home, &present);
    let models: Vec<ModelItem> = present
        .iter()
        .map(|name| {
            let (kind, size) = classify(name);
            let active = match kind {
                "lm" => lm.as_deref() == Some(name.as_str()),
                "dit" | "vae" | "embedding" => true,
                _ => false,
            };
            ModelItem {
                name: name.clone(),
                kind,
                size,
                bytes: file_size(&dir.join(name)),
                active,
                removable: !active,
            }
        })
        .collect();
    // Modellserver = Engine-Ordner ohne Modellordner (der enthält Modelle und Teil-Downloads)
    let engine_bytes = dir_size(&paths::engine_dir(home)).saturating_sub(dir_size(&dir));
    let venv_bytes = dir_size(&home.join(".venv"));
    let cache_bytes = cache_files(home).iter().map(|p| size_of(p)).sum();
    let songs_bytes = dir_size(&paths::data_dir(home).join("songs"));
    Info {
        home: home.to_string_lossy().into_owned(),
        total_bytes: models.iter().map(|m| m.bytes).sum::<u64>()
            + engine_bytes
            + venv_bytes
            + cache_bytes,
        models,
        engine_bytes,
        venv_bytes,
        cache_bytes,
        songs_bytes,
        free_gb: installer::free_gb(home),
    }
}

/// Belegter Platz für die kurze Anzeige im Einstellungsdialog (Engine mit Modellen, Python, Cache; ohne Songs).
pub fn used_bytes(home: &Path) -> u64 {
    let engine = paths::engine_dir(home);
    let outside: u64 = cache_files(home)
        .iter()
        .filter(|p| !p.starts_with(&engine))
        .map(|p| size_of(p))
        .sum();
    dir_size(&engine) + dir_size(&home.join(".venv")) + outside
}

/// Löscht ein Modell, das gerade nicht gebraucht wird. Gibt die freigegebenen Bytes zurück.
pub fn delete_model(home: &Path, manifest: &Manifest, name: &str) -> Result<u64, String> {
    let item = info(home, manifest)
        .models
        .into_iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("{name}: nicht gefunden"))?;
    if !item.removable {
        return Err(format!("{name}: wird gebraucht"));
    }
    std::fs::remove_file(paths::models_dir(home).join(&item.name)).map_err(|e| e.to_string())?;
    Ok(item.bytes)
}

/// Teil-Downloads, Reste der Engine-Einrichtung und Logs der Einrichtung. Laufende Server-Logs bleiben.
fn cache_files(home: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let list = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    for p in list(&paths::models_dir(home)) {
        if p.extension().is_some_and(|e| e == "part") {
            v.push(p);
        }
    }
    for p in list(&paths::data_dir(home)) {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let leftover = (n.starts_with("engine-")
            && (n.ends_with(".zip") || n.ends_with(".part") || p.is_dir()))
            || (n.starts_with("mg-") && n.ends_with(".zip"))
            || (n == "export" && p.is_dir()) // MP3-Zwischenspeicher der Web-App
            || matches!(
                n.as_str(),
                "setup.log" | "selftest.log" | "selftest-lm.log" | "build.log"
            );
        if leftover {
            v.push(p);
        }
    }
    v
}

pub fn clear_cache(home: &Path) -> u64 {
    let mut freed = 0;
    for p in cache_files(home) {
        let size = size_of(&p);
        let ok = if p.is_dir() {
            std::fs::remove_dir_all(&p).is_ok()
        } else {
            std::fs::remove_file(&p).is_ok()
        };
        if ok {
            freed += size;
        }
    }
    freed
}

/// Modellserver, Modelle, Python-Umgebung und `engine.conf` löschen; Songs und Datenbank bleiben.
pub fn reset(home: &Path) -> Result<u64, String> {
    let mut freed = clear_cache(home);
    for d in [paths::engine_dir(home), home.join(".venv")] {
        freed += dir_size(&d);
        if d.exists() {
            std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    let _ = std::fs::remove_file(paths::data_dir(home).join("engine.conf"));
    Ok(freed)
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn size_of(p: &Path) -> u64 {
    if p.is_dir() {
        dir_size(p)
    } else {
        file_size(p)
    }
}

/// Größe eines Ordners; symbolische Links zählen nicht mit (sonst doppelt).
pub fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, bytes: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![0u8; bytes]).unwrap();
    }

    fn home_with_models() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let m = paths::models_dir(t.path());
        for (n, b) in [
            ("vae-BF16.gguf", 10),
            ("Qwen3-Embedding-0.6B-Q8_0.gguf", 20),
            ("acestep-v15-sft-Q8_0.gguf", 30),
            ("acestep-v15-xl-sft-Q8_0.gguf", 40),
            ("acestep-5Hz-lm-0.6B-Q8_0.gguf", 5),
            ("acestep-5Hz-lm-4B-Q8_0.gguf", 50),
            ("acestep-5Hz-lm-1.7B-Q8_0.gguf.part", 7),
        ] {
            write(&m.join(n), b);
        }
        write(
            &paths::engine_dir(t.path()).join("build").join("ace-server"),
            3,
        );
        write(&t.path().join(".venv").join("x"), 4);
        write(&paths::data_dir(t.path()).join("setup.log"), 2);
        write(&paths::data_dir(t.path()).join("export").join("a.mp3"), 6);
        write(&paths::data_dir(t.path()).join("server.log"), 1);
        write(&paths::data_dir(t.path()).join("songs").join("a.mp3"), 9);
        std::fs::write(
            paths::data_dir(t.path()).join("engine.conf"),
            "MODE=single\nLM_FILE=acestep-5Hz-lm-0.6B-Q8_0.gguf\n",
        )
        .unwrap();
        t
    }

    #[test]
    fn info_marks_active_and_removable() {
        let t = home_with_models();
        let i = info(t.path(), &Manifest::default());
        let get = |n: &str| i.models.iter().find(|m| m.name == n).unwrap().clone();
        assert!(get("acestep-5Hz-lm-0.6B-Q8_0.gguf").active);
        assert!(get("acestep-5Hz-lm-4B-Q8_0.gguf").removable);
        assert_eq!(get("acestep-5Hz-lm-4B-Q8_0.gguf").size, "4B");
        assert_eq!(get("acestep-v15-xl-sft-Q8_0.gguf").kind, "xl");
        assert!(!get("vae-BF16.gguf").removable);
        assert_eq!(i.models.len(), 6, ".part zählt nicht als Modell");
        assert_eq!(i.cache_bytes, 7 + 2 + 6);
        assert_eq!(i.engine_bytes, 3);
        assert_eq!(i.venv_bytes, 4);
        assert_eq!(i.songs_bytes, 9);
    }

    #[test]
    fn delete_only_unused() {
        let t = home_with_models();
        let m = Manifest::default();
        assert!(delete_model(t.path(), &m, "vae-BF16.gguf").is_err());
        assert!(delete_model(t.path(), &m, "acestep-5Hz-lm-0.6B-Q8_0.gguf").is_err());
        assert!(delete_model(t.path(), &m, "../../etwas").is_err());
        assert_eq!(
            delete_model(t.path(), &m, "acestep-5Hz-lm-4B-Q8_0.gguf").unwrap(),
            50
        );
        assert!(!paths::models_dir(t.path())
            .join("acestep-5Hz-lm-4B-Q8_0.gguf")
            .exists());
    }

    #[test]
    fn cache_and_reset_keep_songs() {
        let t = home_with_models();
        assert_eq!(clear_cache(t.path()), 15);
        assert!(
            paths::data_dir(t.path()).join("server.log").exists(),
            "Logs laufender Server bleiben"
        );
        reset(t.path()).unwrap();
        assert!(!paths::engine_dir(t.path()).exists() && !t.path().join(".venv").exists());
        assert!(!paths::data_dir(t.path()).join("engine.conf").exists());
        assert!(
            paths::data_dir(t.path())
                .join("songs")
                .join("a.mp3")
                .exists(),
            "Songs bleiben"
        );
        assert!(!paths::is_installed(t.path()));
    }
}
