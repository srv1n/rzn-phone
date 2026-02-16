#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: scripts/ios_tools.sh <command> [args]

Commands:
  build                 Build release worker binary
  build-universal       Build universal macOS worker binary
  test                  Run unit and integration tests
  smoke                 Run JSON-RPC initialize + tools/list smoke
  doctor                Run ios.env.doctor through worker
  devices               Run ios.device.list (physical devices only)
  shutdown [stopAppium=1|0]
                        Close active session and optionally stop Appium (default: stopAppium=1).
  wda-shutdown [port]   Best-effort shutdown of WebDriverAgent (default port: 8100).
  package [priv] [pub]  Build and sign plugin ZIP, then verify signature
  workflow-smoke <udid> [query] [limit]
                        Run safari.google_search workflow on real iPhone.
                        Optional env: IOS_XCODE_ORG_ID, IOS_XCODE_SIGNING_ID, IOS_UPDATED_WDA_BUNDLE_ID, IOS_SHOW_XCODE_LOG=1,
                                      IOS_ALLOW_PROVISIONING_UPDATES=1, IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION=1,
                                      IOS_SESSION_CREATE_TIMEOUT_MS=600000,
                                      IOS_WDA_LAUNCH_TIMEOUT_MS=240000, IOS_WDA_CONNECTION_TIMEOUT_MS=120000,
                                      IOS_STOP_APPIUM_ON_EXIT=1
  reddit-read-smoke <udid>
                        Run reddit.read_first_post workflow (read-only) and return compact snapshot.
  reddit-comment-smoke <udid> <commentText> [commit=0|1]
                        Run reddit.comment_first_post workflow. commit must be 1 to actually submit.
  appstore-typeahead <udid> <query> [--out <dir>] [--limit <n>] [--typing-mode <full|char-by-char>] [--country <cc>] [--locale <locale>]
                        Run appstore.typeahead and write result.json + screenshot.png + ui_source.xml.
  appstore-search-results <udid> <query> [--out <dir>] [--limit <n>] [--target-app-name <name>] [--max-scrolls <n>] [--country <cc>] [--locale <locale>]
  appstore-search-results <udid> <query> [--out <dir>] [--limit <n>] [--target-app-name <name>] [--max-scrolls <n>] [--submit-mode <suggestion|keyboard>] [--country <cc>] [--locale <locale>]
                        Run appstore.search_results and write result.json + screenshot.png + ui_source.xml.
  appstore-smoke <udid> [query]
                        Real-device smoke test; asserts at least 1 suggestion and 1 result row.
EOF
}

worker_bin() {
  local bin="$ROOT/target/release/rzn_ios_tools_worker"
  cargo build -p rzn_ios_tools_worker --release >/dev/null
  echo "$bin"
}

bool_json() {
  local value="${1:-0}"
  if [[ "$value" == "1" || "$value" == "true" ]]; then
    echo "true"
  else
    echo "false"
  fi
}

load_ios_session_env() {
  IOS_XCODE_ORG_ID="${IOS_XCODE_ORG_ID:-}"
  IOS_XCODE_SIGNING_ID="${IOS_XCODE_SIGNING_ID:-}"
  IOS_UPDATED_WDA_BUNDLE_ID="${IOS_UPDATED_WDA_BUNDLE_ID:-}"
  IOS_SHOW_XCODE_LOG="${IOS_SHOW_XCODE_LOG:-0}"
  IOS_ALLOW_PROVISIONING_UPDATES="${IOS_ALLOW_PROVISIONING_UPDATES:-0}"
  IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION="${IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION:-0}"
  IOS_SESSION_CREATE_TIMEOUT_MS="${IOS_SESSION_CREATE_TIMEOUT_MS:-600000}"
  IOS_WDA_LAUNCH_TIMEOUT_MS="${IOS_WDA_LAUNCH_TIMEOUT_MS:-240000}"
  IOS_WDA_CONNECTION_TIMEOUT_MS="${IOS_WDA_CONNECTION_TIMEOUT_MS:-120000}"
  IOS_STOP_APPIUM_ON_EXIT="${IOS_STOP_APPIUM_ON_EXIT:-1}"
}

