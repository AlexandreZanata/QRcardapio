#!/usr/bin/env bash
set -euo pipefail

# Consistent online backup for SQLite (via VACUUM INTO) + media assets + SHA-256 manifest.
# Usage: ./scripts/backup.sh [output_dir]

DATA_DIR="${DATA_DIR:-./data}"
BACKUP_ROOT="${1:-./backups}"
BIN="${QRCARDAPIO_BIN:-qrcardapio}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
TARGET_DIR="${BACKUP_ROOT}/${STAMP}"
mkdir -p "${TARGET_DIR}"

echo "[backup] Creating consistent SQLite snapshot via VACUUM INTO..."
DATA_DIR="${DATA_DIR}" "${BIN}" db backup --output "${TARGET_DIR}/qrcardapio.sqlite"

if [ -d "${DATA_DIR}/media" ]; then
  echo "[backup] Archiving media directory..."
  tar -C "${DATA_DIR}" -czf "${TARGET_DIR}/media.tar.gz" media
fi

echo "[backup] Computing SHA-256 manifest..."
(
  cd "${TARGET_DIR}"
  sha256sum qrcardapio.sqlite $( [ -f media.tar.gz ] && echo "media.tar.gz" ) > SHA256SUMS
)

if [ "${RETENTION_DAYS}" -gt 0 ] 2>/dev/null; then
  find "${BACKUP_ROOT}" -mindepth 1 -maxdepth 1 -type d -mtime "+${RETENTION_DAYS}" -exec rm -rf {} + || true
fi

echo "[backup] Completed safely at ${TARGET_DIR}"
