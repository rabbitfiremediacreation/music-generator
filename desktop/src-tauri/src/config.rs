//! Einstellungen aus dem Projektordner: Ports aus `scripts/common.sh` (eine Quelle für alle Plattformen, wie bei
//! `scripts/common.ps1`) und die vom Installer geschriebene `data/engine.conf`.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ports {
    /// Web-App (uvicorn)
    pub web: u16,
    /// ace-server Synthese (GPU)
    pub synth: u16,
    /// ace-server Sprachmodell (nur Modus split, CPU)
    pub lm: u16,
}

impl Default for Ports {
    fn default() -> Self {
        Ports {
            web: 8765,
            synth: 8085,
            lm: 8086,
        }
    }
}

/// Liest `PORT=…`, `SYNTH_PORT=…`, `LM_PORT=…` aus dem Text von `common.sh`. Fehlende Werte bleiben beim Standard.
pub fn parse_ports(common_sh: &str) -> Ports {
    let mut p = Ports::default();
    for line in common_sh.lines() {
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let value = rest
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        let Ok(n) = value.parse::<u16>() else {
            continue;
        };
        match key.trim() {
            "PORT" => p.web = n,
            "SYNTH_PORT" => p.synth = n,
            "LM_PORT" => p.lm = n,
            _ => {}
        }
    }
    p
}

pub fn load_ports(root: &Path) -> Ports {
    std::fs::read_to_string(root.join("scripts").join("common.sh"))
        .map(|s| parse_ports(&s))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Sprachmodell auf eigenem CPU-Server, Synthese auf der Grafik
    Split,
    /// alles auf einem Server
    Single,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConf {
    pub mode: Mode,
    pub vae_chunk: Option<String>,
    pub lm_cpu: bool,
}

/// Gleiche Regeln wie `lade_conf` in `common.sh` bzw. `ON.ps1`: key=value, die letzte Zeile gewinnt,
/// `LM_DEVICE=cpu` erzwingt den Modus split. `default_mode` unterscheidet sich je Plattform
/// (Mac/Linux: split, Windows: single), falls die Datei fehlt oder keinen MODE nennt.
pub fn parse_engine_conf(text: &str, default_mode: Mode) -> EngineConf {
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim(), v.trim());
        }
    }
    let lm_cpu = kv.get("LM_DEVICE").is_some_and(|v| *v == "cpu");
    let mode = match kv.get("MODE").copied() {
        _ if lm_cpu => Mode::Split,
        Some("single") => Mode::Single,
        Some("split") => Mode::Split,
        _ => default_mode,
    };
    let vae_chunk = kv
        .get("VAE_CHUNK")
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());
    EngineConf {
        mode,
        vae_chunk,
        lm_cpu,
    }
}

pub fn default_mode() -> Mode {
    if cfg!(windows) {
        Mode::Single
    } else {
        Mode::Split
    }
}

pub fn load_engine_conf(root: &Path) -> EngineConf {
    let text = std::fs::read_to_string(root.join("data").join("engine.conf")).unwrap_or_default();
    parse_engine_conf(&text, default_mode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_from_common_sh() {
        let sh = "ROOT=x\nPORT=8765         # Web-App\nSYNTH_PORT=9085   # ace-server\nLM_PORT=\"9086\"\n";
        assert_eq!(
            parse_ports(sh),
            Ports {
                web: 8765,
                synth: 9085,
                lm: 9086
            }
        );
    }

    #[test]
    fn ports_default_when_missing() {
        assert_eq!(parse_ports("FOO=1\n"), Ports::default());
    }

    #[test]
    fn ports_of_real_common_sh() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(load_ports(&root), Ports::default());
    }

    #[test]
    fn engine_conf_rules() {
        let c = parse_engine_conf("MODE=single\nVAE_CHUNK=512\n", Mode::Split);
        assert_eq!(
            c,
            EngineConf {
                mode: Mode::Single,
                vae_chunk: Some("512".into()),
                lm_cpu: false
            }
        );

        let c = parse_engine_conf("MODE=single\nLM_DEVICE=cpu\n", Mode::Single);
        assert_eq!(c.mode, Mode::Split, "LM_DEVICE=cpu erzwingt split");
        assert!(c.lm_cpu);

        let c = parse_engine_conf("MODE=split\nMODE=single\n", Mode::Split);
        assert_eq!(c.mode, Mode::Single, "letzte Zeile gewinnt");

        assert_eq!(parse_engine_conf("", Mode::Single).mode, Mode::Single);
        assert_eq!(
            parse_engine_conf("VAE_CHUNK=\n", Mode::Split).vae_chunk,
            None
        );
    }
}
