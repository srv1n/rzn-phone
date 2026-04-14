#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN="$ROOT/target/release/rzn-phone-worker"
echo "[build] compiling release worker"
cargo build -p rzn_phone_worker --release >/dev/null

echo "[smoke] initialize + tools/list"
cat <<'JSON' | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"tools-1","method":"tools/list","params":{}}
JSON
