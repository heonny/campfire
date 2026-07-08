#!/usr/bin/env bash
#
# Build a macOS .app bundle for Campfire with a generated .icns icon.
#
# Feed it one square PNG (1024x1024 recommended) and it produces
# target/release/bundle/Campfire.app — icon shows in Dock, Finder, and Launchpad.
#
# Usage:
#   ./scripts/bundle-macos.sh                       # uses assets/images/logo-mac.png
#   ./scripts/bundle-macos.sh path/to/icon.png      # custom source icon
#   ./scripts/bundle-macos.sh --install             # also copy the .app into /Applications
#
# --install may be combined with a custom icon path, in any order.
#
# macOS only: relies on sips + iconutil + codesign, all shipped with the OS.

set -euo pipefail

APP_NAME="Campfire"
BIN_NAME="campfire"
BUNDLE_ID="com.heonny.campfire"

# Repo root = parent of this script's directory, regardless of where it's called from.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Parse args: --install is a flag; the first non-flag arg is the source icon.
INSTALL=0
SRC_ICON="assets/images/logo-mac.png"
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    *) SRC_ICON="$arg" ;;
  esac
done
APP_DIR="target/release/bundle/$APP_NAME.app"

# --- preflight ------------------------------------------------------------
[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only (needs sips/iconutil)" >&2; exit 1; }
for tool in sips iconutil codesign; do
  command -v "$tool" >/dev/null || { echo "error: '$tool' not found" >&2; exit 1; }
done
[[ -f "$SRC_ICON" ]] || { echo "error: source icon not found: $SRC_ICON" >&2; exit 1; }

VERSION="$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')"

# --- 1. build the release binary -----------------------------------------
echo "==> cargo build --release"
cargo build --release

# --- 2. generate AppIcon.icns from the source PNG ------------------------
echo "==> generating icon from $SRC_ICON"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
# iconutil expects each size at 1x and 2x (Retina). Largest is 512@2x = 1024px.
for size in 16 32 128 256 512; do
  sips -z "$size" "$size"               "$SRC_ICON" --out "$ICONSET/icon_${size}x${size}.png"    >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" "$SRC_ICON" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$TMP/AppIcon.icns"

# --- 3. assemble the .app bundle -----------------------------------------
echo "==> assembling $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "target/release/$BIN_NAME" "$APP_DIR/Contents/MacOS/$BIN_NAME"
chmod +x "$APP_DIR/Contents/MacOS/$BIN_NAME"
cp "$TMP/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"

cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>       <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>        <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleExecutable</key>        <string>$BIN_NAME</string>
    <key>CFBundleIconFile</key>          <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>    <string>10.15</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <key>LSApplicationCategoryType</key> <string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

# --- 4. ad-hoc sign (local use; not for distribution) --------------------
# Re-sign the hand-assembled bundle so Gatekeeper launches it cleanly on
# Apple Silicon. Distribution needs a real Developer ID + notarization.
echo "==> ad-hoc signing"
codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 \
  || echo "warn: ad-hoc codesign failed (bundle still usable locally)" >&2

# --- 5. optional install into /Applications ------------------------------
# Replaces any existing install so the copy in /Applications tracks this build.
if [[ "$INSTALL" == 1 ]]; then
  DEST="/Applications/$APP_NAME.app"
  echo "==> installing to $DEST"
  rm -rf "$DEST"
  cp -R "$APP_DIR" "$DEST"
fi

echo "==> done: $APP_DIR"
