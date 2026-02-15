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
EOF
}

worker_bin() {
  local bin="$ROOT/target/release/rzn_ios_tools_worker"
  cargo build -p rzn_ios_tools_worker --release >/dev/null
  echo "$bin"
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
  help|-h|--help)
    usage
    ;;
  *)
    echo "unknown command: $cmd" >&2
    usage
    exit 1
    ;;
esac
