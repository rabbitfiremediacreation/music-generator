//! Einrichtung in der App: Engine und Modelle laden, Python-Umgebung anlegen, Grafik testen, `data/engine.conf`
//! schreiben. Nachbau der Schritte aus `Mac-Install.command`, `Windows-Install.ps1` und `arch_linux.sh`.
//! Versionen, Adressen und Prüfsummen kommen aus `scripts/common.sh` (eine Quelle für Skripte und App).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths::{self, Layout};
use crate::supervisor;

pub const LM_SIZES: [&str; 3] = ["0.6B", "1.7B", "4B"];
const TEST_PORT: u16 = 8099;

// ------------------------------------------------------------------ Manifest aus common.sh

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelFile {
    pub name: String,
    pub mb: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnginePackage {
    pub rev: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub acestep_rev: String,
    pub hf_base: String,
    pub models: Vec<ModelFile>,
    vars: HashMap<String, String>,
}

impl Manifest {
    /// Fertige Engine für diese Plattform (Windows x64, macOS arm64, Linux x64).
    pub fn engine(&self) -> Option<EnginePackage> {
        let prefix = if cfg!(windows) {
            "ENGINE_WIN"
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "ENGINE_MAC"
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            "ENGINE_LINUX"
        } else {
            return None;
        };
        self.engine_for(prefix)
    }

    pub fn engine_for(&self, prefix: &str) -> Option<EnginePackage> {
        let get = |k: &str| {
            self.vars
                .get(&format!("{prefix}_{k}"))
                .filter(|v| !v.is_empty())
                .cloned()
        };
        Some(EnginePackage {
            rev: get("REV")?,
            url: get("URL")?,
            sha256: get("SHA256")?,
        })
    }

    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }
}

/// Liest `KEY="value"`-Zeilen und die mehrzeilige Tabelle `MODEL_FILES` (Name:MB:SHA-256), wie `common.ps1`.
pub fn parse_manifest(common_sh: &str) -> Manifest {
    let mut m = Manifest::default();
    for line in common_sh.lines() {
        let line = line.trim();
        let entry = line
            .strip_prefix("MODEL_FILES=\"")
            .unwrap_or(line)
            .trim_end_matches('"');
        let parts: Vec<&str> = entry.split(':').collect();
        if parts.len() == 3 && parts[0].ends_with(".gguf") && parts[2].len() == 64 {
            if let Ok(mb) = parts[1].parse() {
                m.models.push(ModelFile {
                    name: parts[0].into(),
                    mb,
                    sha256: parts[2].to_lowercase(),
                });
                continue;
            }
        }
        if line.starts_with('#') || line.starts_with("export ") {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                let v = v.split(" #").next().unwrap_or("").trim().trim_matches('"');
                m.vars.insert(k.to_string(), v.to_string());
            }
        }
    }
    m.acestep_rev = m.vars.get("ACESTEP_REV").cloned().unwrap_or_default();
    m.hf_base = m.vars.get("HF_BASE").cloned().unwrap_or_default();
    m
}

pub fn load_manifest(code: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(code.join("scripts").join("common.sh"))
        .map_err(|e| e.to_string())?;
    Ok(parse_manifest(&text))
}

pub fn lm_file(size: &str) -> String {
    format!("acestep-5Hz-lm-{size}-Q8_0.gguf")
}

/// Grundmodelle + gewähltes Sprachmodell (+ XL), wie `FILES=…` in den Installern.
pub fn files_needed<'a>(m: &'a Manifest, choice: &Choice) -> Vec<&'a ModelFile> {
    let lm = lm_file(&choice.lm);
    m.models
        .iter()
        .filter(|f| {
            let is_lm = f.name.starts_with("acestep-5Hz-lm-");
            let is_xl = f.name.contains("xl-sft");
            (!is_lm && !is_xl) || f.name == lm || (is_xl && choice.xl)
        })
        .collect()
}

// ------------------------------------------------------------------ Hardware und Empfehlung

#[derive(Debug, Clone, Default, Serialize)]
pub struct Hardware {
    pub ram_gb: u64,
    /// Grafikspeicher (NVIDIA über nvidia-smi, AMD unter Linux über sysfs); 0 = unbekannt bzw. gemeinsamer Speicher
    pub vram_gb: u64,
    pub gpus: Vec<String>,
    /// Linux: AMD- oder Intel-Grafik vorhanden (Vulkan, VAE-Kachel 256 als sichere Vorgabe)
    pub amd_or_intel: bool,
    pub free_gb: u64,
}

