#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(
  python3 - <<'PY'
import json
from pathlib import Path
config = json.loads(Path("plugin_bundle/ios-tools.bundle.json").read_text(encoding="utf-8"))
print(config["version"])
PY
)"

PLATFORM="macos_universal"
PLUGIN_DIR="$ROOT/dist/plugins/ios-tools/$VERSION/$PLATFORM"
PLUGIN_ZIP="$PLUGIN_DIR/ios-tools-$VERSION-$PLATFORM.zip"
KIT_NAME="ios-tools-tester-kit-$VERSION"
STAGE_DIR="$ROOT/dist/tester-kit/$KIT_NAME"
ZIP_PATH="$ROOT/dist/tester-kit/$KIT_NAME.zip"

mkdir -p "$ROOT/dist/tester-kit"

echo "[build] packaging signed plugin bundle"
"$ROOT/scripts/package_plugin.sh"

if [[ ! -f "$PLUGIN_ZIP" ]]; then
  echo "[error] expected plugin zip missing: $PLUGIN_ZIP" >&2
  exit 1
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"/artifacts
mkdir -p "$STAGE_DIR"/scripts
mkdir -p "$STAGE_DIR"/examples
mkdir -p "$STAGE_DIR"/cards

cp "$PLUGIN_ZIP" "$STAGE_DIR/artifacts/"
cp "$ROOT/scripts/tester_doctor.sh" "$STAGE_DIR/scripts/"
cp "$ROOT/docs/tester_kit.md" "$STAGE_DIR/INSTALL.md"
cp "$ROOT/docs/agent_setup.md" "$STAGE_DIR/AGENT_SETUP.md"
cp "$ROOT/examples/ios-tools.mcp.json" "$STAGE_DIR/examples/"
cp "$ROOT/examples/agent-handoff.md" "$STAGE_DIR/examples/"
cp -R "$ROOT/cards/social" "$STAGE_DIR/cards/"

chmod +x "$STAGE_DIR/scripts/tester_doctor.sh"

rm -f "$ZIP_PATH"
(
  cd "$ROOT/dist/tester-kit"
  zip -qry "$ZIP_PATH" "$KIT_NAME"
)

echo "[ok] tester kit created: $ZIP_PATH"
echo "[info] plugin zip inside kit: artifacts/$(basename "$PLUGIN_ZIP")"