build_signing_json() {
  if [[ -n "$IOS_XCODE_ORG_ID" || -n "$IOS_XCODE_SIGNING_ID" || -n "$IOS_UPDATED_WDA_BUNDLE_ID" ]]; then
    jq -nc \
      --arg xcodeOrgId "$IOS_XCODE_ORG_ID" \
      --arg xcodeSigningId "$IOS_XCODE_SIGNING_ID" \
      --arg updatedWDABundleId "$IOS_UPDATED_WDA_BUNDLE_ID" \
      '{xcodeOrgId: $xcodeOrgId, xcodeSigningId: $xcodeSigningId, updatedWDABundleId: $updatedWDABundleId}'
  else
    echo '{}'
  fi
}

cmd="${1:-help}"
shift || true

case "$cmd" in
  build)
    cargo build -p rzn_ios_tools_worker --release
    ;;
  build-universal)
    "$ROOT/scripts/build_universal.sh"
    ;;
  test)
    cargo test -p rzn_ios_tools_worker
    ;;
  smoke)
    "$ROOT/scripts/run_smoke.sh"
    ;;
  doctor)
    BIN="$(worker_bin)"
    cat <<'JSON' | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"doctor-1","method":"tools/call","params":{"name":"ios.env.doctor","arguments":{}}}
JSON
    ;;
  devices)
    BIN="$(worker_bin)"
    cat <<'JSON' | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"devices-1","method":"tools/call","params":{"name":"ios.device.list","arguments":{"includeSimulators":false}}}
JSON
    ;;
  shutdown)
    STOP_APPIUM="${1:-1}"
    STOP_APPIUM_JSON="true"
    if [[ "$STOP_APPIUM" == "0" ]]; then
      STOP_APPIUM_JSON="false"
    fi
    BIN="$(worker_bin)"
    cat <<JSON | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_JSON,"shutdownWDA":true}}}
JSON
    ;;
  wda-shutdown)
    PORT="${1:-8100}"
    BIN="$(worker_bin)"
    cat <<JSON | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wda-1","method":"tools/call","params":{"name":"ios.wda.shutdown","arguments":{"port":$PORT}}}
