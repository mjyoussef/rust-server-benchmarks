#!/usr/bin/env bash
set -euo pipefail

export RUST_BACKTRACE=1

# Change as needed
KIND=closed
RUNTIME=6
DELAY=64
IP=127.0.0.1
PORT=7878
NUM_CLIENTS=1
WORKLOAD="constant"

cargo run --release --bin client -- \
  --kind "$KIND" \
  --runtime "$RUNTIME" \
  --delay "$DELAY" \
  --ip "$IP" \
  --port "$PORT" \
  --num-clients "$NUM_CLIENTS" \
  $WORKLOAD