## Music Generator Desktop – Alpha

Desktop-App (Tauri) für den lokalen Music Generator (ACE-Step 1.5 über acestep.cpp): ein Fenster statt Installer-Skript,
„ON/OFF“ und Browser. Beim ersten Start richtet die App alles selbst ein: Modellserver, Modelle (4–11 GB, mit Fortsetzen
und SHA-256-Prüfung), Python-Umgebung und Grafiktest.

**Neu in 0.1.3:**
- Auf dem aktuellen Stand des Originals: Export wahlweise als WAV oder MP3 (Zahnrad › Einstellungen), neue
  Standardwerte (Iterationen 80, Varianz 1.2).
- MP3-Export funktioniert in der Desktop-App (der MP3-Wandler wird im Datenordner gesucht).
- Längere Wartezeit beim ersten Start (2 Minuten), „Cache leeren“ löscht auch den MP3-Zwischenspeicher.

**Neu in 0.1.2:**
- „Idee aufnehmen“ funktioniert unter Linux: Mikrofon-Zugriff in WebKitGTK eingeschaltet, Aufnahme als WAV statt über
  den MediaRecorder von WebKitGTK (lieferte leere Aufnahmen).
- AppImage startet (0.1.0: „Failed to import encodings module“, weil der AppImage-Starter `PYTHONHOME` setzt) und bringt
  GStreamer für Wiedergabe und Aufnahme mit.

> ⚠️ **Alpha – nur unter Linux getestet** (Linux Mint 22.3, NVIDIA Quadro T2000 über Vulkan).
> **Windows (MSI) und macOS (DMG) sind gebaut, aber nicht getestet.** Rückmeldungen sind sehr willkommen.

| System | Datei | Hinweis |
|---|---|---|
| Linux x86_64 | `.AppImage`, `.deb`, `.rpm` | Vulkan (NVIDIA, AMD, Intel) |
| Windows x64 | `.msi` (Deutsch / Englisch) | ungetestet; Installation systemweit; SmartScreen: „Weitere Informationen › Trotzdem ausführen“ |
| macOS Apple Silicon | `.dmg` | ungetestet; ab macOS 26 (fertiger Modellserver); unsigniert: Rechtsklick › Öffnen |

- Daten liegen in `~/MusicGenerator` (wie bei den Skripten), eine vorhandene Installation wird genutzt.
- Speicher & Modelle verwalten: Zahnrad › „Speicher & Modelle“ oder Tray-Menü.
- Prüfsummen: `SHA256SUMS.txt`.

---

**English:** Alpha desktop app (Tauri). **Only tested on Linux.** The Windows (MSI) and macOS (DMG) packages are built
but **untested**. Unsigned packages. On first start the app downloads the model server and models (4–11 GB) and sets
everything up; data lives in `~/MusicGenerator`.
