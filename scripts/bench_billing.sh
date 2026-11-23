#!/usr/bin/env bash
set -euo pipefail

# Local release benchmark. The sandbox credentials are fixtures and no call is sent to Asaas.
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT_DIR}/target/release/qrcardapio"
WORK_DIR="$(mktemp -d /tmp/qrcardapio-billing-bench.XXXXXX)"
SERVER_PID=
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

PORT="${BENCH_PORT:-18881}"
[[ "$PORT" =~ ^[0-9]{1,5}$ ]]
BASE_URL="http://127.0.0.1:${PORT}"
export DATA_DIR="$WORK_DIR/data" STATIC_DIR="$ROOT_DIR/static" LOG_LEVEL=warn
export BIND_ADDR="127.0.0.1:${PORT}" PUBLIC_BASE_URL=https://menu.example.test COOKIE_SECURE=true
export ASAAS_ENV=sandbox ASAAS_API_KEY=benchmark-fixture ASAAS_WEBHOOK_TOKEN
ASAAS_WEBHOOK_TOKEN="$(openssl rand -hex 24)"
LOCAL_PASSWORD="$(openssl rand -hex 16)"

if [[ ! -x "$BIN" ]]; then
  (cd "$ROOT_DIR" && cargo build --release --locked)
fi
"$BIN" bench-gen --establishments 1000 --items-per-establishment 35 --publish-sample 1 >/dev/null
"$BIN" seed --password "$LOCAL_PASSWORD" >/dev/null

if [[ -n "${BENCH_CPU:-}" ]]; then
  taskset -c "$BENCH_CPU" "$BIN" serve &
else
  "$BIN" serve &
fi
SERVER_PID=$!
for _ in {1..30}; do
  if curl -fsS "$BASE_URL/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done
curl -fsS "$BASE_URL/readyz" >/dev/null

curl -fsS -c "$WORK_DIR/cookies" --data-urlencode 'email=demo@qrcardapio.com.br' \
  --data-urlencode "password=$LOCAL_PASSWORD" "$BASE_URL/login" >/dev/null
COOKIE="$(awk '$6 == "qr_session" {print $6 "=" $7}' "$WORK_DIR/cookies")"
[[ -n "$COOKIE" ]]

cat > "$WORK_DIR/event.json" <<'JSON'
{"id":"evt_bench_checkout","event":"CHECKOUT_EXPIRED","checkout":{"id":"2d30dcee-0051-40f9-bcb3-11e1ff63cbea"}}
JSON
WEBHOOK_HEADER="asaas-access-token: $ASAAS_WEBHOOK_TOKEN"
curl -fsS -H "$WEBHOOK_HEADER" -H 'Content-Type: application/json' \
  --data-binary "@$WORK_DIR/event.json" "$BASE_URL/webhooks/asaas" >/dev/null

echo "RSS inicial $(awk '/VmRSS/ {print $2, $3}' "/proc/$SERVER_PID/status")"
for route in dashboard dashboard/billing; do
  echo "GET /$route (16 conexões, 5 s)"
  oha -w -z 5s -c 16 --no-tui -H "Cookie: $COOKIE" "$BASE_URL/$route"
done

echo 'POST /webhooks/asaas (reentrega do mesmo evento, 16 conexões, 5 s)'
oha -w -z 5s -c 16 --no-tui -m POST -H "$WEBHOOK_HEADER" -H 'Content-Type: application/json' \
  -D "$WORK_DIR/event.json" "$BASE_URL/webhooks/asaas"
echo 'POST /webhooks/asaas (1.000 eventos inéditos de cobrança, 16 conexões)'
python3 - "$PORT" <<'PYTHON'
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from http.client import HTTPConnection
import json
import os
import sys
from time import perf_counter

workers = 16
count = 1000
port = int(sys.argv[1])
created = datetime.now(timezone(timedelta(hours=-3)))
headers = {"Content-Type": "application/json", "asaas-access-token": os.environ["ASAAS_WEBHOOK_TOKEN"]}

def send_batch(index):
    connection = HTTPConnection("127.0.0.1", port, timeout=10)
    results = []
    try:
        for number in range(index, count, workers):
            event = json.dumps({
                "id": f"evt_bench_{number}", "event": "PAYMENT_CONFIRMED",
                "dateCreated": created.strftime("%Y-%m-%d %H:%M:%S"),
                "payment": {"id": f"pay_bench_{number}", "subscription": "sub_bench",
                            "dueDate": created.date().isoformat(), "value": 29.90},
            })
            started = perf_counter()
            connection.request("POST", "/webhooks/asaas", event, headers)
            response = connection.getresponse()
            response.read()
            results.append((response.status, (perf_counter() - started) * 1000))
    finally:
        connection.close()
    return results

started = perf_counter()
with ThreadPoolExecutor(max_workers=workers) as pool:
    results = [result for batch in pool.map(send_batch, range(workers)) for result in batch]
elapsed = perf_counter() - started
successes = sum(status == 200 for status, _ in results)
latencies = sorted(latency for _, latency in results)
print(f"{count / elapsed:.2f} req/s · p95 {latencies[949]:.2f} ms · HTTP 200 {successes}/{count}")
if successes != count:
    sys.exit(1)
PYTHON
echo "RSS final $(awk '/VmRSS/ {print $2, $3}' "/proc/$SERVER_PID/status")"
