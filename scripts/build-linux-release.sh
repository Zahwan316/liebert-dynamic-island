#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN_NAME="liebert-dynamic-island"
STAGE_NAME="${BIN_NAME}-linux-x86_64"
STAGE_DIR="$ROOT_DIR/dist/staging/$STAGE_NAME"

if [[ "${1:-}" != "--skip-cargo" ]]; then
    cargo build --release --locked
fi

./scripts/build-appimage.sh --skip-cargo

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp "target/release/$BIN_NAME" "$STAGE_DIR/$BIN_NAME"
cp LICENSE.md README.md "$STAGE_DIR/"
cp "assets/$BIN_NAME.desktop" "assets/$BIN_NAME.png" "$STAGE_DIR/"
chmod 755 "$STAGE_DIR/$BIN_NAME"

tar -C "$ROOT_DIR/dist/staging" -czf "$ROOT_DIR/dist/$STAGE_NAME.tar.gz" "$STAGE_NAME"
echo "Linux archive built: $ROOT_DIR/dist/$STAGE_NAME.tar.gz"
