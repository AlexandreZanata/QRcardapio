# Operations Runbook

## Local Development

```bash
cp .env.example .env
./scripts/dev.sh
```

System packages for local OCR & PDF menu pre-registration (pre-installed in `Dockerfile`):
```bash
sudo apt-get install -y tesseract-ocr tesseract-ocr-por tesseract-ocr-eng tesseract-ocr-spa poppler-utils
```

Useful CLI commands:
```bash
qrcardapio serve
qrcardapio db migrate
qrcardapio admin create --email ops@qrcardapio.com.br --password "<strong-password>"
qrcardapio admin reset-token --email owner@restaurant.com.br
qrcardapio seed
qrcardapio republish-all
```

## Production Deployment (Single 1-vCPU / 1-GB VPS)

1. Build or deploy on the target host with Docker Compose:
   ```bash
   DOMAIN=qrcardapio.com.br PUBLIC_BASE_URL=https://qrcardapio.com.br docker compose up -d --build
   ```
2. Bootstrap the initial operational admin account inside the container:
   ```bash
   docker compose exec app qrcardapio admin create --email admin@qrcardapio.com.br --password "<strong-password>"
   ```
3. Verify readiness:
   ```bash
   curl -fsS http://127.0.0.1/healthz
   curl -fsS http://127.0.0.1/readyz
   ```

## Online Backups (`scripts/backup.sh`)

Runs `qrcardapio db backup` (`VACUUM INTO` + read-only `PRAGMA integrity_check`), archives `DATA_DIR/media`, writes `SHA256SUMS`, and prunes snapshots older than `RETENTION_DAYS` (default 14 days). Safe while the application is running.

```bash
DATA_DIR=/var/lib/qrcardapio QRCARDAPIO_BIN=/usr/local/bin/qrcardapio ./scripts/backup.sh /var/backups/qrcardapio
```

Schedule via cron or systemd timer (recommended daily off-peak + off-site sync of the timestamped backup directory).

## Verified Restore (`scripts/restore.sh`)

**Always stop the `qrcardapio` service first** before replacing the SQLite file:

```bash
docker compose stop app
DATA_DIR=/var/lib/qrcardapio ./scripts/restore.sh /var/backups/qrcardapio/20261123T120000Z
docker compose start app
```

`restore.sh` validates `SHA256SUMS`, removes stale `-wal`/`-shm` sidecars, verifies `PRAGMA integrity_check`, restores `media/`, and executes `qrcardapio republish-all` to regenerate static public snapshots cleanly.

## Logging & Incident Recovery

- Production sets `LOG_FORMAT=json` (`stdout`). Docker's `json-file` log driver or `journald` should rotate logs (`max-size: 20m, max-file: 5`).
- If a snapshot file becomes corrupted or missing on disk without touching SQLite, run `qrcardapio republish-all` to rebuild all published static menus cleanly.
- Never copy an active `qrcardapio.sqlite` without `VACUUM INTO` or stopping the process; never mount `DATA_DIR/db` over NFS.
