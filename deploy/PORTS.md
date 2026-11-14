# Ports for the Rynuvo server

Open these in the VPS provider's firewall panel:

| Port | Protocol | Purpose |
|------|----------|---------|
| 3478 | UDP | STUN (connection test, P2P discovery) |
| 3479 | UDP | STUN second port (detects NAT mapping type) |
| 443  | TCP | HTTPS via Caddy (already open) |

The HTTP API listens only on `127.0.0.1:8110` and is reached through Caddy.

## Backup path for the Windows installer

If cross-building the Windows installer from macOS (`cargo-xwin`) fails, build it on a
GitHub Actions `windows-latest` runner with `pnpm tauri build` and upload the NSIS
`*-setup.exe` to `/var/lib/rynuvo/downloads/`.