pub fn detect_hardware(home: &Path) -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let ram_gb = (sys.total_memory() + (1 << 29)) >> 30;
    let mut hw = Hardware {
        ram_gb,
        free_gb: free_gb(home),
        ..Default::default()
    };
    if cfg!(target_os = "macos") {
        return hw;
    }
    if let Some(out) = run_capture(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    ) {
        for line in out.lines().filter(|l| !l.trim().is_empty()) {
            let (name, mem) = line.rsplit_once(',').unwrap_or((line, "0"));
            hw.gpus.push(name.trim().to_string());
            let gb = (mem.trim().parse::<u64>().unwrap_or(0) + 512) / 1024;
            hw.vram_gb = hw.vram_gb.max(gb);
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(cards) = std::fs::read_dir("/sys/class/drm") {
        for card in cards.flatten() {
            let dev = card.path().join("device");
            let vendor = std::fs::read_to_string(dev.join("vendor")).unwrap_or_default();
            match vendor.trim() {
                "0x1002" => {
                    hw.amd_or_intel = true;
                    let bytes: u64 = std::fs::read_to_string(dev.join("mem_info_vram_total"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0);
                    if hw.vram_gb == 0 && bytes > 0 {
                        hw.vram_gb = (bytes / 1_048_576 + 512) / 1024;
                    }
                }
                "0x8086" => hw.amd_or_intel = true,
                _ => {}
            }
        }
    }
    hw
}

/// Freier Platz auf dem Laufwerk des (ggf. noch nicht vorhandenen) Ordners.
pub fn free_gb(dir: &Path) -> u64 {
    let mut existing = dir.to_path_buf();
    while !existing.exists() {
        if !existing.pop() {
            break;
        }
    }
    let existing = existing.canonicalize().unwrap_or(existing);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| existing.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space() >> 30)
        .unwrap_or(0)
}

/// Sprachmodell nach Arbeitsspeicher, begrenzt durch den Grafikspeicher (Sprachmodell + 1 GB KV-Cache müssen passen).
pub fn recommend_lm(hw: &Hardware) -> &'static str {
    let by_ram = if hw.ram_gb < 12 {
        0
    } else if hw.ram_gb < 24 {
        1
    } else {
        2
    };
    let by_vram = match hw.vram_gb {
        0 => 2,
        v if v < 6 => 0,
        v if v < 8 => 1,
        _ => 2,
    };
    LM_SIZES[by_ram.min(by_vram)]
}

/// Kachelgröße des VAE-Decoders: Mac nach Arbeitsspeicher (geteilter Speicher), sonst nach Grafikspeicher.
pub fn vae_chunk(hw: &Hardware) -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        return match hw.ram_gb {
            r if r < 12 => Some("256"),
            r if r < 16 => Some("512"),
            _ => None,
        };
    }
    match hw.vram_gb {
        0 if hw.amd_or_intel => Some("256"),
        0 => None,
        v if v < 8 => Some("128"),
        v if v < 12 => Some("256"),
        v if v < 16 => Some("512"),
        _ => None,
    }
}

// ------------------------------------------------------------------ Was zu tun ist

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Choice {
    pub lm: String,
    pub xl: bool,
}

/// Für den Einrichtungsdialog.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub home: String,
    pub hardware: Hardware,
    pub recommended: String,
    pub choice: Choice,
    /// MB je Sprachmodellgröße und XL, für die Anzeige
    pub sizes_mb: HashMap<String, u64>,
    /// bereits vorhandene Modelldateien
    pub present: Vec<String>,
    /// noch zu ladende MB bei der vorgeschlagenen Wahl (ohne Engine)
    pub base_mb: u64,
    pub engine_available: bool,
    pub engine_present: bool,
    /// schon eingerichtet (Aufruf aus „Speicher & Modelle“): Dialog zeigt „Zurück“
    pub installed: bool,
}

