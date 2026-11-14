#!/usr/bin/env bash
# Build the Linux server, install it on the VPS, and (first run) set up user, config, systemd and Caddy.
set -euo pipefail
HOST=${HOST:-hypertopia-vps}
DOMAIN=rynuvo.pokyvl.my.id
cd "$(dirname "$0")/.."

cargo zigbuild -p rynuvo-server --release --target x86_64-unknown-linux-gnu.2.39
BIN=target/x86_64-unknown-linux-gnu/release/rynuvo-server
ls -lh "$BIN"

# Stage in a private temp dir on the remote (the VPS is shared; fixed /tmp names could be pre-created).
STAGE=$(ssh "$HOST" 'mktemp -d')
scp -q "$BIN" deploy/rynuvo.service deploy/Caddyfile.rynuvo deploy/config.example.toml deploy/caddy-install.sh "$HOST:$STAGE/"
ssh "$HOST" "sudo STAGE=$STAGE bash -s" <<'REMOTE'
set -euo pipefail
trap 'rm -rf "$STAGE"' EXIT
id rynuvo >/dev/null 2>&1 || useradd --system --home /var/lib/rynuvo --shell /usr/sbin/nologin rynuvo
install -m 0755 "$STAGE/rynuvo-server" /usr/local/bin/rynuvo-server
install -d -m 0755 /etc/rynuvo
[ -f /etc/rynuvo/config.toml ] || install -m 0644 "$STAGE/config.example.toml" /etc/rynuvo/config.toml
install -d -o rynuvo -g rynuvo -m 0755 /var/lib/rynuvo /var/lib/rynuvo/downloads
install -m 0644 "$STAGE/rynuvo.service" /etc/systemd/system/rynuvo.service
systemctl daemon-reload
systemctl enable --now rynuvo
systemctl restart rynuvo

bash "$STAGE/caddy-install.sh" "$STAGE/Caddyfile.rynuvo" /etc/caddy/Caddyfile /etc/caddy/sites/rynuvo.caddy
sleep 1
systemctl is-active rynuvo
ps -o rss=,comm= -C rynuvo-server
REMOTE

for i in 1 2 3 4 5; do
  curl -fsS "https://$DOMAIN/api/v1/health" && echo && exit 0
  sleep 3
done
echo "health check over HTTPS failed" >&2
exit 1
