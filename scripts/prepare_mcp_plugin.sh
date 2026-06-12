#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="$ROOT/artifacts"
PLUGIN_OUT="${RZN_PHONE_TESTER_PLUGIN_DIR:-$ROOT/plugin/rzn-phone}"
CONFIG_OUT="$ROOT/generated/rzn-phone.mcp.json"

fail() {
  printf '[fail] %s\n' "$1" >&2
  exit 1
}

pass() {
  printf '[ok] %s\n' "$1"
}

if [[ "${RZN_PHONE_TESTER_SKIP_DOCTOR:-0}" != "1" ]]; then
  if [[ ! -x "$ROOT/scripts/tester_doctor.sh" ]]; then
    fail "missing prerequisite checker: $ROOT/scripts/tester_doctor.sh"
  fi
  RZN_TESTER_DOCTOR_CALLED_BY_PREP=1 "$ROOT/scripts/tester_doctor.sh"
fi

command -v unzip >/dev/null 2>&1 || fail "unzip missing. Install the macOS command line tools, then rerun this script."
command -v python3 >/dev/null 2>&1 || fail "python3 missing. Install Python 3, then rerun this script."

shopt -s nullglob
artifacts=( "$ARTIFACT_DIR"/rzn-phone-*-macos_universal.zip )
shopt -u nullglob

if [[ "${#artifacts[@]}" -eq 0 ]]; then
  fail "no plugin artifact found under artifacts/. Expected artifacts/rzn-phone-<version>-macos_universal.zip"
fi
if [[ "${#artifacts[@]}" -gt 1 ]]; then
  fail "multiple plugin artifacts found under artifacts/. Keep exactly one rzn-phone-*-macos_universal.zip"
fi

artifact="${artifacts[0]}"
rm -rf "$PLUGIN_OUT"
mkdir -p "$PLUGIN_OUT"
unzip -q "$artifact" -d "$PLUGIN_OUT"

worker="$PLUGIN_OUT/bin/macos/universal/rzn-phone-worker"
workflow_dir="$PLUGIN_OUT/resources/workflows"

[[ -x "$worker" ]] || fail "unpacked worker is missing or not executable: $worker"
[[ -d "$workflow_dir" ]] || fail "unpacked workflow directory is missing: $workflow_dir"

shopt -s nullglob
workflows=( "$workflow_dir"/*.json )
shopt -u nullglob
[[ "${#workflows[@]}" -gt 0 ]] || fail "unpacked workflow directory has no workflow JSON files: $workflow_dir"

mkdir -p "$(dirname "$CONFIG_OUT")"
python3 - "$worker" "$PLUGIN_OUT" "$CONFIG_OUT" <<'PY'
import json
import sys
from pathlib import Path

worker = sys.argv[1]
plugin_root = sys.argv[2]
out = Path(sys.argv[3])

config = {
    "mcpServers": {
        "rzn-phone": {
            "command": worker,
            "args": [],
            "env": {
                "RZN_PLUGIN_DIR": plugin_root,
                "RZN_IOS_APPIUM_URL": "http://127.0.0.1:4723",
            },
        }
    }
}

out.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
PY

pass "plugin unpacked: $PLUGIN_OUT"
pass "worker ready: $worker"
pass "workflow files: ${#workflows[@]}"
pass "MCP config written: $CONFIG_OUT"

cat <<EOF

Next action:
  1. Start Appium in another terminal: appium
  2. Add the generated MCP config to your client, or copy these values:
     command: $worker
     RZN_PLUGIN_DIR: $PLUGIN_OUT
     RZN_IOS_APPIUM_URL: http://127.0.0.1:4723
  3. Start with ios.env.doctor, ios.device.list, and one read-only workflow.
EOF