JSON
    ;;
  package)
    if [[ "$#" -eq 0 ]]; then
      "$ROOT/scripts/package_plugin.sh"
    elif [[ "$#" -eq 1 ]]; then
      "$ROOT/scripts/package_plugin.sh" "$1"
    else
      "$ROOT/scripts/package_plugin.sh" "$1" "$2"
    fi
    ;;
  workflow-smoke)
    UDID="${1:-}"
    QUERY="${2:-best wireless headphones}"
    LIMIT="${3:-5}"
    IOS_XCODE_ORG_ID="${IOS_XCODE_ORG_ID:-}"
    IOS_XCODE_SIGNING_ID="${IOS_XCODE_SIGNING_ID:-}"
    IOS_UPDATED_WDA_BUNDLE_ID="${IOS_UPDATED_WDA_BUNDLE_ID:-}"
    IOS_SHOW_XCODE_LOG="${IOS_SHOW_XCODE_LOG:-0}"
    IOS_ALLOW_PROVISIONING_UPDATES="${IOS_ALLOW_PROVISIONING_UPDATES:-0}"
    IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION="${IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION:-0}"
    IOS_SESSION_CREATE_TIMEOUT_MS="${IOS_SESSION_CREATE_TIMEOUT_MS:-600000}"
    IOS_WDA_LAUNCH_TIMEOUT_MS="${IOS_WDA_LAUNCH_TIMEOUT_MS:-240000}"
    IOS_WDA_CONNECTION_TIMEOUT_MS="${IOS_WDA_CONNECTION_TIMEOUT_MS:-120000}"
    IOS_STOP_APPIUM_ON_EXIT="${IOS_STOP_APPIUM_ON_EXIT:-1}"
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh workflow-smoke <udid> [query] [limit]" >&2
      exit 1
    fi
    BIN="$(worker_bin)"
    SIGNING_JSON='{}'
    if [[ -n "$IOS_XCODE_ORG_ID" || -n "$IOS_XCODE_SIGNING_ID" || -n "$IOS_UPDATED_WDA_BUNDLE_ID" ]]; then
      SIGNING_JSON="{\"xcodeOrgId\":\"$IOS_XCODE_ORG_ID\",\"xcodeSigningId\":\"$IOS_XCODE_SIGNING_ID\",\"updatedWDABundleId\":\"$IOS_UPDATED_WDA_BUNDLE_ID\"}"
    fi
    SHOW_XCODE_LOG_JSON="false"
    if [[ "$IOS_SHOW_XCODE_LOG" == "1" ]]; then
      SHOW_XCODE_LOG_JSON="true"
    fi
    ALLOW_PROVISIONING_UPDATES_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_UPDATES" == "1" ]]; then
      ALLOW_PROVISIONING_UPDATES_JSON="true"
    fi
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION" == "1" ]]; then
      ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="true"
    fi
    STOP_APPIUM_ON_EXIT_JSON="false"
    if [[ "$IOS_STOP_APPIUM_ON_EXIT" == "1" ]]; then
      STOP_APPIUM_ON_EXIT_JSON="true"
    fi
    cat <<JSON | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"safari.google_search","session":{"udid":"$UDID","showXcodeLog":$SHOW_XCODE_LOG_JSON,"allowProvisioningUpdates":$ALLOW_PROVISIONING_UPDATES_JSON,"allowProvisioningDeviceRegistration":$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON,"sessionCreateTimeoutMs":$IOS_SESSION_CREATE_TIMEOUT_MS,"wdaLaunchTimeoutMs":$IOS_WDA_LAUNCH_TIMEOUT_MS,"wdaConnectionTimeoutMs":$IOS_WDA_CONNECTION_TIMEOUT_MS,"signing":$SIGNING_JSON},"args":{"query":"$QUERY","limit":$LIMIT},"commit":false}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_ON_EXIT_JSON}}}
JSON
    ;;
  reddit-read-smoke)
    UDID="${1:-}"
    IOS_XCODE_ORG_ID="${IOS_XCODE_ORG_ID:-}"
    IOS_XCODE_SIGNING_ID="${IOS_XCODE_SIGNING_ID:-}"
    IOS_UPDATED_WDA_BUNDLE_ID="${IOS_UPDATED_WDA_BUNDLE_ID:-}"
    IOS_SHOW_XCODE_LOG="${IOS_SHOW_XCODE_LOG:-0}"
    IOS_ALLOW_PROVISIONING_UPDATES="${IOS_ALLOW_PROVISIONING_UPDATES:-0}"
    IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION="${IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION:-0}"
    IOS_SESSION_CREATE_TIMEOUT_MS="${IOS_SESSION_CREATE_TIMEOUT_MS:-600000}"
    IOS_WDA_LAUNCH_TIMEOUT_MS="${IOS_WDA_LAUNCH_TIMEOUT_MS:-240000}"
    IOS_WDA_CONNECTION_TIMEOUT_MS="${IOS_WDA_CONNECTION_TIMEOUT_MS:-120000}"
    IOS_STOP_APPIUM_ON_EXIT="${IOS_STOP_APPIUM_ON_EXIT:-1}"
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-read-smoke <udid>" >&2
      exit 1
    fi
    BIN="$(worker_bin)"
    SIGNING_JSON='{}'
    if [[ -n "$IOS_XCODE_ORG_ID" || -n "$IOS_XCODE_SIGNING_ID" || -n "$IOS_UPDATED_WDA_BUNDLE_ID" ]]; then
      SIGNING_JSON="{\"xcodeOrgId\":\"$IOS_XCODE_ORG_ID\",\"xcodeSigningId\":\"$IOS_XCODE_SIGNING_ID\",\"updatedWDABundleId\":\"$IOS_UPDATED_WDA_BUNDLE_ID\"}"
    fi
    SHOW_XCODE_LOG_JSON="false"
    if [[ "$IOS_SHOW_XCODE_LOG" == "1" ]]; then
      SHOW_XCODE_LOG_JSON="true"
    fi
    ALLOW_PROVISIONING_UPDATES_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_UPDATES" == "1" ]]; then
      ALLOW_PROVISIONING_UPDATES_JSON="true"
    fi
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION" == "1" ]]; then
      ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="true"
    fi
    STOP_APPIUM_ON_EXIT_JSON="false"
    if [[ "$IOS_STOP_APPIUM_ON_EXIT" == "1" ]]; then
      STOP_APPIUM_ON_EXIT_JSON="true"
    fi
    cat <<JSON | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"reddit.read_first_post","session":{"udid":"$UDID","showXcodeLog":$SHOW_XCODE_LOG_JSON,"allowProvisioningUpdates":$ALLOW_PROVISIONING_UPDATES_JSON,"allowProvisioningDeviceRegistration":$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON,"sessionCreateTimeoutMs":$IOS_SESSION_CREATE_TIMEOUT_MS,"wdaLaunchTimeoutMs":$IOS_WDA_LAUNCH_TIMEOUT_MS,"wdaConnectionTimeoutMs":$IOS_WDA_CONNECTION_TIMEOUT_MS,"signing":$SIGNING_JSON},"args":{},"commit":false}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_ON_EXIT_JSON}}}
