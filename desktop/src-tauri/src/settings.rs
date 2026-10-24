//! Einstellungen der Hülle (Konfigurationsordner der App, nicht im Projektordner).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// zuletzt in der Web-App gewählte Oberflächensprache
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// gewählter Datenordner (Engine, Modelle, Songs); fehlt = `~/MusicGenerator`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<std::path::PathBuf>,
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_broken_file() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub").join("settings.json");
        assert_eq!(load(&p), Settings::default());
        save(
            &p,
            &Settings {
                lang: Some("uk".into()),
                home: Some("/daten/MusicGenerator".into()),
            },
        );
        assert_eq!(load(&p).lang.as_deref(), Some("uk"));
        assert_eq!(load(&p).home, Some("/daten/MusicGenerator".into()));
        std::fs::write(&p, "{kaputt").unwrap();
        assert_eq!(load(&p), Settings::default());
    }
}
