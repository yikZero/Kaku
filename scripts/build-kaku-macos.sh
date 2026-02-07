#!/usr/bin/env bash
set -euo pipefail

if [[ "${OSTYPE:-}" != darwin* ]]; then
	echo "This script is macOS-only." >&2
	exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

APP_NAME="Kaku"
TARGET_DIR="${TARGET_DIR:-target}"
PROFILE="${PROFILE:-release}"
OUT_DIR="${OUT_DIR:-dist}"
OPEN_APP="${OPEN_APP:-0}"

if [[ "${1:-}" == "--open" ]]; then
	OPEN_APP=1
fi

APP_BUNDLE_SRC="assets/macos/Kaku.app"
APP_BUNDLE_OUT="$OUT_DIR/$APP_NAME.app"

echo "[1/4] Building binaries ($PROFILE)..."
if [[ "$PROFILE" == "release" ]]; then
	cargo build --release -p wezterm-gui -p wezterm
	BIN_DIR="$TARGET_DIR/release"
else
	cargo build -p wezterm-gui -p wezterm
	BIN_DIR="$TARGET_DIR/debug"
fi

echo "[2/4] Preparing app bundle..."
rm -rf "$APP_BUNDLE_OUT"
mkdir -p "$OUT_DIR"
cp -R "$APP_BUNDLE_SRC" "$APP_BUNDLE_OUT"

mkdir -p "$APP_BUNDLE_OUT/Contents/MacOS"
mkdir -p "$APP_BUNDLE_OUT/Contents/Resources"

echo "[3/4] Copying resources and binaries..."
cp -R assets/shell-integration/* "$APP_BUNDLE_OUT/Contents/Resources/"
cp -R assets/shell-completion "$APP_BUNDLE_OUT/Contents/Resources/"
cp -R assets/fonts "$APP_BUNDLE_OUT/Contents/Resources/"
tic -xe wezterm -o "$APP_BUNDLE_OUT/Contents/Resources/terminfo" termwiz/data/wezterm.terminfo

for bin in wezterm wezterm-gui; do
	cp "$BIN_DIR/$bin" "$APP_BUNDLE_OUT/Contents/MacOS/$bin"
	chmod +x "$APP_BUNDLE_OUT/Contents/MacOS/$bin"
done

echo "[4/4] Done: $APP_BUNDLE_OUT"
if [[ "$OPEN_APP" == "1" ]]; then
	echo "Opening app..."
	open "$APP_BUNDLE_OUT"
fi