JSON
    ;;
  reddit-comment-smoke)
    UDID="${1:-}"
    COMMENT_TEXT="${2:-}"
    DO_COMMIT="${3:-0}"
    IOS_XCODE_ORG_ID="${IOS_XCODE_ORG_ID:-}"
    IOS_XCODE_SIGNING_ID="${IOS_XCODE_SIGNING_ID:-}"
    IOS_UPDATED_WDA_BUNDLE_ID="${IOS_UPDATED_WDA_BUNDLE_ID:-}"
    IOS_SHOW_XCODE_LOG="${IOS_SHOW_XCODE_LOG:-0}"
    IOS_ALLOW_PROVISIONING_UPDATES="${IOS_ALLOW_PROVISIONING_UPDATES:-0}"
    IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION="${IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION:-0}"
    IOS_SESSION_CREATE_TIMEOUT_MS="${IOS_SESSION_CREATE_TIMEOUT_MS:-600000}"
    IOS_WDA_LAUNCH_TIMEOUT_MS="${IOS_WDA_LAUNCH_TIMEOUT_MS:-240000}"
    IOS_WDA_CONNECTION_TIMEOUT_MS="${IOS_WDA_CONNECTION_TIMEOUT_MS:-120000}"
    IOS_STOP_APPIUM_ON_EXIT="${IOS_STOP_APPIUM_ON_EXIT:-1}"
    if [[ -z "$UDID" || -z "$COMMENT_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-comment-smoke <udid> <commentText> [commit=0|1]" >&2
      exit 1
    fi
    BIN="$(worker_bin)"
    SIGNING_JSON='{}'
    if [[ -n "$IOS_XCODE_ORG_ID" || -n "$IOS_XCODE_SIGNING_ID" || -n "$IOS_UPDATED_WDA_BUNDLE_ID" ]]; then
      SIGNING_JSON="{\"xcodeOrgId\":\"$IOS_XCODE_ORG_ID\",\"xcodeSigningId\":\"$IOS_XCODE_SIGNING_ID\",\"updatedWDABundleId\":\"$IOS_UPDATED_WDA_BUNDLE_ID\"}"
    fi
    SHOW_XCODE_LOG_JSON="false"
    if [[ "$IOS_SHOW_XCODE_LOG" == "1" ]]; then
      SHOW_XCODE_LOG_JSON="true"
    fi
    ALLOW_PROVISIONING_UPDATES_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_UPDATES" == "1" ]]; then
      ALLOW_PROVISIONING_UPDATES_JSON="true"
    fi
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="false"
    if [[ "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION" == "1" ]]; then
      ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="true"
    fi
    COMMIT_JSON="false"
    if [[ "$DO_COMMIT" == "1" ]]; then
      COMMIT_JSON="true"
    fi
    STOP_APPIUM_ON_EXIT_JSON="false"
    if [[ "$IOS_STOP_APPIUM_ON_EXIT" == "1" ]]; then
      STOP_APPIUM_ON_EXIT_JSON="true"
    fi
    cat <<JSON | "$BIN"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"reddit.comment_first_post","session":{"udid":"$UDID","showXcodeLog":$SHOW_XCODE_LOG_JSON,"allowProvisioningUpdates":$ALLOW_PROVISIONING_UPDATES_JSON,"allowProvisioningDeviceRegistration":$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON,"sessionCreateTimeoutMs":$IOS_SESSION_CREATE_TIMEOUT_MS,"wdaLaunchTimeoutMs":$IOS_WDA_LAUNCH_TIMEOUT_MS,"wdaConnectionTimeoutMs":$IOS_WDA_CONNECTION_TIMEOUT_MS,"signing":$SIGNING_JSON},"args":{"commentText":"$COMMENT_TEXT"},"commit":$COMMIT_JSON}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_ON_EXIT_JSON}}}
