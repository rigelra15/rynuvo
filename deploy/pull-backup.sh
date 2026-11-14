#!/usr/bin/env bash
# Pull the server's daily database snapshots and the image folder to this Mac.
# Usage: deploy/pull-backup.sh [destination]   (default ~/RynuvoBackups)
set -euo pipefail
HOST=${HOST:-hypertopia-vps}
DEST=${1:-$HOME/RynuvoBackups}
mkdir -p "$DEST/db" "$DEST/media"
rsync -az --info=stats1 "$HOST:/var/lib/rynuvo/backups/" "$DEST/db/"
rsync -az --info=stats1 "$HOST:/var/lib/rynuvo/media/" "$DEST/media/"
echo "Backups in $DEST: $(ls "$DEST/db" | wc -l | tr -d ' ') database snapshots, $(ls "$DEST/media" | wc -l | tr -d ' ') images."