pub fn plan(layout: &Layout, m: &Manifest) -> Plan {
    let home = &layout.home;
    let hw = detect_hardware(home);
    let recommended = recommend_lm(&hw).to_string();
    let present: Vec<String> = m
        .models
        .iter()
        .filter(|f| paths::models_dir(home).join(&f.name).is_file())
        .map(|f| f.name.clone())
        .collect();
    // eine vorhandene Wahl bleibt (z. B. nach abgebrochener Einrichtung)
    let conf_lm = config_value(home, "LM_FILE");
    let lm = LM_SIZES
        .iter()
        .find(|s| conf_lm.as_deref() == Some(lm_file(s).as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| recommended.clone());
    let xl = present.iter().any(|n| n.contains("xl-sft"));
    let choice = Choice { lm, xl };
    let mut sizes_mb = HashMap::new();
    for f in &m.models {
        if let Some(size) = LM_SIZES.iter().find(|s| f.name == lm_file(s)) {
            sizes_mb.insert(size.to_string(), f.mb);
        } else if f.name.contains("xl-sft") {
            sizes_mb.insert("XL".to_string(), f.mb);
        }
    }
    let base_mb = m
        .models
        .iter()
        .filter(|f| !f.name.starts_with("acestep-5Hz-lm-") && !f.name.contains("xl-sft"))
        .filter(|f| !present.contains(&f.name))
        .map(|f| f.mb)
        .sum();
    Plan {
        home: home.to_string_lossy().into_owned(),
        hardware: hw,
        recommended,
        choice,
        sizes_mb,
        present,
        base_mb,
        engine_available: m.engine().is_some(),
        engine_present: paths::ace_server(home).is_some(),
        installed: paths::is_installed(home),
    }
}

fn config_value(home: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(paths::data_dir(home).join("engine.conf")).ok()?;
    text.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_string())
        .next_back()
}

// ------------------------------------------------------------------ Ablauf

/// Fortschritt für die Anzeige. `key` ist ein Schlüssel in `locales/*.json`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Progress {
    pub key: &'static str,
    pub file: String,
    pub index: usize,
    pub count: usize,
    pub done: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
}

