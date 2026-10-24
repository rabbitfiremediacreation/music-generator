#!/bin/bash
# Baut acestep.cpp (Version ACESTEP_REV aus common.sh) für Linux x86_64 mit Vulkan als portables Paket:
# ace-server + Backends als eigene Bibliotheken (CPU-Varianten werden zur Laufzeit gewählt), Suchpfad $ORIGIN.
# Genutzt von .github/workflows/engine-linux.yml und zum lokalen Bauen.
#
# Aufruf: scripts/build_engine_linux.sh <arbeitsordner> [zielordner]
#   Ergebnis: <arbeitsordner>/acestep-cpp-linux-x64-<rev7>.zip (+ .sha256)
#   Mit zielordner werden die Dateien zusätzlich dorthin kopiert (z. B. ~/MusicGenerator/engine/acestep.cpp/build).
# Braucht: git, cmake, C++-Compiler, Vulkan-Header und glslc.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mkdir -p "$1" && cd "$1" && pwd)"
TARGET="${2:-}"
ACESTEP_REPO=$(sed -n 's/^ACESTEP_REPO="\(.*\)"/\1/p' "$ROOT/scripts/common.sh")
ACESTEP_REV=$(sed -n 's/^ACESTEP_REV="\(.*\)"/\1/p' "$ROOT/scripts/common.sh")
REV7="${ACESTEP_REV:0:7}"
SRC="$WORK/src"
NAME="acestep-cpp-linux-x64-$REV7"

if [ ! -d "$SRC/.git" ]; then
  git clone --quiet "$ACESTEP_REPO" "$SRC"
fi
git -C "$SRC" cat-file -e "$ACESTEP_REV^{commit}" 2>/dev/null || git -C "$SRC" fetch --quiet origin
git -C "$SRC" checkout --quiet "$ACESTEP_REV"
git -C "$SRC" submodule update --init --recursive --quiet

cmake -S "$SRC" -B "$WORK/build" -DCMAKE_BUILD_TYPE=Release \
  -DGGML_VULKAN=ON -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_NATIVE=OFF \
  -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON -DCMAKE_INSTALL_RPATH='$ORIGIN'
cmake --build "$WORK/build" --config Release -j "$(nproc)"

PKG="$WORK/$NAME"
rm -rf "$PKG" && mkdir -p "$PKG"
cp "$WORK/build/ace-server" "$PKG/"
for f in ace-lm ace-synth ace-understand quantize mp3-codec neural-codec; do
  [ -f "$WORK/build/$f" ] && cp "$WORK/build/$f" "$PKG/"
done
find "$WORK/build" -maxdepth 1 -name '*.so*' -exec cp -P {} "$PKG/" \;
cp "$SRC/LICENSE" "$PKG/LICENSE-acestep.cpp.txt"
{
  echo "ENGINE_REV=$REV7"
  echo "ACESTEP_REV=$ACESTEP_REV"
  echo "BUILD=Vulkan + CPU (GGML_BACKEND_DL, GGML_CPU_ALL_VARIANTS)"
  echo "BUILT_ON=$(. /etc/os-release && echo "$PRETTY_NAME"), $(ldd --version 2>&1 | sed -n 1p)"
} > "$PKG/BUILDINFO.txt"

# Rauchtest: startet die Programmdatei und meldet sie die erwartete Version?
# (erst ganz lesen: mit pipefail zählt ein abgebrochenes "| head" als Fehler)
HELP=$("$PKG/ace-server" --help 2>&1 || true)
echo "$HELP" | sed -n 1p
grep -q "$REV7" <<< "$HELP" || { echo "ace-server meldet nicht die Version $REV7" >&2; exit 1; }

(cd "$WORK" && rm -f "$NAME.zip" && zip -qry "$NAME.zip" "$NAME")
sha256sum "$WORK/$NAME.zip" | cut -d' ' -f1 > "$WORK/$NAME.zip.sha256"
echo "Paket: $WORK/$NAME.zip"
echo "SHA-256: $(cat "$WORK/$NAME.zip.sha256")"

if [ -n "$TARGET" ]; then
  mkdir -p "$TARGET"
  cp -a "$PKG/." "$TARGET/"
  echo "Kopiert nach $TARGET"
fi
