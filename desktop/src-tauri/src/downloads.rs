//! Downloads aus der Web-App (Song als MP3/WAV, Ordner als ZIP, Lyrics als TXT). Die Webviews der drei Plattformen
//! laden unterschiedlich (oder gar nicht) herunter, deshalb holt die Hülle die Datei selbst vom lokalen Server und
//! legt sie im Downloads-Ordner ab.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Zielordner: Downloads des Benutzers, sonst der persönliche Ordner.
pub fn download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Dateiname aus `Content-Disposition` (`filename*=utf-8''…` hat Vorrang vor `filename="…"`).
pub fn disposition_name(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';').map(str::trim) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        match k.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                let v = v.trim().trim_matches('"');
                let encoded = v.split_once("''").map(|(_, e)| e).unwrap_or(v);
                return Some(String::from_utf8_lossy(&percent_decode(encoded)).into_owned());
            }
            "filename" => plain = Some(v.trim().trim_matches('"').to_string()),
            _ => {}
        }
    }
    plain
}

pub fn percent_decode(s: &str) -> Vec<u8> {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Nur ein Dateiname, ohne Pfadteile und Zeichen, die Windows nicht erlaubt.
pub fn sanitize(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_matches('.').trim().to_string();
    if clean.is_empty() {
        "download".to_string()
    } else {
        clean
    }
}

/// `name`, `name (1)`, `name (2)` … – vorhandene Dateien werden nie überschrieben.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .expect("unendliche Folge")
}

/// Lädt `path_query` (z. B. `/api/download?folder=x`) von der lokalen Web-App und speichert die Datei in `dir`.
pub fn fetch(port: u16, path_query: &str, dir: &Path, fallback: &str) -> Result<PathBuf, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut s =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    // ZIP-Erstellung großer Ordner dauert, daher großzügig
    s.set_read_timeout(Some(Duration::from_secs(300)))
        .map_err(|e| e.to_string())?;
    write!(
        s,
        "GET {path_query} HTTP/1.0\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| e.to_string())?;

    let mut r = BufReader::new(s);
    let mut status_line = String::new();
    r.read_line(&mut status_line).map_err(|e| e.to_string())?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let mut name = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).map_err(|e| e.to_string())? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-disposition") {
                name = disposition_name(v.trim());
            }
        }
    }
    if status != 200 {
        let mut body = String::new();
        let _ = r.take(500).read_to_string(&mut body);
        return Err(format!("HTTP {status} {}", body.trim()));
    }
    let name = sanitize(name.as_deref().unwrap_or(fallback));
    save(dir, &name, |f| std::io::copy(&mut r, f).map(|_| ()))
}

/// `data:`-Adresse (Lyrics als TXT): `data:<typ>[;charset=…],<prozentkodierte Daten>`.
pub fn save_data_url(url: &str, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let rest = url.strip_prefix("data:").ok_or("keine data:-Adresse")?;
    let (meta, data) = rest.split_once(',').ok_or("data:-Adresse ohne Daten")?;
    if meta.split(';').any(|m| m.eq_ignore_ascii_case("base64")) {
        return Err("base64 wird nicht unterstützt".into());
    }
    let bytes = percent_decode(data);
    save(dir, &sanitize(name), |f| f.write_all(&bytes))
}

/// Erst als `.part` schreiben, dann umbenennen: abgebrochene Downloads hinterlassen keine halben Dateien.
fn save(
    dir: &Path,
    name: &str,
    write: impl FnOnce(&mut File) -> std::io::Result<()>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let dest = unique_path(dir, name);
    let part = dest.with_file_name(format!(
        "{}.part",
        dest.file_name().unwrap().to_string_lossy()
    ));
    let result = File::create(&part).and_then(|mut f| {
        write(&mut f)?;
        f.flush()
    });
    if let Err(e) = result.and_then(|_| std::fs::rename(&part, &dest)) {
        let _ = std::fs::remove_file(&part);
        return Err(e.to_string());
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn disposition_variants() {
        assert_eq!(
            disposition_name(r#"attachment; filename="Night_Drive_42.mp3""#).as_deref(),
            Some("Night_Drive_42.mp3")
        );
        assert_eq!(
            disposition_name("attachment; filename*=utf-8''M%C3%BCde%20Lieder.zip").as_deref(),
            Some("Müde Lieder.zip")
        );
        assert_eq!(disposition_name("inline"), None);
    }

    #[test]
    fn sanitize_and_unique() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize(r"C:\x\a:b?.txt"), "a_b_.txt");
        assert_eq!(sanitize(" .. "), "download");
        let t = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(t.path(), "a.mp3"), t.path().join("a.mp3"));
        std::fs::write(t.path().join("a.mp3"), b"").unwrap();
        std::fs::write(t.path().join("a (1).mp3"), b"").unwrap();
        assert_eq!(unique_path(t.path(), "a.mp3"), t.path().join("a (2).mp3"));
        std::fs::write(t.path().join("README"), b"").unwrap();
        assert_eq!(unique_path(t.path(), "README"), t.path().join("README (1)"));
    }

    #[test]
    fn data_url_lyrics() {
        let t = tempfile::tempdir().unwrap();
        let p = save_data_url(
            "data:text/plain;charset=utf-8,Zeile%201%0Af%C3%BCr%20dich",
            t.path(),
            "Song.txt",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(p).unwrap(), "Zeile 1\nfür dich");
        assert!(save_data_url("data:text/plain;base64,QQ==", t.path(), "x.txt").is_err());
        assert_eq!(percent_decode("100%"), b"100%");
        assert_eq!(percent_decode("%zz%ü"), "%zz%ü".as_bytes());
    }

    fn serve_once(response: &'static [u8]) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            // ganze Anfrage lesen, sonst setzt das Schließen die Verbindung zurück (RST) und die Antwort geht verloren
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match c.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            c.write_all(response).unwrap();
            let _ = c.shutdown(std::net::Shutdown::Write);
        });
        port
    }

    #[test]
    fn fetch_saves_binary_body() {
        let port = serve_once(
            b"HTTP/1.1 200 OK\r\ncontent-type: audio/mpeg\r\ncontent-disposition: attachment; filename=\"Song_7.mp3\"\r\n\r\n\x00\xffID3",
        );
        let t = tempfile::tempdir().unwrap();
        let p = fetch(port, "/api/songs/1/audio?download=1", t.path(), "x").unwrap();
        assert_eq!(p, t.path().join("Song_7.mp3"));
        assert_eq!(std::fs::read(p).unwrap(), b"\x00\xffID3");
    }

    #[test]
    fn fetch_reports_http_errors_and_leaves_nothing() {
        let port = serve_once(b"HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\n\r\n{\"detail\":\"Not Found\"}");
        let t = tempfile::tempdir().unwrap();
        let e = fetch(port, "/api/songs/x/audio?download=1", t.path(), "x").unwrap_err();
        assert!(e.starts_with("HTTP 404"), "{e}");
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
    }
}
