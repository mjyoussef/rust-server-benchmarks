#!/usr/bin/env bash
set -euo pipefail

export RUST_BACKTRACE=1

# Change as needed
KIND=vanilla
TIMEOUT=24
IP=127.0.0.1
PORT=7878

cargo run --release --bin server -- \
  --kind "$KIND" \
  --timeout "$TIMEOUT" \
  --ip "$IP" \
  --port "$PORT"