JSON
    ;;
  appstore-typeahead)
    UDID="${1:-}"
    QUERY="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    LIMIT=10
    TYPING_MODE="full"
    COUNTRY=""
    LOCALE=""
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --limit)
          LIMIT="${2:-10}"
          shift 2
          ;;
        --typing-mode)
          TYPING_MODE="${2:-full}"
          shift 2
          ;;
        --country)
          COUNTRY="${2:-}"
          shift 2
          ;;
        --locale)
          LOCALE="${2:-}"
          shift 2
          ;;
        *)
          echo "unknown option for appstore-typeahead: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$QUERY" ]]; then
      echo "usage: scripts/ios_tools.sh appstore-typeahead <udid> <query> [--out <dir>] [--limit <n>] [--typing-mode <full|char-by-char>] [--country <cc>] [--locale <locale>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/appstore-typeahead.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    SESSION_JSON="$(jq -nc \
      --arg udid "$UDID" \
      --argjson showXcodeLog "$SHOW_XCODE_LOG_JSON" \
      --argjson allowProvisioningUpdates "$ALLOW_PROVISIONING_UPDATES_JSON" \
      --argjson allowProvisioningDeviceRegistration "$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON" \
      --argjson sessionCreateTimeoutMs "$IOS_SESSION_CREATE_TIMEOUT_MS" \
      --argjson wdaLaunchTimeoutMs "$IOS_WDA_LAUNCH_TIMEOUT_MS" \
      --argjson wdaConnectionTimeoutMs "$IOS_WDA_CONNECTION_TIMEOUT_MS" \
      --argjson signing "$SIGNING_JSON" \
      '{udid:$udid,showXcodeLog:$showXcodeLog,allowProvisioningUpdates:$allowProvisioningUpdates,allowProvisioningDeviceRegistration:$allowProvisioningDeviceRegistration,sessionCreateTimeoutMs:$sessionCreateTimeoutMs,wdaLaunchTimeoutMs:$wdaLaunchTimeoutMs,wdaConnectionTimeoutMs:$wdaConnectionTimeoutMs,signing:$signing}')"

    ARGS_JSON="$(jq -nc \
      --arg query "$QUERY" \
      --arg typing_mode "$TYPING_MODE" \
      --arg country "$COUNTRY" \
      --arg locale "$LOCALE" \
      --argjson limit "$LIMIT" \
      '{query:$query,limit:$limit,typing_mode:$typing_mode}
       + (if $country == "" then {} else {country:$country} end)
       + (if $locale == "" then {} else {locale:$locale} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    cat <<JSON | "$BIN" > "$RAW_OUT"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"appstore.typeahead","session":$SESSION_JSON,"args":$ARGS_JSON,"commit":false}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_ON_EXIT_JSON}}}
