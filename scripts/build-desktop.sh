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

pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
cp ../../target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe "$OUT/Rynuvo-$VERSION-windows-setup.exe"

ls -lh "$OUT"
scp -q "$OUT/Rynuvo-$VERSION-mac.dmg" "$OUT/Rynuvo-$VERSION-windows-setup.exe" "$HOST":/tmp/
ssh "$HOST" "sudo install -o rynuvo -g rynuvo -m 0644 /tmp/Rynuvo-$VERSION-* /var/lib/rynuvo/downloads/ && rm -f /tmp/Rynuvo-$VERSION-*"
echo "https://rynuvo.pokyvl.my.id/downloads/Rynuvo-$VERSION-mac.dmg"
echo "https://rynuvo.pokyvl.my.id/downloads/Rynuvo-$VERSION-windows-setup.exe"
