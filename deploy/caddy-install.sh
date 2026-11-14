#!/usr/bin/env bash
# Install the Rynuvo Caddy site and import it from the main Caddyfile, without ever leaving a
# broken config behind: if validation fails, both files are restored and Caddy is not reloaded.
# Usage: caddy-install.sh NEW_SITE_FILE CADDYFILE SITE_PATH   (env: CADDY, RELOAD)
set -euo pipefail
NEW=$1 CADDYFILE=$2 SITE=$3
CADDY=${CADDY:-caddy}
RELOAD=${RELOAD:-systemctl reload caddy}

BAK=$(mktemp -d)
cp -p "$CADDYFILE" "$BAK/Caddyfile"
HAD_SITE=0
if [ -f "$SITE" ]; then cp -p "$SITE" "$BAK/site"; HAD_SITE=1; fi

restore() {
  cp -p "$BAK/Caddyfile" "$CADDYFILE"
  if [ "$HAD_SITE" = 1 ]; then cp -p "$BAK/site" "$SITE"; else rm -f "$SITE"; fi
  echo "Caddy config did not validate; restored the previous files and did not reload" >&2
}

install -d -m 0755 "$(dirname "$SITE")"
install -m 0644 "$NEW" "$SITE"
LINE="import $SITE"
grep -qxF "$LINE" "$CADDYFILE" || echo "$LINE" >> "$CADDYFILE"

if ! "$CADDY" validate --config "$CADDYFILE" --adapter caddyfile; then
  restore
  rm -rf "$BAK"
  exit 1
fi
rm -rf "$BAK"
eval "$RELOAD"
