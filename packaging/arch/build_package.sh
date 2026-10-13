#!/bin/bash
# Music Generator – Arch Linux Paket-Ersteller (makepkg Wrapper)
# Autor: ZeroDot1
# Baut ein natives Arch Linux Installationspaket (.pkg.tar.zst)

set -e

SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SOURCE_DIR/../.." && pwd)"
BUILD_DIR="$SOURCE_DIR"

echo "═══════════════════════════════════════════════════════════"
echo "  MUSIC GENERATOR · Arch Linux Paket-Bau (makepkg)"
echo "  Autor: ZeroDot1"
echo "═══════════════════════════════════════════════════════════"
echo ""

cd "$BUILD_DIR"

if ! command -v makepkg >/dev/null 2>&1; then
  echo "✗ makepkg nicht gefunden. Bitte 'base-devel' installieren:"
  echo "  sudo pacman -S base-devel"
  exit 1
fi

echo "▸ Bereite Quellcode-Archiv vor..."
rm -rf src pkg *.pkg.tar.zst 2>/dev/null || true

# Erstelle Tarball in einem separaten temporären Verzeichnis außerhalb des Quellbaums
TMP_DIR=$(mktemp -d)
TARBALL="$TMP_DIR/music-generator.tar.gz"

echo "▸ Packe Anwendungsdateien für makepkg..."
tar --exclude='.git' --exclude='data' --exclude='engine' --exclude='.venv' \
    --exclude='packaging' \
    -czf "$TARBALL" -C "$ROOT" .

mv "$TARBALL" "$BUILD_DIR/music-generator.tar.gz"
rm -rf "$TMP_DIR"

echo "▸ Führe makepkg aus..."
makepkg -f -C --nodeps

rm -f "$BUILD_DIR/music-generator.tar.gz" 2>/dev/null || true

PKG_FILE=$(ls -1 "$BUILD_DIR"/*.pkg.tar.zst 2>/dev/null | head -1 || true)

if [ -n "$PKG_FILE" ] && [ -f "$PKG_FILE" ]; then
  echo ""
  echo "═══════════════════════════════════════════════════════════"
  echo "  ✓ Paket erfolgreich erstellt!"
  echo "  Datei: $PKG_FILE"
  echo ""
  echo "  Installation mit pacman:"
  echo "    sudo pacman -U $PKG_FILE"
  echo "═══════════════════════════════════════════════════════════"
else
  echo "✗ Paket konnte nicht erstellt werden."
  exit 1
fi
