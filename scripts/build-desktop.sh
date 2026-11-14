#!/usr/bin/env bash
# Build the macOS dmg and Windows NSIS installer, then publish them to the server's /downloads.
set -euo pipefail
HOST=${HOST:-hypertopia-vps}
cd "$(dirname "$0")/../apps/desktop"
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
OUT=../../target/installers
mkdir -p "$OUT"

pnpm tauri build --target aarch64-apple-darwin --bundles dmg
cp ../../target/aarch64-apple-darwin/release/bundle/dmg/*.dmg "$OUT/Rynuvo-$VERSION-mac.dmg"

# libwebp's SSE4.1 sources need the flag under clang-cl (MSVC doesn't); every x86-64 CPU since ~2008 has it.
# cc-rs reads the hyphenated variable before the underscored one cargo-xwin sets.
WIN=../../target/x86_64-pc-windows-msvc/release
if ! env "CFLAGS_x86_64-pc-windows-msvc=-msse4.1" pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis; then
  # Homebrew makensis crashes (std::bad_alloc) on macOS 27 beta. Tauri has already written the
  # NSIS script, so package it with Ubuntu's makensis on the VPS instead.
  [ -f "$WIN/nsis/x64/installer.nsi" ] || { echo "Windows build failed before packaging" >&2; exit 1; }
  R=/tmp/rynuvo-nsis
  REPO=$(cd ../.. && pwd)
  CACHE="$HOME/Library/Caches"
  ssh "$HOST" "command -v makensis >/dev/null || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nsis >/dev/null; rm -rf $R; mkdir -p $R/repo/target/x86_64-pc-windows-msvc/release/nsis $R/cache/tauri/NSIS/Plugins/x86-unicode"
  scp -rq "$WIN/nsis/x64" "$HOST:$R/repo/target/x86_64-pc-windows-msvc/release/nsis/"
  scp -q "$WIN/rynuvo-desktop.exe" "$HOST:$R/repo/target/x86_64-pc-windows-msvc/release/"
  scp -rq "$CACHE/tauri/NSIS/Plugins/x86-unicode/additional" "$HOST:$R/cache/tauri/NSIS/Plugins/x86-unicode/"
  ssh "$HOST" "cd $R/repo/target/x86_64-pc-windows-msvc/release/nsis/x64 && sed -i 's|$REPO|$R/repo|g; s|$CACHE|$R/cache|g' *.nsi *.nsh && makensis -V2 -INPUTCHARSET UTF8 installer.nsi"
  mkdir -p "$WIN/bundle/nsis"
  scp -q "$HOST:$R/repo/target/x86_64-pc-windows-msvc/release/nsis/x64/nsis-output.exe" "$WIN/bundle/nsis/Rynuvo_${VERSION}_x64-setup.exe"
  ssh "$HOST" "rm -rf $R"
fi
cp "$WIN"/bundle/nsis/*-setup.exe "$OUT/Rynuvo-$VERSION-windows-setup.exe"

ls -lh "$OUT"
scp -q "$OUT/Rynuvo-$VERSION-mac.dmg" "$OUT/Rynuvo-$VERSION-windows-setup.exe" "$HOST":/tmp/
ssh "$HOST" "sudo install -o rynuvo -g rynuvo -m 0644 /tmp/Rynuvo-$VERSION-* /var/lib/rynuvo/downloads/ && rm -f /tmp/Rynuvo-$VERSION-*"
echo "https://rynuvo.pokyvl.my.id/downloads/Rynuvo-$VERSION-mac.dmg"
echo "https://rynuvo.pokyvl.my.id/downloads/Rynuvo-$VERSION-windows-setup.exe"
