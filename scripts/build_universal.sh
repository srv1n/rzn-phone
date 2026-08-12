#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CRATE="rzn_phone_worker"
OUT_DIR="dist/bin/macos/universal"
BINS=("rzn-phone" "rzn-phone-worker")
TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")

[[ "$(uname -s)" == "Darwin" ]] || { echo "build_universal.sh requires macOS" >&2; exit 1; }
command -v lipo >/dev/null 2>&1 || { echo "lipo is required" >&2; exit 1; }
rustup target add "${TARGETS[@]}"

echo "[build] building ${CRATE} for aarch64-apple-darwin"
cargo build -p "$CRATE" --release --target aarch64-apple-darwin --features cli --bin rzn-phone --bin rzn-phone-worker

echo "[build] building ${CRATE} for x86_64-apple-darwin"
cargo build -p "$CRATE" --release --target x86_64-apple-darwin --features cli --bin rzn-phone --bin rzn-phone-worker

mkdir -p "$OUT_DIR"

for bin in "${BINS[@]}"; do
  aarch64_bin="target/aarch64-apple-darwin/release/${bin}"
  x86_bin="target/x86_64-apple-darwin/release/${bin}"
  out_bin="${OUT_DIR}/${bin}"

  echo "[build] creating universal binary for ${bin}"
  lipo -create "$aarch64_bin" "$x86_bin" -output "$out_bin"
  chmod +x "$out_bin"
  echo "[ok] universal binary: $out_bin"
done
