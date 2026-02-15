#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CRATE="rzn_ios_tools_worker"
AARCH64_BIN="target/aarch64-apple-darwin/release/${CRATE}"
X86_BIN="target/x86_64-apple-darwin/release/${CRATE}"
OUT_DIR="dist/bin/macos/universal"
OUT_BIN="${OUT_DIR}/rzn-ios-tools-worker"

echo "[build] building ${CRATE} for aarch64-apple-darwin"
cargo build -p "$CRATE" --release --target aarch64-apple-darwin

echo "[build] building ${CRATE} for x86_64-apple-darwin"
cargo build -p "$CRATE" --release --target x86_64-apple-darwin

mkdir -p "$OUT_DIR"

echo "[build] creating universal binary"
lipo -create "$AARCH64_BIN" "$X86_BIN" -output "$OUT_BIN"
chmod +x "$OUT_BIN"

echo "[ok] universal binary: $OUT_BIN"
