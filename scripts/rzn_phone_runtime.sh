#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKER_BIN="$ROOT/libexec/rzn-phone-worker"
VERSION_FILE="$ROOT/VERSION"
WORKFLOW_PACK_VERSION_FILE="$ROOT/WORKFLOW_PACK_VERSION"
UPDATE_SOURCE_FILE="$ROOT/UPDATE_SOURCE"

export RZN_PLUGIN_DIR="$ROOT"
export CLAUDE_PLUGIN_ROOT="$ROOT"

usage() {
  cat <<'EOF'
Usage: rzn-phone <command> [args]

Commands:
  worker                               Run the MCP worker on stdio.
  doctor                               Check local iOS/Appium prerequisites.
  devices                              List connected physical iPhones.
  version                              Print runtime and workflow pack versions.
  info                                 Print install metadata.
  workflow list                        List installed workflows.
  workflow run <name> --udid <udid> [--args-json <json|@file>] [--commit 0|1]
                                       [--disconnect-on-finish 0|1] [--stop-appium-on-finish 0|1]
                                       [--background-on-exit 0|1] [--lock-device-on-exit 0|1]
  workflows update [--source <path|url>] [--version <version>]
                                       Refresh installed workflows/examples from a release pack.
  workflows path                       Print the installed workflow directory.
  examples path                        Print the installed examples directory.
EOF
}

fail() {
  echo "rzn-phone: $*" >&2
  exit 1
}

read_file_trimmed() {
  local path="$1"
  if [[ -f "$path" ]]; then
    tr -d '\n' <"$path"
  fi
}

runtime_version() {
  read_file_trimmed "$VERSION_FILE"
}

workflow_pack_version() {
  read_file_trimmed "$WORKFLOW_PACK_VERSION_FILE"
}

default_update_source() {
  read_file_trimmed "$UPDATE_SOURCE_FILE"
}

ensure_worker() {
  [[ -x "$WORKER_BIN" ]] || fail "worker binary is missing at $WORKER_BIN"
}

bool_json() {
  local value="${1:-0}"
  case "$value" in
    1|true|TRUE|yes|YES)
      printf 'true\n'
      ;;
    *)
      printf 'false\n'
      ;;
  esac
}

read_json_input() {
  local raw="${1:-}"
  if [[ -z "$raw" ]]; then
    printf '{}\n'
    return 0
  fi
  if [[ "${raw#@}" != "$raw" ]]; then
    cat "${raw#@}"
    return 0
  fi
  printf '%s\n' "$raw"
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
}

jsonrpc_tool_call() {
  local request_id="$1"
  local request_json="$2"
  ensure_worker
  RZN_PHONE_REQUEST_JSON="$request_json" python3 - "$WORKER_BIN" "$request_id" <<'PY'
import json
import os
import subprocess
import sys

worker = sys.argv[1]
request_id = sys.argv[2]
request = json.loads(os.environ["RZN_PHONE_REQUEST_JSON"])
payloads = [
    {
        "jsonrpc": "2.0",
        "id": "init-1",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "rzn-phone-runtime", "version": "0.1"},
        },
    },
    {"jsonrpc": "2.0", "method": "initialized", "params": {}},
    request,
]
proc = subprocess.run(
    [worker],
    input="\n".join(json.dumps(item, separators=(",", ":")) for item in payloads) + "\n",
    text=True,
    capture_output=True,
)
if proc.returncode != 0:
    sys.stderr.write(proc.stderr)
    raise SystemExit(proc.returncode)

responses = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
response = next((item for item in responses if item.get("id") == request_id), None)
if response is None:
    sys.stderr.write(proc.stdout)
    raise SystemExit("missing tool response")

if "error" in response:
    json.dump(response["error"], sys.stderr, indent=2)
    sys.stderr.write("\n")
    raise SystemExit(1)

result = response.get("result", {})
if isinstance(result, dict) and result.get("isError"):
    payload = result.get("structuredContent", result)
    json.dump(payload, sys.stderr, indent=2)
    sys.stderr.write("\n")
    raise SystemExit(1)