JSON

    if jq -e 'select(.id=="wf-1") | .result.isError == true' "$RAW_OUT" >/dev/null; then
      jq -r 'select(.id=="wf-1") | .result.content[]?.text // "appstore-typeahead failed"' "$RAW_OUT" >&2
      exit 1
    fi

    jq -c 'select(.id=="wf-1") | .result.structuredContent' "$RAW_OUT" | jq . > "$OUT_DIR/result.json"
    SCREENSHOT_B64="$(jq -r 'select(.id=="wf-1") | .result.structuredContent.screenshot.data // empty' "$RAW_OUT")"
    UI_SOURCE="$(jq -r 'select(.id=="wf-1") | .result.structuredContent.uiSource.source // empty' "$RAW_OUT")"
    if [[ -n "$SCREENSHOT_B64" ]]; then
      printf '%s' "$SCREENSHOT_B64" | base64 --decode > "$OUT_DIR/screenshot.png"
    fi
    if [[ -n "$UI_SOURCE" ]]; then
      printf '%s' "$UI_SOURCE" > "$OUT_DIR/ui_source.xml"
    fi
    if [[ ! -f "$OUT_DIR/screenshot.png" || ! -f "$OUT_DIR/ui_source.xml" ]]; then
      echo "missing expected App Store artifacts in $OUT_DIR" >&2
      exit 1
    fi
    echo "appstore typeahead saved artifacts to: $OUT_DIR"
    ;;
  appstore-search-results)
    UDID="${1:-}"
    QUERY="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    LIMIT=10
    MAX_SCROLLS=5
    TARGET_APP_NAME=""
    SUBMIT_MODE="suggestion"
    COUNTRY=""
    LOCALE=""
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --limit)
          LIMIT="${2:-10}"
          shift 2
          ;;
        --max-scrolls)
          MAX_SCROLLS="${2:-5}"
          shift 2
          ;;
        --target-app-name)
          TARGET_APP_NAME="${2:-}"
          shift 2
          ;;
        --submit-mode)
          SUBMIT_MODE="${2:-suggestion}"
          shift 2
          ;;
        --country)
          COUNTRY="${2:-}"
          shift 2
          ;;
        --locale)
          LOCALE="${2:-}"
          shift 2
          ;;
        *)
          echo "unknown option for appstore-search-results: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$QUERY" ]]; then
      echo "usage: scripts/ios_tools.sh appstore-search-results <udid> <query> [--out <dir>] [--limit <n>] [--target-app-name <name>] [--max-scrolls <n>] [--submit-mode <suggestion|keyboard>] [--country <cc>] [--locale <locale>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/appstore-results.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    SESSION_JSON="$(jq -nc \
      --arg udid "$UDID" \
      --argjson showXcodeLog "$SHOW_XCODE_LOG_JSON" \
      --argjson allowProvisioningUpdates "$ALLOW_PROVISIONING_UPDATES_JSON" \
      --argjson allowProvisioningDeviceRegistration "$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON" \
      --argjson sessionCreateTimeoutMs "$IOS_SESSION_CREATE_TIMEOUT_MS" \
      --argjson wdaLaunchTimeoutMs "$IOS_WDA_LAUNCH_TIMEOUT_MS" \
      --argjson wdaConnectionTimeoutMs "$IOS_WDA_CONNECTION_TIMEOUT_MS" \
      --argjson signing "$SIGNING_JSON" \
      '{udid:$udid,showXcodeLog:$showXcodeLog,allowProvisioningUpdates:$allowProvisioningUpdates,allowProvisioningDeviceRegistration:$allowProvisioningDeviceRegistration,sessionCreateTimeoutMs:$sessionCreateTimeoutMs,wdaLaunchTimeoutMs:$wdaLaunchTimeoutMs,wdaConnectionTimeoutMs:$wdaConnectionTimeoutMs,signing:$signing}')"

    ARGS_JSON="$(jq -nc \
      --arg query "$QUERY" \
      --arg target_app_name "$TARGET_APP_NAME" \
      --arg submit_mode "$SUBMIT_MODE" \
      --arg country "$COUNTRY" \
      --arg locale "$LOCALE" \
      --argjson limit "$LIMIT" \
      --argjson maxScrolls "$MAX_SCROLLS" \
      '{query:$query,limit:$limit,maxScrolls:$maxScrolls,submit_mode:$submit_mode}
       + (if $target_app_name == "" then {} else {target_app_name:$target_app_name} end)
       + (if $country == "" then {} else {country:$country} end)
       + (if $locale == "" then {} else {locale:$locale} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    cat <<JSON | "$BIN" > "$RAW_OUT"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"appstore.search_results","session":$SESSION_JSON,"args":$ARGS_JSON,"commit":false}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$STOP_APPIUM_ON_EXIT_JSON}}}
