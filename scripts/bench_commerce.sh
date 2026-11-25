#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
allowed=$(taskset -pc "$$")
allowed=${allowed##* }
cpu=${COMMERCE_BENCH_CPU:-${allowed%%[-,]*}}

run_bench() {
    local target=$1 filter=$2 selector=$3 executable
    shift 3
    executable=$(cargo test --release --locked --no-run --message-format=json-render-diagnostics \
        "$selector" "$@" | python3 -c '
import json
import sys

executable = None
for line in sys.stdin:
    message = json.loads(line)
    if (message.get("reason") == "compiler-artifact" and message.get("executable")
            and message["target"]["name"] == sys.argv[1] and message["profile"]["test"]):
        executable = message["executable"]
if executable is None:
    sys.exit("benchmark executable was not built")
print(executable)
' "$target")
    taskset -c "$cpu" "$executable" "$filter" --ignored --nocapture
}

checkout() { run_bench commerce_checkout_bench sustained_checkout --test commerce_checkout_bench; }
outbox() { run_bench commerce_dispatch_benchmark drain_and_steady_state --test commerce_dispatch_benchmark; }
push() { run_bench qrcardapio notifications::tests::push_throughput --lib; }

case "${1:-all}" in
    checkout) checkout ;;
    mixed) COMMERCE_BENCH_MIXED=1 checkout ;;
    outbox) outbox ;;
    push) push ;;
    all) checkout; COMMERCE_BENCH_MIXED=1 checkout; outbox; push ;;
    *) printf 'Usage: %s [checkout|mixed|outbox|push|all]\n' "$0" >&2; exit 2 ;;
esac
