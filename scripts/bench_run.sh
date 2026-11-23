#!/usr/bin/env bash
set -euo pipefail

# Reproducible benchmark suite for QR Cardapio (SQLite scaling, publish time, RSS memory, HTTP RPS/latency).
# Runs inside an isolated temporary directory so the repository stays clean.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT_DIR}/target/release/qrcardapio"
WORK_DIR="$(mktemp -d /tmp/qrcardapio-bench.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

if [ ! -x "${BIN}" ]; then
  echo "[bench] Building release binary..."
  (cd "${ROOT_DIR}" && cargo build --release --locked)
fi

echo "[bench] Host specs: $(nproc) vCPU, $(free -h | awk '/Mem:/ {print $2}') RAM, kernel $(uname -r)"
echo "[bench] Work dir: ${WORK_DIR}"

for COUNT in 100 1000 10000 100000; do
  RUN_DIR="${WORK_DIR}/scale_${COUNT}"
  mkdir -p "${RUN_DIR}"
  echo "--- Scale ${COUNT} establishments (12 items each) ---"
  DATA_DIR="${RUN_DIR}" STATIC_DIR="${ROOT_DIR}/static" LOG_LEVEL=warn \
    "${BIN}" bench-gen --establishments "${COUNT}" --items-per-establishment 12 --publish-sample 3
done

# Menu size publish test + HTTP benchmarks on a fresh 1,000-establishment DB
HTTP_DIR="${WORK_DIR}/http_bench"
mkdir -p "${HTTP_DIR}"
echo "--- Preparing HTTP & Publish benchmark dataset (1,000 establishments) ---"
GEN_OUT="$(DATA_DIR="${HTTP_DIR}" STATIC_DIR="${ROOT_DIR}/static" LOG_LEVEL=warn \
  "${BIN}" bench-gen --establishments 1000 --items-per-establishment 35 --publish-sample 5)"
echo "${GEN_OUT}"
SLUG="$(echo "${GEN_OUT}" | sed -n 's/.*First slug: \([^.]*\)\..*/\1/p')"
echo "[bench] Target published slug: ${SLUG}"
echo "[bench] Published HTML bytes: $(wc -c < "${HTTP_DIR}/published/${SLUG}/index.html") B"
echo "[bench] Published HTML gzipped bytes: $(gzip -c "${HTTP_DIR}/published/${SLUG}/index.html" | wc -c) B"

SEED_OUT="$(DATA_DIR="${HTTP_DIR}" STATIC_DIR="${ROOT_DIR}/static" LOG_LEVEL=warn \
  "${BIN}" seed --password 'BenchPassword123')"
echo "${SEED_OUT}"

PORT=18880
BIND_ADDR="127.0.0.1:${PORT}" \
PUBLIC_BASE_URL="http://127.0.0.1:${PORT}" \
DATA_DIR="${HTTP_DIR}" \
STATIC_DIR="${ROOT_DIR}/static" \
COOKIE_SECURE=false \
LOG_LEVEL=warn \
  "${BIN}" serve &
SRV_PID=$!
trap 'kill ${SRV_PID} 2>/dev/null || true; rm -rf "${WORK_DIR}"' EXIT

for _ in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

echo "[bench] Server RSS before load: $(awk '/VmRSS/ {print $2, $3}' "/proc/${SRV_PID}/status")"

# Obtain authenticated session cookie
COOKIE_FILE="${WORK_DIR}/cookies.txt"
curl -fsS -c "${COOKIE_FILE}" -d "email=demo%40qrcardapio.com.br&password=BenchPassword123" \
  "http://127.0.0.1:${PORT}/login" >/dev/null
CSRF_TOK="$(curl -fsS -b "${COOKIE_FILE}" "http://127.0.0.1:${PORT}/dashboard" | sed -n 's/.*name="_csrf" value="\([^"]*\)".*/\1/p' | head -n1)"

echo "--- Benchmark 1: Public static menu GET /c/${SLUG} (5s, 32 connections) ---"
oha -z 5s -c 32 --no-tui "http://127.0.0.1:${PORT}/c/${SLUG}"

echo "--- Benchmark 2: Authenticated dashboard GET /dashboard (5s, 16 connections) ---"
COOKIE_HDR="$(awk '/qr_session/ {print "qr_session=" $7}' "${COOKIE_FILE}")"
oha -z 5s -c 16 -H "Cookie: ${COOKIE_HDR}" --no-tui "http://127.0.0.1:${PORT}/dashboard"

echo "--- Benchmark 3: Concurrent publish during public load on /c/cafe-da-praca ---"
(
  for i in 1 2 3 4 5; do
    curl -fsS -b "${COOKIE_FILE}" -d "_csrf=${CSRF_TOK}" "http://127.0.0.1:${PORT}/dashboard/publish" >/dev/null
  done
) &
PUB_PID=$!
oha -z 4s -c 32 --no-tui "http://127.0.0.1:${PORT}/c/cafe-da-praca"
wait "${PUB_PID}"

echo "[bench] Server RSS after load: $(awk '/VmRSS/ {print $2, $3}' "/proc/${SRV_PID}/status")"
kill "${SRV_PID}" && wait "${SRV_PID}" 2>/dev/null || true
