#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export RZN_PHONE_ROOT="$ROOT"
DEFAULT_PRIV_KEY="/Users/sarav/Downloads/side/rzn/rznapp/.secrets/plugin-signing/ed25519.private"
DEFAULT_PUB_KEY="/Users/sarav/Downloads/side/rzn/rznapp/.secrets/plugin-signing/ed25519.public"

PRIV_KEY="${1:-$DEFAULT_PRIV_KEY}"
PUB_KEY="${2:-$DEFAULT_PUB_KEY}"
PLATFORM="macos_universal"
CONFIG="$ROOT/plugin_bundle/rzn-phone.bundle.json"
OUT_DIR="$ROOT/dist/plugins"

resolve_devkit() {
  if [[ -n "${RZN_PLUGIN_DEVKIT_BIN:-}" && -x "${RZN_PLUGIN_DEVKIT_BIN}" ]]; then
    echo "${RZN_PLUGIN_DEVKIT_BIN}"
    return 0
  fi
  if command -v rzn-plugin-devkit >/dev/null 2>&1; then
    command -v rzn-plugin-devkit
    return 0
  fi
  if [[ -x "/Users/sarav/Downloads/side/rzn/rzn-browser-native/target/release/rzn-plugin-devkit" ]]; then
    echo "/Users/sarav/Downloads/side/rzn/rzn-browser-native/target/release/rzn-plugin-devkit"
    return 0
  fi
  if [[ -x "/Users/sarav/Downloads/side/rzn/rzn-python-sandbox/target/release/rzn-plugin-devkit" ]]; then
    echo "/Users/sarav/Downloads/side/rzn/rzn-python-sandbox/target/release/rzn-plugin-devkit"
    return 0
  fi
  return 1
}

is_valid_seed_key() {
  local path="$1"
  python3 - "$path" <<'PY'
import base64
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
try:
    raw = base64.b64decode(path.read_text(encoding="utf-8").strip(), validate=True)
except Exception:
    print("0")
    raise SystemExit(0)
print("1" if len(raw) == 32 else "0")
PY
}

DEVKIT="$(resolve_devkit || true)"
if [[ -z "$DEVKIT" ]]; then
  echo "[error] rzn-plugin-devkit not found"
  echo "        install/build it, or set RZN_PLUGIN_DEVKIT_BIN"
  exit 1
fi

if [[ ! -x "dist/bin/macos/universal/rzn-ios-tools-worker" ]]; then
  echo "[info] universal binary missing, building it first"
  "$ROOT/scripts/build_universal.sh"
fi

if [[ ! -f "$PRIV_KEY" || ! -f "$PUB_KEY" ]]; then
  if [[ "$PRIV_KEY" == "$DEFAULT_PRIV_KEY" || "$PUB_KEY" == "$DEFAULT_PUB_KEY" ]]; then
    KEY_DIR="$ROOT/.secrets/plugin-signing"
    mkdir -p "$KEY_DIR"
    echo "[info] signing keys missing at defaults, generating dev keys in $KEY_DIR"
    "$DEVKIT" keygen --out "$KEY_DIR"
    PRIV_KEY="$KEY_DIR/ed25519.private"
    PUB_KEY="$KEY_DIR/ed25519.public"
  fi
fi

if [[ ! -f "$PRIV_KEY" ]]; then
  echo "[error] private key not found: $PRIV_KEY"
  exit 1
fi

if [[ ! -f "$PUB_KEY" ]]; then
  echo "[error] public key not found: $PUB_KEY"
  exit 1
fi

if [[ "$(is_valid_seed_key "$PRIV_KEY")" != "1" ]]; then
  if [[ "$PRIV_KEY" == "$DEFAULT_PRIV_KEY" ]]; then
    KEY_DIR="$ROOT/.secrets/plugin-signing"
    mkdir -p "$KEY_DIR"
    echo "[info] default private key format is incompatible with current devkit; generating dev keypair in $KEY_DIR"
    "$DEVKIT" keygen --out "$KEY_DIR"
    PRIV_KEY="$KEY_DIR/ed25519.private"
    PUB_KEY="$KEY_DIR/ed25519.public"
  else
    echo "[error] private key has invalid format for this devkit (expected base64 32-byte seed): $PRIV_KEY"
    exit 1
  fi
fi

echo "[build] packaging signed rzn-phone bundle"
python3 "$ROOT/scripts/build_bundle.py" \
  --config "$CONFIG" \
  --platform "$PLATFORM" \
  --key "$PRIV_KEY" \
  --out "$OUT_DIR" \
  --devkit "$DEVKIT" >/dev/null

ZIP_PATH="$(ls -1 "$OUT_DIR"/rzn-phone/0.1.0/"$PLATFORM"/rzn-phone-0.1.0-"$PLATFORM".zip | head -n1)"
PLUGIN_JSON="$(ls -1 "$OUT_DIR"/rzn-phone/0.1.0/"$PLATFORM"/plugin.json | head -n1)"
PLUGIN_SIG="$(ls -1 "$OUT_DIR"/rzn-phone/0.1.0/"$PLATFORM"/plugin.sig | head -n1)"

echo "[verify] verifying plugin bundle"
"$DEVKIT" verify --public "$PUB_KEY" --input "$PLUGIN_JSON" --sig "$PLUGIN_SIG"

echo "[ok] bundle verified: $ZIP_PATH"