payload = result.get("structuredContent", result)
json.dump(payload, sys.stdout, indent=2)
sys.stdout.write("\n")
PY
}

build_simple_tool_request() {
  local request_id="$1"
  local tool_name="$2"
  local arguments_json="${3:-}"
  if [[ -z "$arguments_json" ]]; then
    arguments_json='{}'
  fi
  python3 - "$request_id" "$tool_name" "$arguments_json" <<'PY'
import json
import sys

request_id, tool_name, arguments_json = sys.argv[1:]
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": json.loads(arguments_json)},
        }
    )
)
PY
}

resolve_archive_ref() {
  local source="$1"
  local version="$2"
  local archive_name="rzn-phone-workflows-${version}.tar.gz"
  case "$source" in
    http://*|https://*|file://*)
      if [[ "$source" == *.tar.gz ]]; then
        printf '%s\n' "$source"
      else
        printf '%s/%s\n' "${source%/}" "$archive_name"
      fi
      ;;
    *)
      local local_source="$source"
      if [[ "$source" == "~" || "$source" == ~/* ]]; then
        local_source="${source/#\~/$HOME}"
      fi
      if [[ -d "$local_source" ]]; then
        printf '%s/%s\n' "$local_source" "$archive_name"
      else
        printf '%s\n' "$local_source"
      fi
      ;;
  esac
}

read_source_text() {
  local ref="$1"
  case "$ref" in
    http://*|https://*)
      curl -fsSL "$ref"
      ;;
    file://*)
      cat "${ref#file://}"
      ;;
    *)
      cat "$ref"
      ;;
  esac
}

stage_workflow_archive() {
  local ref="$1"
  local target="$2"
  case "$ref" in
    http://*|https://*)
      curl -fsSL "$ref" -o "$target"
      ;;
    file://*)
      cp "${ref#file://}" "$target"
      ;;
    *)
      cp "$ref" "$target"
      ;;
  esac
}

update_workflows() {
  local source="${1:-}"
  local version="${2:-}"

  if [[ -z "$source" ]]; then
    source="$(default_update_source)"
  fi
  [[ -n "$source" ]] || fail "no workflow update source configured; pass --source"

  if [[ -z "$version" ]]; then
    case "$source" in
      http://*|https://*|file://*)
        version="$(read_source_text "${source%/}/VERSION" 2>/dev/null | tr -d '\n' || true)"
        ;;
      *)
        if [[ -d "$source" && -f "$source/VERSION" ]]; then
          version="$(tr -d '\n' <"$source/VERSION")"
        fi
        ;;
    esac
  fi
  [[ -n "$version" ]] || fail "unable to determine workflow pack version from source; pass --version"

  local archive_ref
  archive_ref="$(resolve_archive_ref "$source" "$version")"
  local tmpdir
  tmpdir="$(mktemp -d /tmp/rzn-phone-workflows.XXXXXX)"
  trap 'rm -rf "$tmpdir"' RETURN

  local archive_path="$tmpdir/workflows.tar.gz"
  stage_workflow_archive "$archive_ref" "$archive_path"
  tar -xzf "$archive_path" -C "$tmpdir"
  local pack_root="$tmpdir/rzn-phone-workflows"
  [[ -d "$pack_root/resources/workflows" ]] || fail "workflow pack is missing resources/workflows"
  [[ -d "$pack_root/examples" ]] || fail "workflow pack is missing examples"

  rm -rf "$ROOT/resources/workflows" "$ROOT/resources/systems" "$ROOT/examples"
  mkdir -p "$ROOT/resources"
  cp -R "$pack_root/resources/workflows" "$ROOT/resources/workflows"
  cp -R "$pack_root/resources/systems" "$ROOT/resources/systems"
  cp -R "$pack_root/examples" "$ROOT/examples"
  if [[ -f "$pack_root/VERSION" ]]; then
    cp "$pack_root/VERSION" "$WORKFLOW_PACK_VERSION_FILE"
  fi
  printf '%s\n' "$source" >"$UPDATE_SOURCE_FILE"

  local workflow_count
  workflow_count="$(find "$ROOT/resources/workflows" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d ' ')"
  cat <<EOF
Updated workflows from $source
Workflow pack version: $(workflow_pack_version)
Installed workflows: $workflow_count
EOF
}

print_info() {
  python3 - "$ROOT" "$(runtime_version)" "$(workflow_pack_version)" "$(default_update_source)" <<'PY'
import json
import sys

root, runtime_version, workflow_pack_version, update_source = sys.argv[1:]
print(
    json.dumps(
        {
            "root": root,
            "runtimeVersion": runtime_version,
            "workflowPackVersion": workflow_pack_version or runtime_version,
            "updateSource": update_source,
            "worker": f"{root}/libexec/rzn-phone-worker",
            "workflowDir": f"{root}/resources/workflows",
            "examplesDir": f"{root}/examples",
        },
        indent=2,
    )
)
PY
}

if [[ "$#" -eq 0 ]]; then
  usage >&2
  exit 1
fi

cmd="$1"
shift

case "$cmd" in
  worker)
    ensure_worker
    exec "$WORKER_BIN" "$@"
    ;;
  doctor)
    REQUEST="$(build_simple_tool_request "doctor-1" "ios.env.doctor" "{}")"
    jsonrpc_tool_call "doctor-1" "$REQUEST"
    ;;
  devices)
    REQUEST="$(build_simple_tool_request "devices-1" "ios.device.list" '{"includeSimulators":false}')"
    jsonrpc_tool_call "devices-1" "$REQUEST"
    ;;
  version)
    python3 - "$(runtime_version)" "$(workflow_pack_version)" <<'PY'
import json
import sys

runtime_version, workflow_pack_version = sys.argv[1:]
print(
    json.dumps(
        {
            "runtimeVersion": runtime_version,
            "workflowPackVersion": workflow_pack_version or runtime_version,
        },
        indent=2,
    )
)
PY
    ;;
  info)
    print_info
    ;;
  workflow)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      list)
        REQUEST="$(build_simple_tool_request "workflow-list-1" "ios.workflow.list" "{}")"
        jsonrpc_tool_call "workflow-list-1" "$REQUEST"
        ;;
      run)
        WORKFLOW_NAME="${1:-}"
        [[ -n "$WORKFLOW_NAME" ]] || fail "workflow run requires a workflow name"
        shift
        UDID=""
        ARGS_JSON="{}"
        COMMIT="0"
        DISCONNECT_ON_FINISH="1"
        STOP_APPIUM_ON_FINISH="0"
        BACKGROUND_ON_EXIT="0"
        LOCK_DEVICE_ON_EXIT="0"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --udid)
              UDID="${2:-}"
              shift 2
              ;;
            --args-json)
              ARGS_JSON="$(read_json_input "${2:-}")"
              shift 2
              ;;
            --commit)
              COMMIT="${2:-0}"
              shift 2
              ;;
            --disconnect-on-finish)
              DISCONNECT_ON_FINISH="${2:-1}"
              shift 2
              ;;
            --stop-appium-on-finish)
              STOP_APPIUM_ON_FINISH="${2:-0}"
              shift 2
              ;;
            --background-on-exit)
              BACKGROUND_ON_EXIT="${2:-0}"
              shift 2
              ;;
            --lock-device-on-exit)
              LOCK_DEVICE_ON_EXIT="${2:-0}"
              shift 2
              ;;
            *)
              fail "unknown workflow run argument: $1"
              ;;
          esac
        done
        [[ -n "$UDID" ]] || fail "workflow run requires --udid"
        load_ios_session_env
        SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
        ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
        ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
        COMMIT_JSON="$(bool_json "$COMMIT")"
        DISCONNECT_ON_FINISH_JSON="$(bool_json "$DISCONNECT_ON_FINISH")"
        STOP_APPIUM_ON_FINISH_JSON="$(bool_json "$STOP_APPIUM_ON_FINISH")"
        BACKGROUND_ON_EXIT_JSON="$(bool_json "$BACKGROUND_ON_EXIT")"
        LOCK_DEVICE_ON_EXIT_JSON="$(bool_json "$LOCK_DEVICE_ON_EXIT")"
        REQUEST="$(
          python3 - "$WORKFLOW_NAME" "$UDID" "$ARGS_JSON" "$COMMIT_JSON" "$DISCONNECT_ON_FINISH_JSON" "$STOP_APPIUM_ON_FINISH_JSON" "$BACKGROUND_ON_EXIT_JSON" "$LOCK_DEVICE_ON_EXIT_JSON" "$IOS_XCODE_ORG_ID" "$IOS_XCODE_SIGNING_ID" "$IOS_UPDATED_WDA_BUNDLE_ID" "$SHOW_XCODE_LOG_JSON" "$ALLOW_PROVISIONING_UPDATES_JSON" "$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON" "$IOS_SESSION_CREATE_TIMEOUT_MS" "$IOS_WDA_LAUNCH_TIMEOUT_MS" "$IOS_WDA_CONNECTION_TIMEOUT_MS" <<'PY'
import json
import sys

(
    workflow_name,
    udid,
    args_json,
    commit_json,
    disconnect_json,
    stop_appium_json,
    background_json,
    lock_json,
    xcode_org_id,
    xcode_signing_id,
    updated_wda_bundle_id,
    show_xcode_log_json,
    allow_provisioning_updates_json,
    allow_provisioning_device_registration_json,
    session_create_timeout_ms,
    wda_launch_timeout_ms,
    wda_connection_timeout_ms,
) = sys.argv[1:]

signing = {}
if xcode_org_id or xcode_signing_id or updated_wda_bundle_id:
    signing = {
        "xcodeOrgId": xcode_org_id,
        "xcodeSigningId": xcode_signing_id,
        "updatedWDABundleId": updated_wda_bundle_id,
    }

request = {
    "jsonrpc": "2.0",
    "id": "workflow-run-1",
    "method": "tools/call",
    "params": {
        "name": "ios.workflow.run",
        "arguments": {
            "name": workflow_name,
            "session": {
                "udid": udid,
                "showXcodeLog": json.loads(show_xcode_log_json),
                "allowProvisioningUpdates": json.loads(allow_provisioning_updates_json),
                "allowProvisioningDeviceRegistration": json.loads(allow_provisioning_device_registration_json),
                "sessionCreateTimeoutMs": int(session_create_timeout_ms),
                "wdaLaunchTimeoutMs": int(wda_launch_timeout_ms),
                "wdaConnectionTimeoutMs": int(wda_connection_timeout_ms),
                "signing": signing,
            },
            "args": json.loads(args_json),
            "commit": json.loads(commit_json),
            "disconnectOnFinish": json.loads(disconnect_json),
            "stopAppiumOnFinish": json.loads(stop_appium_json),
            "backgroundAppOnFinish": json.loads(background_json),
            "lockDeviceOnFinish": json.loads(lock_json),
        },
    },
}
print(json.dumps(request))
PY
        )"
        jsonrpc_tool_call "workflow-run-1" "$REQUEST"
        ;;
      *)
        fail "unknown workflow subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  workflows)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      update)
        SOURCE=""
        VERSION_OVERRIDE=""
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --source)
              SOURCE="${2:-}"
              shift 2
              ;;
            --version)
              VERSION_OVERRIDE="${2:-}"
              shift 2
              ;;
            *)
              fail "unknown workflows update argument: $1"
              ;;
          esac
        done
        update_workflows "$SOURCE" "$VERSION_OVERRIDE"
        ;;
      path)
        printf '%s\n' "$ROOT/resources/workflows"
        ;;
      *)
        fail "unknown workflows subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  examples)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      path)
        printf '%s\n' "$ROOT/examples"
        ;;
      *)
        fail "unknown examples subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    fail "unknown command: $cmd"
    ;;
esac