impl Progress {
    fn step(key: &'static str) -> Self {
        Progress {
            key,
            file: String::new(),
            index: 0,
            count: 0,
            done: 0,
            total: 0,
            bytes_per_sec: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupError {
    pub key: &'static str,
    pub detail: String,
}

impl SetupError {
    fn new(key: &'static str, detail: impl ToString) -> Self {
        SetupError {
            key,
            detail: detail.to_string(),
        }
    }
}

pub struct Installer<'a> {
    pub layout: &'a Layout,
    pub manifest: &'a Manifest,
    pub cancel: &'a AtomicBool,
    pub report: &'a (dyn Fn(Progress) + Sync),
}

impl Installer<'_> {
    pub fn run(&self, choice: &Choice) -> Result<(), SetupError> {
        if !LM_SIZES.contains(&choice.lm.as_str()) {
            return Err(SetupError::new("setup_err_choice", &choice.lm));
        }
        let home = &self.layout.home;
        for d in [paths::models_dir(home), paths::data_dir(home)] {
            std::fs::create_dir_all(&d).map_err(|e| {
                SetupError::new("setup_err_folder", format!("{}: {e}", d.display()))
            })?;
        }
        let files = files_needed(self.manifest, choice);
        let missing: Vec<&ModelFile> = files
            .iter()
            .copied()
            .filter(|f| !paths::models_dir(home).join(&f.name).is_file())
            .collect();
        let need_gb = missing.iter().map(|f| f.mb).sum::<u64>().div_ceil(1024) + 3;
        let free = free_gb(home);
        if free > 0 && free < need_gb {
            return Err(SetupError::new(
                "setup_err_space",
                format!("{free}/{need_gb}"),
            ));
        }

        self.engine()?;
        for (i, f) in missing.iter().enumerate() {
            let url = format!("{}/{}", self.manifest.hf_base, f.name);
            let dest = paths::models_dir(home).join(&f.name);
            let mut p = Progress::step("setup_models");
            p.file = f.name.clone();
            p.index = i + 1;
            p.count = missing.len();
            p.total = f.mb << 20;
            self.download(&url, &dest, &f.sha256, p)?;
        }
        self.python()?;
        self.selftest_and_config(choice)?;
        Ok(())
    }

    fn check_cancel(&self) -> Result<(), SetupError> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(SetupError::new("setup_err_cancelled", ""))
        } else {
            Ok(())
        }
    }

    /// Fertige Programmdateien laden, prüfen und nach `engine/acestep.cpp/build` (Windows: `build/Release`) entpacken.
    fn engine(&self) -> Result<(), SetupError> {
        let home = &self.layout.home;
        let Some(pkg) = self.manifest.engine() else {
            return if paths::ace_server(home).is_some() {
                Ok(()) // selbst gebaut (z. B. Arch-Installer)
            } else {
                Err(SetupError::new("setup_err_no_engine", std::env::consts::OS))
            };
        };
        let build = paths::engine_dir(home).join("build");
        let target = if cfg!(windows) {
            build.join("Release")
        } else {
            build.clone()
        };
        let version = std::fs::read_to_string(target.join("VERSION.txt")).unwrap_or_default();
        let installed_rev = version
            .lines()
            .find_map(|l| l.strip_prefix("ENGINE_REV="))
            .map(str::trim);
        if paths::ace_server(home).is_some()
            && (installed_rev == Some(pkg.rev.as_str()) || version.is_empty())
        {
            return Ok(()); // passende Version oder selbst gebaut
        }

        // Mac: die fertigen Programmdateien laufen erst ab einer macOS-Version (ältere kompilieren mit Mac-Install.command --build)
        if cfg!(target_os = "macos") {
            let min: u32 = self
                .manifest
                .var("ENGINE_MAC_MIN_OS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let have: u32 = run_capture("sw_vers", &["-productVersion"])
                .and_then(|v| v.trim().split('.').next().and_then(|m| m.parse().ok()))
                .unwrap_or(0);
            if have < min {
                return Err(SetupError::new(
                    "setup_err_no_engine",
                    format!("macOS {have} < {min}"),
                ));
            }
        }
        let zip = paths::data_dir(home).join(format!("engine-{}.zip", pkg.rev));
        let mut p = Progress::step("setup_engine");
        p.file = pkg
            .url
            .rsplit('/')
            .next()
            .unwrap_or("engine.zip")
            .to_string();
        self.download(&pkg.url, &zip, &pkg.sha256, p)?;
        self.check_cancel()?;
        (self.report)(Progress::step("setup_unpack"));
        let tmp = paths::data_dir(home).join("engine-unpack");
        let _ = std::fs::remove_dir_all(&tmp);
        unzip(&zip, &tmp).map_err(|e| SetupError::new("setup_err_unpack", e))?;
        let src = single_subdir(&tmp).unwrap_or_else(|| tmp.clone());
        let exe = if cfg!(windows) {
            "ace-server.exe"
        } else {
            "ace-server"
        };
        if !src.join(exe).is_file() {
            return Err(SetupError::new(
                "setup_err_unpack",
                format!("{exe} fehlt im Paket"),
            ));
        }
        let _ = std::fs::remove_dir_all(&target);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        move_dir(&src, &target).map_err(|e| SetupError::new("setup_err_unpack", e))?;
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&zip);
        let stamp = format!(
            "PREBUILT={}\nENGINE_REV={}\nSOURCE={}\nZIP_SHA256={}\n",
            unix_now(),
            pkg.rev,
            pkg.url,
            pkg.sha256
        );
        let _ = std::fs::write(target.join("VERSION.txt"), stamp);
        Ok(())
    }

    /// Download mit Fortsetzen (`.part` + Range) und SHA-256-Prüfung; erst danach Umbenennen.
    fn download(
        &self,
        url: &str,
        dest: &Path,
        sha256: &str,
        mut p: Progress,
    ) -> Result<(), SetupError> {
        let part = dest.with_file_name(format!(
            "{}.part",
            dest.file_name().unwrap().to_string_lossy()
        ));
        let fail = |e: String| SetupError::new("setup_err_download", format!("{}: {e}", p.file));
        let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_body(None)
            .build()
            .new_agent();
        let mut req = agent.get(url);
        if have > 0 {
            req = req.header("Range", &format!("bytes={have}-"));
        }
        let mut resp = match req.call() {
            // schon vollständig geladen (Server lehnt den Bereich ab): nur noch prüfen
            Err(ureq::Error::StatusCode(416)) => None,
            Err(e) => return Err(fail(e.to_string())),
            Ok(r) => Some(r),
        };
        if let Some(resp) = resp.as_mut() {
            let resumed = resp.status().as_u16() == 206;
            if !resumed {
                have = 0;
            }
            let len: u64 = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if len > 0 {
                p.total = have + len;
            }
            let mut out = OpenOptions::new()
                .create(true)
                .write(true)
                .append(resumed)
                .truncate(!resumed)
                .open(&part)
                .map_err(|e| fail(e.to_string()))?;
            let mut reader = resp.body_mut().as_reader();
            let mut buf = vec![0u8; 1 << 20];
            let (mut last, start, start_have) = (Instant::now(), Instant::now(), have);
            p.done = have;
            (self.report)(p.clone());
            loop {
                self.check_cancel()?;
                let n = reader.read(&mut buf).map_err(|e| fail(e.to_string()))?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| fail(e.to_string()))?;
                have += n as u64;
                if last.elapsed() > Duration::from_millis(250) {
                    last = Instant::now();
                    p.done = have;
                    let secs = start.elapsed().as_secs_f64().max(0.001);
                    p.bytes_per_sec = ((have - start_have) as f64 / secs) as u64;
                    (self.report)(p.clone());
                }
            }
            out.flush().map_err(|e| fail(e.to_string()))?;
        }
        let mut v = p.clone();
        v.key = "setup_verify";
        v.done = have;
        (self.report)(v);
        let actual = sha256_file(&part).map_err(|e| fail(e.to_string()))?;
        if !actual.eq_ignore_ascii_case(sha256) {
            let _ = std::fs::remove_file(&part);
            return Err(SetupError::new("setup_err_checksum", &p.file));
        }
        std::fs::rename(&part, dest).map_err(|e| fail(e.to_string()))
    }

    /// Python-Pakete der Web-App (`uv sync`), Ausgabe in `data/setup.log`.
    fn python(&self) -> Result<(), SetupError> {
        self.check_cancel()?;
        (self.report)(Progress::step("setup_python"));
        let uv = paths::uv().ok_or_else(|| SetupError::new("setup_err_python", "uv"))?;
        let mut args = vec!["sync".to_string()];
        if self.layout.bundled() {
            args.push("--frozen".into());
        }
        let ok = self
            .run_logged(&uv, &args, "setup.log")
            .map_err(|e| SetupError::new("setup_err_python", e))?;
        if !ok {
            return Err(SetupError::new(
                "setup_err_python",
                self.log_tail("setup.log"),
            ));
        }
        Ok(())
    }

    /// Testet wie die Installer, ob das Sprachmodell auf der Grafik richtig rechnet (single), sonst CPU (split).
    /// Ein vorhandenes Ergebnis für dasselbe Sprachmodell und dieselbe Engine wird übernommen.
    fn selftest_and_config(&self, choice: &Choice) -> Result<(), SetupError> {
        let home = &self.layout.home;
        let lm = lm_file(&choice.lm);
        let hw = detect_hardware(home);
        let rev = tested_rev(home, &self.manifest.acestep_rev);
        let lm_cpu = config_value(home, "LM_DEVICE").as_deref() == Some("cpu");
        let old_mode = config_value(home, "MODE");
        let reuse = old_mode.is_some()
            && config_value(home, "LM_FILE").as_deref() == Some(lm.as_str())
            && config_value(home, "TESTED_REV")
                .map(|r| r.get(..7) == rev.get(..7))
                .unwrap_or(false);
        let vae = vae_chunk(&hw);
        let mode = if lm_cpu || reuse {
            old_mode.unwrap_or_else(|| "split".into())
        } else {
            self.check_cancel()?;
            (self.report)(Progress::step("setup_selftest"));
            if self.selftest(&lm, vae)? {
                "single".into()
            } else {
                "split".into()
            }
        };
        let mut lines = vec![
            format!("MODE={mode}"),
            format!("LM_FILE={lm}"),
            format!("TESTED_REV={rev}"),
            format!("VAE_CHUNK={}", vae.unwrap_or("")),
        ];
        if cfg!(target_os = "linux") {
            lines.push("BACKEND=vulkan".into());
        }
        if lm_cpu {
            lines.push("LM_DEVICE=cpu".into());
        }
        std::fs::write(
            paths::data_dir(home).join("engine.conf"),
            lines.join("\n") + "\n",
        )
        .map_err(|e| SetupError::new("setup_err_folder", e))
    }

    fn selftest(&self, lm: &str, vae: Option<&str>) -> Result<bool, SetupError> {
        let home = &self.layout.home;
        if supervisor::port_open(TEST_PORT) {
            return Ok(true); // wie arch_linux.sh: Testport belegt → Test überspringen, Modus single
        }
        let ace =
            paths::ace_server(home).ok_or_else(|| SetupError::new("setup_err_no_engine", ""))?;
        let mut args: Vec<String> = vec![
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            TEST_PORT.to_string(),
            "--models".into(),
            paths::models_dir(home).to_string_lossy().into_owned(),
            "--max-batch".into(),
            "1".into(),
        ];
        if let Some(v) = vae {
            args.extend(["--vae-chunk".into(), v.into()]);
        }
        let log = File::create(paths::data_dir(home).join("selftest.log"))
            .map_err(|e| SetupError::new("setup_err_selftest", e))?;
        let mut cmd = Command::new(&ace);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        supervisor::hide_console(&mut cmd);
        paths::prepare(&mut cmd);
        let mut server = cmd
            .spawn()
            .map_err(|e| SetupError::new("setup_err_selftest", e))?;
        let result = (|| {
            let deadline = Instant::now() + Duration::from_secs(120);
            while supervisor::http_get(TEST_PORT, "/health").map(|(s, _)| s) != Some(200) {
                self.check_cancel()?;
                if matches!(server.try_wait(), Ok(Some(_))) || Instant::now() > deadline {
                    return Err(SetupError::new(
                        "setup_err_selftest",
                        self.log_tail("selftest.log"),
                    ));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            let uv = paths::uv().ok_or_else(|| SetupError::new("setup_err_python", "uv"))?;
            let mut a: Vec<String> = supervisor::uv_run_args(self.layout)
                .into_iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            a.extend([
                "python".into(),
                "scripts/metal_selftest.py".into(),
                TEST_PORT.to_string(),
                lm.to_string(),
            ]);
            self.run_logged(&uv, &a, "selftest-lm.log")
                .map_err(|e| SetupError::new("setup_err_selftest", e))
        })();
        let _ = server.kill();
        let _ = server.wait();
        result
    }

    /// Programm im Programmordner mit Python-Umgebung ausführen, Ausgabe ins Log. `Ok(true)` = Exit 0.
    fn run_logged(&self, program: &Path, args: &[String], log_name: &str) -> Result<bool, String> {
        let log = File::create(paths::data_dir(&self.layout.home).join(log_name))
            .map_err(|e| e.to_string())?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&self.layout.code)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        paths::prepare(&mut cmd);
        for (k, v) in supervisor::python_env(self.layout) {
            cmd.env(k, v);
        }
        supervisor::hide_console(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("{}: {e}", program.display()))?;
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Ok(status.success());
            }
            if self.cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("abgebrochen".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn log_tail(&self, name: &str) -> String {
        supervisor::tail(&paths::data_dir(&self.layout.home).join(name), 15)
    }
}

/// Engine-Kennung für `TESTED_REV`: Windows nimmt wie `Windows-Install.ps1` die erste Zeile von `VERSION.txt`.
fn tested_rev(home: &Path, acestep_rev: &str) -> String {
    if cfg!(windows) {
        let v = paths::engine_dir(home)
            .join("build")
            .join("Release")
            .join("VERSION.txt");
        if let Some(first) = std::fs::read_to_string(v)
            .ok()
            .and_then(|t| t.lines().next().map(str::to_string))
        {
            return first;
        }
    }
    acestep_rev.to_string()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn run_capture(program: &str, args: &[&str]) -> Option<String> {
    let path = paths::which(program)?;
    let mut cmd = Command::new(path);
    cmd.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    supervisor::hide_console(&mut cmd);
    paths::prepare(&mut cmd);
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Entpacken mit Schutz vor Pfaden außerhalb des Ziels; Unix-Rechte (ausführbar) und symbolische Links
/// (z. B. `libggml.so.0` → `libggml.so.0.25.1`) bleiben erhalten.
pub fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(File::open(zip).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut links: Vec<(PathBuf, String)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        // Link: der Inhalt ist der Zielname. Nur Namen im selben Ordner, nichts, was hinauszeigt.
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            let mut target = String::new();
            entry
                .read_to_string(&mut target)
                .map_err(|e| e.to_string())?;
            if target.is_empty() || target.contains(['/', '\\']) || target == ".." {
                return Err(format!("unzulässiger Link {} -> {target}", rel.display()));
            }
            links.push((out, target));
            continue;
        }
        let mut f = File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777));
        }
    }
    // Links zuletzt anlegen, wenn ihre Ziele da sind. Ohne Link-Unterstützung (Windows): Ziel kopieren.
    for (link, target) in links {
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
        #[cfg(not(unix))]
        std::fs::copy(link.with_file_name(&target), &link).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Enthält der Ordner genau einen Unterordner (z. B. `acestep-cpp-macos-arm64-b7ba6d9/`), dessen Pfad.
fn single_subdir(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    match entries.as_slice() {
        [only] if only.path().is_dir() => Some(only.path()),
        _ => None,
    }
}

fn move_dir(src: &Path, dest: &Path) -> Result<(), String> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    // anderes Laufwerk: kopieren
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let to = dest.join(e.file_name());
        if e.path().is_dir() {
            move_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_manifest() -> Manifest {
        let code = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        load_manifest(&code).unwrap()
    }

    #[test]
    fn manifest_from_real_common_sh() {
        let m = repo_manifest();
        assert_eq!(m.acestep_rev.len(), 40, "ACESTEP_REV");
        assert!(
            m.hf_base.starts_with("https://huggingface.co/"),
            "{}",
            m.hf_base
        );
        assert_eq!(m.models.len(), 7);
        assert!(m.models.iter().all(|f| f.sha256.len() == 64 && f.mb > 0));
        for size in LM_SIZES {
            assert!(m.models.iter().any(|f| f.name == lm_file(size)), "{size}");
        }
        let win = m.engine_for("ENGINE_WIN").expect("Windows-Engine");
        assert!(win.url.starts_with("https://") && win.sha256.len() == 64);
        let mac = m.engine_for("ENGINE_MAC").expect("Mac-Engine");
        assert!(mac.url.ends_with(".zip"));
        assert_eq!(m.var("ENGINE_MAC_MIN_OS"), Some("26"));
        let linux = m.engine_for("ENGINE_LINUX").expect("Linux-Engine");
        assert_eq!(
            linux.rev,
            &m.acestep_rev[..7],
            "Linux-Engine passt zur ACESTEP_REV"
        );
    }

    #[test]
    fn needed_files_follow_choice() {
        let m = repo_manifest();
        let names = |c: &Choice| {
            files_needed(&m, c)
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
        };
        let small = names(&Choice {
            lm: "0.6B".into(),
            xl: false,
        });
        assert_eq!(small.len(), 4);
        assert!(small.contains(&lm_file("0.6B")) && !small.iter().any(|n| n.contains("xl")));
        let big = names(&Choice {
            lm: "4B".into(),
            xl: true,
        });
        assert_eq!(big.len(), 5);
        assert!(big.contains(&lm_file("4B")) && big.iter().any(|n| n.contains("xl-sft")));
    }

    #[test]
    fn recommendations_match_installers() {
        let hw = |ram_gb, vram_gb| Hardware {
            ram_gb,
            vram_gb,
            ..Default::default()
        };
        assert_eq!(recommend_lm(&hw(8, 0)), "0.6B");
        assert_eq!(recommend_lm(&hw(16, 0)), "1.7B");
        assert_eq!(recommend_lm(&hw(32, 0)), "4B");
        assert_eq!(recommend_lm(&hw(32, 4)), "0.6B", "GTX mit 4 GB");
        assert_eq!(recommend_lm(&hw(32, 6)), "1.7B", "GTX 1060 6 GB");
        assert_eq!(recommend_lm(&hw(16, 12)), "1.7B", "RAM begrenzt");
        if !cfg!(target_os = "macos") {
            assert_eq!(vae_chunk(&hw(32, 4)), Some("128"));
            assert_eq!(vae_chunk(&hw(32, 10)), Some("256"));
            assert_eq!(vae_chunk(&hw(32, 24)), None);
            assert_eq!(
                vae_chunk(&Hardware {
                    amd_or_intel: true,
                    ..hw(16, 0)
                }),
                Some("256")
            );
        }
    }

    #[test]
    fn parse_tolerates_comments_and_quotes() {
        let m = parse_manifest(
            "# x\nACESTEP_REV=\"abc\"\nHF_BASE=\"https://h/x\"\nMODEL_FILES=\"a.gguf:1:"
                .to_string()
                .as_str(),
        );
        assert_eq!(m.acestep_rev, "abc");
        assert!(m.models.is_empty());
        let sha = "a".repeat(64);
        let m = parse_manifest(&format!(
            "MODEL_FILES=\"vae.gguf:340:{sha}\nlm.gguf:7:{sha}\"\nENGINE_LINUX_REV=\"r\"\n"
        ));
        assert_eq!(m.models.len(), 2);
        assert!(m.engine_for("ENGINE_LINUX").is_none(), "URL/SHA fehlen");
    }

    #[test]
    fn unzip_strips_and_keeps_exec_bit() {
        let t = tempfile::tempdir().unwrap();
        let zip_path = t.path().join("e.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
            w.start_file("pkg-1/ace-server", opts).unwrap();
            w.write_all(b"#!/bin/sh\n").unwrap();
            // Link vor seinem Ziel, wie zip -y es auch schreiben kann
            w.add_symlink("pkg-1/libx.so.0", "libx.so.0.1", opts)
                .unwrap();
            w.start_file("pkg-1/libx.so.0.1", opts).unwrap();
            w.write_all(b"ELF-Bibliothek").unwrap();
            w.start_file("../evil", opts).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let out = t.path().join("out");
        unzip(&zip_path, &out).unwrap();
        assert!(!t.path().join("evil").exists(), "Pfad außerhalb des Ziels");
        let sub = single_subdir(&out).unwrap();
        assert!(sub.join("ace-server").is_file());
        assert_eq!(
            std::fs::read(sub.join("libx.so.0")).unwrap(),
            b"ELF-Bibliothek"
        );
        #[cfg(unix)]
        assert!(std::fs::symlink_metadata(sub.join("libx.so.0"))
            .unwrap()
            .file_type()
            .is_symlink());

        // Link, der aus dem Ordner zeigt: abgelehnt
        let bad = t.path().join("bad.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&bad).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.add_symlink("raus", "../../etc/passwd", opts).unwrap();
            w.finish().unwrap();
        }
        let e = unzip(&bad, &t.path().join("bad")).unwrap_err();
        assert!(e.contains("unzulässiger Link"), "{e}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(sub.join("ace-server"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
    }

    /// Mini-Server: liefert `body`, versteht `Range: bytes=N-` (206). Merkt sich die angefragten Bereiche.
    fn range_server(body: Vec<u8>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/model.gguf",
            l.local_addr().unwrap().port()
        );
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut range = String::new();
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        range = v.trim().trim_end_matches('-').to_string();
                    }
                }
                seen2.lock().unwrap().push(range.clone());
                let mut c = c;
                let from: usize = range.parse().unwrap_or(0);
                let head = if from > 0 {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\nConnection: close\r\n\r\n",
                        body.len() - from,
                        body.len() - 1,
                        body.len()
                    )
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                };
                let _ = c.write_all(head.as_bytes());
                let _ = c.write_all(&body[from..]);
            }
        });
        (url, seen)
    }

    fn sha_hex(b: &[u8]) -> String {
        Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    }

    #[test]
    fn download_resumes_and_verifies() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let (url, seen) = range_server(body.clone());
        let t = tempfile::tempdir().unwrap();
        let layout = Layout {
            code: t.path().into(),
            home: t.path().into(),
        };
        let manifest = Manifest::default();
        let cancel = AtomicBool::new(false);
        let reports = std::sync::Mutex::new(Vec::new());
        let report = |p: Progress| reports.lock().unwrap().push(p);
        let inst = Installer {
            layout: &layout,
            manifest: &manifest,
            cancel: &cancel,
            report: &report,
        };

        // abgebrochener Download: erste 50 000 Bytes liegen schon da
        let dest = t.path().join("model.gguf");
        std::fs::write(t.path().join("model.gguf.part"), &body[..50_000]).unwrap();
        inst.download(&url, &dest, &sha_hex(&body), Progress::step("setup_models"))
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!t.path().join("model.gguf.part").exists());
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["50000"],
            "nur der Rest wurde angefragt"
        );
        {
            let reports = reports.lock().unwrap();
            assert_eq!(reports.last().unwrap().key, "setup_verify");
            assert_eq!(reports.last().unwrap().total, 200_000);
        }

        // falsche Prüfsumme: Fehler, Teil-Datei gelöscht, Ziel nicht angelegt
        let dest2 = t.path().join("other.gguf");
        let e = inst
            .download(
                &url,
                &dest2,
                &"0".repeat(64),
                Progress::step("setup_models"),
            )
            .unwrap_err();
        assert_eq!(e.key, "setup_err_checksum");
        assert!(!dest2.exists() && !t.path().join("other.gguf.part").exists());

        // Abbrechen
        cancel.store(true, Ordering::SeqCst);
        let e = inst
            .download(
                &url,
                &t.path().join("c.gguf"),
                &sha_hex(&body),
                Progress::step("setup_models"),
            )
            .unwrap_err();
        assert_eq!(e.key, "setup_err_cancelled");
    }

    #[test]
    fn sha256_of_file() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("x");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
