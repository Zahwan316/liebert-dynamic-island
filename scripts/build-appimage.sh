#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="Liebert Dynamic Island"
BIN_NAME="liebert-dynamic-island"
VERSION="0.2.0"
ARCH="x86_64"
APPDIR="$ROOT_DIR/target/appimage/AppDir"
RUNTIME_PATH="$ROOT_DIR/target/appimage/runtime-$ARCH"
OUT_PATH="$ROOT_DIR/dist/Liebert_Dynamic_Island-${VERSION}-${ARCH}.AppImage"
RUNTIME_URL="https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-${ARCH}"

if [[ "${1:-}" != "--skip-cargo" ]]; then
    cargo build --release
fi

BIN_PATH="$ROOT_DIR/target/release/$BIN_NAME"
if [[ ! -x "$BIN_PATH" ]]; then
    echo "Error: release binary not found at $BIN_PATH" >&2
    exit 1
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$ROOT_DIR/dist" "$ROOT_DIR/target/appimage"
cp "$BIN_PATH" "$APPDIR/usr/bin/$BIN_NAME"
chmod 755 "$APPDIR/usr/bin/$BIN_NAME"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/liebert-dynamic-island" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

cp "$ROOT_DIR/assets/${BIN_NAME}.desktop" "$APPDIR/${BIN_NAME}.desktop"
cp "$ROOT_DIR/assets/${BIN_NAME}.png" "$APPDIR/${BIN_NAME}.png"
cp "$APPDIR/${BIN_NAME}.png" "$APPDIR/.DirIcon"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$APPDIR/${BIN_NAME}.desktop"
fi

if command -v appimagetool >/dev/null 2>&1; then
    ARCH="$ARCH" appimagetool "$APPDIR" "$OUT_PATH"
elif command -v mksquashfs >/dev/null 2>&1; then
    if [[ ! -f "$RUNTIME_PATH" ]]; then
        echo "Downloading AppImage type2-runtime..."
        curl -fL --retry 3 -o "$RUNTIME_PATH" "$RUNTIME_URL"
    fi
    SQFS_PATH="$ROOT_DIR/target/appimage/app.squashfs"
    rm -f "$SQFS_PATH"
    mksquashfs "$APPDIR" "$SQFS_PATH" -root-owned -noappend -comp zstd -Xcompression-level 19 -quiet
    cat "$RUNTIME_PATH" "$SQFS_PATH" > "$OUT_PATH"
    chmod 755 "$OUT_PATH"
else
    echo "Error: neither appimagetool nor mksquashfs found." >&2
    exit 1
fi

echo "AppImage built: $OUT_PATH"
