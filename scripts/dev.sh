#!/usr/bin/env bash
set -euo pipefail

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

mkdir -p "${DATA_DIR:-./data}/db" "${DATA_DIR:-./data}/media" "${DATA_DIR:-./data}/published/.tmp"
exec cargo run -- serve
