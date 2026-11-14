#!/usr/bin/env bash
# Publish a GitHub release to the VPS: installers, updater bundles and latest.json (what installed
# apps poll for updates). Usage: deploy/publish-release.sh 0.3.0 ["release notes"]
set -euo pipefail
V=${1:?version, e.g. 0.3.0}
NOTES=${2:-"Rynuvo $V"}
HOST=${HOST:-hypertopia-vps}
BASE=https://rynuvo.pokyvl.my.id/downloads
cd "$(dirname "$0")/.."

DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT
gh release download "v$V" -D "$DIR" -p "Rynuvo-$V-*"
ls -lh "$DIR"
for f in mac.dmg mac.app.tar.gz mac.app.tar.gz.sig windows-setup.exe windows-setup.exe.sig; do
  [ -f "$DIR/Rynuvo-$V-$f" ] || { echo "missing Rynuvo-$V-$f in the release" >&2; exit 1; }
done

V="$V" NOTES="$NOTES" BASE="$BASE" DIR="$DIR" python3 - <<'PY'
import json, os, datetime
v, d, base = os.environ["V"], os.environ["DIR"], os.environ["BASE"]
sig = lambda f: open(os.path.join(d, f)).read().strip()
doc = {
    "version": v,
    "notes": os.environ["NOTES"],
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {
        "darwin-aarch64": {"url": f"{base}/Rynuvo-{v}-mac.app.tar.gz", "signature": sig(f"Rynuvo-{v}-mac.app.tar.gz.sig")},
        "windows-x86_64": {"url": f"{base}/Rynuvo-{v}-windows-setup.exe", "signature": sig(f"Rynuvo-{v}-windows-setup.exe.sig")},
    },
}
json.dump(doc, open(os.path.join(d, "latest.json"), "w"), indent=2)
PY
cp "$DIR/Rynuvo-$V-mac.dmg" "$DIR/Rynuvo-latest-mac.dmg"
cp "$DIR/Rynuvo-$V-windows-setup.exe" "$DIR/Rynuvo-latest-windows-setup.exe"

# latest.json goes last so no app sees it before the files it points to exist.
STAGE=$(ssh "$HOST" 'mktemp -d')
scp -q "$DIR"/Rynuvo-* "$DIR/latest.json" "$HOST:$STAGE/"
ssh "$HOST" "STAGE=$STAGE bash -s" <<'REMOTE'
set -euo pipefail
trap 'rm -rf "$STAGE"' EXIT
cd "$STAGE"
for f in Rynuvo-*; do sudo -n install -o rynuvo -g rynuvo -m 644 "$f" /var/lib/rynuvo/downloads/; done
sudo -n install -o rynuvo -g rynuvo -m 644 latest.json /var/lib/rynuvo/downloads/
ls -l /var/lib/rynuvo/downloads
REMOTE
curl -fsS "$BASE/latest.json" | head -c 300; echo
