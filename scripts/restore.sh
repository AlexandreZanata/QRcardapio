#!/usr/bin/env bash
set -euo pipefail

# Verified restore from a backup directory created by scripts/backup.sh.
# IMPORTANT: Stop the qrcardapio server before running restore!
# Usage: ./scripts/restore.sh <backup_dir>

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <backup_dir>" >&2
  exit 1
fi

BACKUP_DIR="$1"
DATA_DIR="${DATA_DIR:-./data}"
BIN="${QRCARDAPIO_BIN:-qrcardapio}"

if [ ! -f "${BACKUP_DIR}/SHA256SUMS" ]; then
  echo "[restore] Missing SHA256SUMS manifest in ${BACKUP_DIR}" >&2
  exit 1
fi

echo "[restore] Verifying SHA-256 checksums..."
(
  cd "${BACKUP_DIR}"
  sha256sum -c SHA256SUMS
)

echo "[restore] Restoring SQLite database..."
DATA_DIR="${DATA_DIR}" "${BIN}" db restore --input "${BACKUP_DIR}/qrcardapio.sqlite"

if [ -f "${BACKUP_DIR}/media.tar.gz" ]; then
  echo "[restore] Restoring media assets..."
  mkdir -p "${DATA_DIR}"
  rm -rf "${DATA_DIR}/media.restoring"
  mkdir -p "${DATA_DIR}/media.restoring"
  tar -C "${DATA_DIR}/media.restoring" -xzf "${BACKUP_DIR}/media.tar.gz"
  rm -rf "${DATA_DIR}/media.old"
  if [ -d "${DATA_DIR}/media" ]; then
    mv "${DATA_DIR}/media" "${DATA_DIR}/media.old"
  fi
  mv "${DATA_DIR}/media.restoring/media" "${DATA_DIR}/media"
  rm -rf "${DATA_DIR}/media.restoring" "${DATA_DIR}/media.old"
fi

echo "[restore] Rebuilding static published snapshots..."
DATA_DIR="${DATA_DIR}" "${BIN}" republish-all

echo "[restore] Completed cleanly for DATA_DIR=${DATA_DIR}"