JSON

    if jq -e 'select(.id=="wf-1") | .result.isError == true' "$RAW_OUT" >/dev/null; then
      jq -r 'select(.id=="wf-1") | .result.content[]?.text // "appstore-search-results failed"' "$RAW_OUT" >&2
      exit 1
    fi

    jq -c 'select(.id=="wf-1") | .result.structuredContent' "$RAW_OUT" | jq . > "$OUT_DIR/result.json"
    SCREENSHOT_B64="$(jq -r 'select(.id=="wf-1") | .result.structuredContent.screenshot.data // empty' "$RAW_OUT")"
    UI_SOURCE="$(jq -r 'select(.id=="wf-1") | .result.structuredContent.uiSource.source // empty' "$RAW_OUT")"
    if [[ -n "$SCREENSHOT_B64" ]]; then
      printf '%s' "$SCREENSHOT_B64" | base64 --decode > "$OUT_DIR/screenshot.png"
    fi
    if [[ -n "$UI_SOURCE" ]]; then
      printf '%s' "$UI_SOURCE" > "$OUT_DIR/ui_source.xml"
    fi
    if [[ ! -f "$OUT_DIR/screenshot.png" || ! -f "$OUT_DIR/ui_source.xml" ]]; then
      echo "missing expected App Store artifacts in $OUT_DIR" >&2
      exit 1
    fi
    echo "appstore search results saved artifacts to: $OUT_DIR"
    ;;
  appstore-smoke)
    UDID="${1:-}"
    QUERY="${2:-voice notes}"
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh appstore-smoke <udid> [query]" >&2
      exit 1
    fi

    OUT_ROOT="$(mktemp -d /tmp/appstore-smoke.XXXXXX)"
    TYPEAHEAD_OUT="$OUT_ROOT/typeahead"
    RESULTS_OUT="$OUT_ROOT/results"

    "$ROOT/scripts/ios_tools.sh" appstore-typeahead "$UDID" "$QUERY" --out "$TYPEAHEAD_OUT" >/dev/null
    "$ROOT/scripts/ios_tools.sh" appstore-search-results "$UDID" "$QUERY" --out "$RESULTS_OUT" >/dev/null

    TYPEAHEAD_COUNT="$(jq '.suggestions | length' "$TYPEAHEAD_OUT/result.json")"
    RESULTS_COUNT="$(jq '.results | length' "$RESULTS_OUT/result.json")"

    if [[ "$TYPEAHEAD_COUNT" -ge 1 && "$RESULTS_COUNT" -ge 1 ]]; then
      echo "appstore smoke ok: suggestions=$TYPEAHEAD_COUNT results=$RESULTS_COUNT artifacts=$OUT_ROOT"
      exit 0
    fi

    echo "appstore smoke failed: suggestions=$TYPEAHEAD_COUNT results=$RESULTS_COUNT artifacts=$OUT_ROOT" >&2
    exit 1
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    echo "unknown command: $cmd" >&2
    usage
    exit 1
    ;;
esac
