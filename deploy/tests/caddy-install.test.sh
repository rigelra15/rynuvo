#!/usr/bin/env bash
# Tests for deploy/caddy-install.sh using a fake `caddy` binary. Run: bash deploy/tests/caddy-install.test.sh
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
SCRIPT="$HERE/../caddy-install.sh"
fail() { echo "FAIL: $*" >&2; exit 1; }

setup() {
  T=$(mktemp -d)
  mkdir -p "$T/etc/sites" "$T/bin"
  printf 'other.example { respond "hi" }\n' > "$T/etc/Caddyfile"
  printf 'old site\n' > "$T/etc/sites/rynuvo.caddy"
  printf 'new site\n' > "$T/new.caddy"
  : > "$T/reloaded"
}

# 1. validation fails -> Caddyfile and old site file restored, no reload, nonzero exit
setup
printf '#!/bin/sh\nexit 1\n' > "$T/bin/caddy"; chmod +x "$T/bin/caddy"
if CADDY="$T/bin/caddy" RELOAD="echo yes >> $T/reloaded" bash "$SCRIPT" "$T/new.caddy" "$T/etc/Caddyfile" "$T/etc/sites/rynuvo.caddy" 2>/dev/null; then
  fail "should exit nonzero when validation fails"
fi
grep -qxF 'other.example { respond "hi" }' "$T/etc/Caddyfile" || fail "Caddyfile not restored"
! grep -q import "$T/etc/Caddyfile" || fail "import line left behind"
grep -qx 'old site' "$T/etc/sites/rynuvo.caddy" || fail "old site file not restored"
[ ! -s "$T/reloaded" ] || fail "reloaded despite failed validation"

# 2. validation passes -> import added once, site installed, reload once
setup
printf '#!/bin/sh\nexit 0\n' > "$T/bin/caddy"; chmod +x "$T/bin/caddy"
for _ in 1 2; do
  CADDY="$T/bin/caddy" RELOAD="echo yes >> $T/reloaded" bash "$SCRIPT" "$T/new.caddy" "$T/etc/Caddyfile" "$T/etc/sites/rynuvo.caddy"
done
[ "$(grep -c "import $T/etc/sites/rynuvo.caddy" "$T/etc/Caddyfile")" = 1 ] || fail "import line not added exactly once"
grep -qx 'new site' "$T/etc/sites/rynuvo.caddy" || fail "site not installed"
[ "$(wc -l < "$T/reloaded" | tr -d ' ')" = 2 ] || fail "expected a reload per successful run"

# 3. fresh host without sites dir works
setup
printf '#!/bin/sh\nexit 0\n' > "$T/bin/caddy"; chmod +x "$T/bin/caddy"
rm -rf "$T/etc/sites"
CADDY="$T/bin/caddy" RELOAD=true bash "$SCRIPT" "$T/new.caddy" "$T/etc/Caddyfile" "$T/etc/sites/rynuvo.caddy"
[ -f "$T/etc/sites/rynuvo.caddy" ] || fail "sites dir not created"
echo "caddy-install: all tests passed"
