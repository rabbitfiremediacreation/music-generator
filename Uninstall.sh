#!/bin/bash
# Music Generator – Deinstallation unter Linux (Arch Linux / Derivate).
# Autor: ZeroDot1
# Entfernt Anwendungsdateien, Engine, venv und Konfiguration.
# Systempakete in Arch Linux bleiben unberührt.
SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SOURCE_DIR/arch_linux.sh" --uninstall "$@"
