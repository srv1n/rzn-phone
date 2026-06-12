#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SELF_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
CLI_HELPER="$ROOT/scripts/rzn_phone_cli.py"
RELEASE_ARCHIVE_HELPER="$ROOT/scripts/release_archive.py"
RELEASE_PUBLIC_KEY="$ROOT/scripts/rzn_phone_release_ed25519.pub"
WORKER_BIN="$ROOT/libexec/rzn-phone-worker"
TARGET_WORKER_BIN="$ROOT/target/release/rzn-phone-worker"
VERSION_FILE="$ROOT/VERSION"
WORKFLOW_PACK_VERSION_FILE="$ROOT/WORKFLOW_PACK_VERSION"
UPDATE_SOURCE_FILE="$ROOT/UPDATE_SOURCE"

if [[ -x "$TARGET_WORKER_BIN" && ( ! -x "$WORKER_BIN" || "$TARGET_WORKER_BIN" -nt "$WORKER_BIN" ) ]]; then
  WORKER_BIN="$TARGET_WORKER_BIN"
fi

export RZN_PLUGIN_DIR="$ROOT"
export CLAUDE_PLUGIN_ROOT="$ROOT"

usage() {
  cat <<'EOF'
Usage: rzn-phone <command> [args]

Commands:
  worker                               Run the MCP worker on stdio.
  doctor                               Check local iOS/Appium prerequisites.
  devices [--json]                     List connected physical iPhones.
  status                               Show current Appium/session status, including fast-mode state when present.
  shutdown [--stop-appium 0|1] [--background-on-exit 0|1] [--lock-device-on-exit 0|1]
                                       Close persisted session state and optionally stop Appium/WDA.
  version                              Print runtime and workflow pack versions.
  info                                 Print install metadata.
  run <ref>|<system> <workflow> --udid <udid> [--args-json <json|@file>] [--commit 0|1]
                                       [--dry-run] [--disconnect-on-finish 0|1] [--stop-appium-on-finish 0|1] [--fast 0|1]
                                       [--background-on-exit 0|1] [--lock-device-on-exit 0|1] [--json]
  list [system|query] [--family <family>] [--search <text>] [--surface <surface>] [--has-input <name>]
                                       [--mutating[=0|1]] [--favorites] [--compact] [--json]
                                       List installed systems and workflows. If the positional value is not an exact system id, it is treated as a search query.
  show <ref>|<system> <workflow> [--example] [--json]
                                       Show one workflow or tool definition.
  workflow list [system|query] [--family <family>] [--search <text>] [--surface <surface>] [--has-input <name>]
                                       [--mutating[=0|1]] [--favorites] [--compact] [--json]
                                       List installed systems and workflows. Positional values first try exact system ids, then fall back to search.
  workflow show <ref>|<system> <workflow> [--example] [--json]
                                       Show one workflow definition. Canonical refs use system/workflow.
  capability list [--json]             Show the two-tier capability taxonomy used by the worker.
  tool list [--direct] [--search <text>] [--family <family>] [--tier <tier>] [--json]
                                       List worker tools. Use --direct to hide workflow/script runners.
  tools [--direct] [--search <text>] [--family <family>] [--tier <tier>] [--json]
                                       Alias for tool list.
  tool show <tool-name> [--json]       Show one worker tool definition.
  tool call <tool-name> [--args-json <json|@file>]
                                       Call any worker tool directly.
  recent [--limit <n>] [--json]        Show recent workflow runs.
  rerun <n>                            Rerun the nth recent workflow entry.
  favorite add <ref>                   Add a workflow to favorites.
  favorite remove <ref>                Remove a workflow from favorites.
  favorite list [--json]               List favorite workflows.
  favorites [--json]                   Alias for favorite list.
  completion <bash|zsh>                Print shell completion script.
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

validate_release_version() {
  local version="$1"
  if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?(\+[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]]; then
    fail "invalid release version: $version"
  fi
}

ensure_worker() {
  [[ -x "$WORKER_BIN" ]] || fail "worker binary is missing at $WORKER_BIN"
}

python_cli() {
  python3 "$CLI_HELPER" "$@"
}

default_pretty_output() {
  [[ -t 1 ]] && printf '1\n' || printf '0\n'
}

render_devices_output() {
  local output_json="${1:-0}"
  local pretty="${2:-0}"
  if [[ "$output_json" == "1" ]]; then
    cat
  else
    python_cli devices $([[ "$pretty" == "1" ]] && printf '%s' "--pretty")
  fi
}

render_workflow_list_output() {
  local output_json="$1"
  local pretty="$2"
  local compact="$3"
  local system="${4:-}"
  local search="${5:-}"
  local surface="${6:-}"
  local has_input="${7:-}"
  local mutating="${8:-}"
  local favorites="${9:-0}"
  local args=()
  [[ "$output_json" == "1" ]] && args+=(--json)
  [[ "$pretty" == "1" ]] && args+=(--pretty)
  [[ "$compact" == "1" ]] && args+=(--compact)
  [[ -n "$system" ]] && args+=(--system "$system")
  [[ -n "$search" ]] && args+=(--search "$search")
  [[ -n "$surface" ]] && args+=(--surface "$surface")
  [[ -n "$has_input" ]] && args+=(--has-input "$has_input")
  [[ -n "$mutating" ]] && args+=(--mutating "$mutating")
  [[ "$favorites" == "1" ]] && args+=(--favorites)
  python_cli workflow-list "${args[@]}"
}

render_workflow_show_output() {
  local output_json="$1"
  local pretty="$2"
  local example="$3"
  local args=()
  [[ "$output_json" == "1" ]] && args+=(--json)
  [[ "$pretty" == "1" ]] && args+=(--pretty)
  [[ "$example" == "1" ]] && args+=(--example)
  python_cli workflow-show "${args[@]}"
}

render_tool_list_output() {
  local output_json="$1"
  local pretty="$2"
  local search="${3:-}"
  local family="${4:-}"
  local tier="${5:-}"
  local args=()
  [[ "$output_json" == "1" ]] && args+=(--json)
  [[ "$pretty" == "1" ]] && args+=(--pretty)
  [[ -n "$search" ]] && args+=(--search "$search")
  [[ -n "$family" ]] && args+=(--family "$family")
  [[ -n "$tier" ]] && args+=(--tier "$tier")
  python_cli tool-list "${args[@]}"
}

render_tool_show_output() {
  local output_json="$1"
  local pretty="$2"
  local args=()
  [[ "$output_json" == "1" ]] && args+=(--json)
  [[ "$pretty" == "1" ]] && args+=(--pretty)
  python_cli tool-show "${args[@]}"
}

render_capability_output() {
  local output_json="$1"
  local pretty="$2"
  local args=()
  [[ "$output_json" == "1" ]] && args+=(--json)
  [[ "$pretty" == "1" ]] && args+=(--pretty)
  python_cli capability-list "${args[@]}"
}

suggest_top_level_command() {
  local query="$1"
  local suggestions
  suggestions="$(python_cli suggest-command "$query")"
  if [[ -n "$suggestions" ]]; then
    {
      printf 'rzn-phone: unknown command: %s\n' "$query"
      printf 'Did you mean:\n'
      while IFS= read -r line; do
        [[ -n "$line" ]] && printf '  - %s\n' "$line"
      done <<<"$suggestions"
    } >&2
    exit 1
  fi
  fail "unknown command: $query"
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

runtime_state_file_path() {
  if [[ -n "${RZN_IOS_RUNTIME_STATE_FILE:-}" ]]; then
    printf '%s\n' "$RZN_IOS_RUNTIME_STATE_FILE"
  else
    printf '%s\n' "${HOME:-/tmp}/.rzn-phone/runtime-state.json"
  fi
}

runtime_cache_ttl_secs() {
  printf '%s\n' "${RZN_IOS_RUNTIME_CACHE_TTL_SECS:-300}"
}

smart_cache_default_enabled() {
  case "${RZN_IOS_SMART_CACHE:-1}" in
    0|false|FALSE|no|NO)
      printf '0\n'
      ;;
    *)
      printf '1\n'
      ;;
  esac
}

runtime_state_touch_ms() {
  local state_file="$1"
  python3 - "$state_file" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], "r", encoding="utf-8") as fh:
        payload = json.load(fh)
except Exception:
    raise SystemExit(0)

value = payload.get("last_used_epoch_ms")
if value is not None:
    print(value)
PY
}

runtime_cache_warm_for_udid() {
  local udid="$1"
  local state_file
  state_file="$(runtime_state_file_path)"
  [[ -f "$state_file" ]] || return 1

  python3 - "$state_file" "$udid" <<'PY'
import json
import sys

state_file, want_udid = sys.argv[1:]

try:
    with open(state_file, "r", encoding="utf-8") as fh:
        payload = json.load(fh)
except Exception:
    raise SystemExit(1)

session = payload.get("session") or {}
session_id = session.get("session_id") or session.get("sessionId")
session_udid = session.get("udid") or payload.get("last_udid")

if session_id and session_udid == want_udid:
    raise SystemExit(0)

raise SystemExit(1)
PY
}

maybe_print_cold_start_notice() {
  local smart_cache_active="$1"
  local udid="$2"
  local output_json="$3"

  [[ "$output_json" == "1" ]] && return 0
  [[ -t 2 ]] || return 0

  if [[ "$smart_cache_active" == "1" ]] && runtime_cache_warm_for_udid "$udid"; then
    return 0
  fi

  if [[ "$smart_cache_active" == "1" ]]; then
    printf '%s\n' "Preparing device session. Cold starts can take a few seconds; once this session is warm, later runs are faster." >&2
  else
    printf '%s\n' "Preparing device session. This run is starting cold, so it can take a few seconds." >&2
  fi
}

maybe_cleanup_stale_runtime_cache() {
  local state_file ttl_secs touch_ms now_ms age_ms
  state_file="$(runtime_state_file_path)"
  [[ -f "$state_file" ]] || return 0

  touch_ms="$(runtime_state_touch_ms "$state_file")"
  if [[ -z "$touch_ms" ]]; then
    rm -f "$state_file"
    return 0
  fi

  ttl_secs="$(runtime_cache_ttl_secs)"
  now_ms="$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)"
  age_ms=$((now_ms - touch_ms))
  if (( age_ms < ttl_secs * 1000 )); then
    return 0
  fi

  RZN_IOS_PERSIST_RUNTIME=1 "$SELF_PATH" shutdown --stop-appium 1 >/dev/null 2>&1 || true
  rm -f "$state_file"
}

arm_runtime_cache_reaper() {
  local state_file ttl_secs touch_ms
  state_file="$(runtime_state_file_path)"
  [[ -f "$state_file" ]] || return 0

  touch_ms="$(runtime_state_touch_ms "$state_file")"
  [[ -n "$touch_ms" ]] || return 0
  ttl_secs="$(runtime_cache_ttl_secs)"

  nohup python3 - "$state_file" "$touch_ms" "$ttl_secs" "$SELF_PATH" <<'PY' >/dev/null 2>&1 &
import json
import os
import subprocess
import sys
import time

state_file, touch_ms, ttl_secs, self_path = sys.argv[1:]
time.sleep(max(int(ttl_secs), 0))

try:
    with open(state_file, "r", encoding="utf-8") as fh:
        payload = json.load(fh)
except Exception:
    raise SystemExit(0)

if str(payload.get("last_used_epoch_ms", "")) != touch_ms:
    raise SystemExit(0)

env = os.environ.copy()
env["RZN_IOS_PERSIST_RUNTIME"] = "1"
subprocess.run(
    [self_path, "shutdown", "--stop-appium", "1"],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    env=env,
    check=False,
)
PY
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

jsonrpc_request() {
  local request_id="$1"
  local request_json="$2"
  local response_mode="${3:-raw}"
  ensure_worker
  RZN_PHONE_REQUEST_JSON="$request_json" \
  RZN_PHONE_RESPONSE_MODE="$response_mode" \
  python3 - "$WORKER_BIN" "$request_id" <<'PY'
import json
import os
import subprocess
import sys

def format_cli_presentation(payload):
    if not isinstance(payload, dict):
        return None
    presentation = payload.get("_presentation")
    if not isinstance(presentation, dict):
        return None
    cli = presentation.get("cli")
    if not isinstance(cli, dict):
        return None
    if cli.get("type") != "result_list":
        return None
    items = cli.get("items")
    if not isinstance(items, list):
        return None
    title = cli.get("title")
    if isinstance(title, str) and title.strip():
        lines = [title.strip()]
    else:
        lines = ["Results"]
    if not items:
        lines.append("No results found.")
        return "\n".join(lines) + "\n"
    title_field = str(cli.get("titleField") or "title")
    url_field = str(cli.get("urlField") or "url")
    snippet_field = str(cli.get("snippetField") or "snippet")
    for idx, item in enumerate(items, start=1):
        if not isinstance(item, dict):
            continue
        item_title = str(item.get(title_field) or "").strip() or "(untitled)"
        url = str(item.get(url_field) or "").strip()
        snippet = str(item.get(snippet_field) or "").strip()
        lines.append(f"{idx}. {item_title}")
        if url:
            lines.append(f"   {url}")
        if snippet:
            lines.append(f"   {snippet}")
    footer = cli.get("footer")
    if isinstance(footer, str) and footer.strip():
        lines.append(footer.strip())
    return "\n".join(lines) + "\n"

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

# MCP stdio messages are LF-delimited JSON objects.
# Do not use splitlines(): it also splits on Unicode line separators like U+2028/U+2029,
# which can legitimately appear inside app text captured in JSON strings.
responses = [json.loads(line) for line in proc.stdout.split("\n") if line.strip()]
response = next((item for item in responses if item.get("id") == request_id), None)
if response is None:
    sys.stderr.write(proc.stdout)
    raise SystemExit("missing tool response")

if "error" in response:
    json.dump(response["error"], sys.stderr, indent=2)
    sys.stderr.write("\n")
    raise SystemExit(1)

result = response.get("result", {})
if os.environ.get("RZN_PHONE_RESPONSE_MODE") == "tool":
    if isinstance(result, dict) and result.get("isError"):
        payload = result.get("structuredContent", result)
        json.dump(payload, sys.stderr, indent=2)
        sys.stderr.write("\n")
        raise SystemExit(1)
    payload = result.get("structuredContent", result)
else:
    payload = result

if os.environ.get("RZN_PHONE_SHOW_TIMINGS") == "1":
    timings = payload.get("timings") if isinstance(payload, dict) else None
    if isinstance(timings, dict):
        total = timings.get("totalDurationMs")
        steps = timings.get("steps") or []
        if total is not None:
            sys.stderr.write(f"timings total={total}ms\n")
        for step in steps:
            if not isinstance(step, dict):
                continue
            idx = step.get("step", "?")
            tool = step.get("tool", "?")
            duration = step.get("durationMs", 0)
            status = "ok" if step.get("ok") else "fail"
            if step.get("skipped"):
                status = "skip"
            sys.stderr.write(f"  step {idx}: {tool} {duration}ms [{status}]\n")

pretty = None
if os.environ.get("RZN_PHONE_PRETTY_WORKFLOW") == "1":
    pretty = format_cli_presentation(payload)

if pretty is not None:
    sys.stdout.write(pretty)
else:
    json.dump(payload, sys.stdout, indent=2)
    sys.stdout.write("\n")
PY
}

jsonrpc_tool_call() {
  local request_id="$1"
  local request_json="$2"
  jsonrpc_request "$request_id" "$request_json" tool
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

build_method_request() {
  local request_id="$1"
  local method_name="$2"
  local params_json="${3:-}"
  if [[ -z "$params_json" ]]; then
    params_json='{}'
  fi
  python3 - "$request_id" "$method_name" "$params_json" <<'PY'
import json
import sys

request_id, method_name, params_json = sys.argv[1:]
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method_name,
            "params": json.loads(params_json),
        }
    )
)
PY
}

normalize_workflow_ref() {
  local first="$1"
  local second="${2:-}"
  python3 - "$first" "$second" <<'PY'
import sys

first = sys.argv[1].strip()
second = sys.argv[2].strip()

def canonicalize(value: str) -> str:
    value = value.strip().replace("\\", "/")
    if "/" in value:
        system, workflow = value.split("/", 1)
    elif "." in value:
        system, workflow = value.split(".", 1)
    else:
        return value
    system = system.strip(" /.")
    workflow = workflow.strip(" /.")
    return f"{system}/{workflow}" if system and workflow else value

if second:
    first = first.strip(" /.")
    second = second.strip(" /.")
    print(f"{first}/{second}")
else:
    print(canonicalize(first))
PY
}

workflow_catalog_json() {
  local system_filter="${1:-}"
  local family_filter="${2:-}"
  local args_json
  args_json="$(python3 - "$system_filter" "$family_filter" <<'PY'
import json
import sys

payload = {}
if sys.argv[1]:
    payload["system"] = sys.argv[1]
if sys.argv[2]:
    payload["family"] = sys.argv[2]
print(json.dumps(payload))
PY
)"
  local request
  request="$(build_simple_tool_request "workflow-list-1" "ios.workflow.list" "$args_json")"
  jsonrpc_tool_call "workflow-list-1" "$request"
}

parse_workflow_list_args() {
  SYSTEM_FILTER=""
  FAMILY_FILTER=""
  SEARCH_FILTER=""
  SURFACE_FILTER=""
  HAS_INPUT_FILTER=""
  MUTATING_FILTER=""
  FAVORITES_ONLY="0"
  COMPACT_VIEW="0"
  OUTPUT_JSON="0"
  OUTPUT_PRETTY="$(default_pretty_output)"
  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --family)
        shift || fail "workflow list requires a value after --family"
        [[ "$#" -gt 0 ]] || fail "workflow list requires a value after --family"
        FAMILY_FILTER="$1"
        shift
        ;;
      --search)
        shift || fail "workflow list requires a value after --search"
        [[ "$#" -gt 0 ]] || fail "workflow list requires a value after --search"
        SEARCH_FILTER="$1"
        shift
        ;;
      --surface)
        shift || fail "workflow list requires a value after --surface"
        [[ "$#" -gt 0 ]] || fail "workflow list requires a value after --surface"
        SURFACE_FILTER="$1"
        shift
        ;;
      --has-input)
        shift || fail "workflow list requires a value after --has-input"
        [[ "$#" -gt 0 ]] || fail "workflow list requires a value after --has-input"
        HAS_INPUT_FILTER="$1"
        shift
        ;;
      --mutating)
        if [[ "$#" -ge 2 && "${2:0:1}" != "-" ]]; then
          MUTATING_FILTER="$2"
          shift 2
        else
          MUTATING_FILTER="1"
          shift
        fi
        ;;
      --favorites)
        FAVORITES_ONLY="1"
        shift
        ;;
      --compact)
        COMPACT_VIEW="1"
        OUTPUT_PRETTY="1"
        shift
        ;;
      --json)
        OUTPUT_JSON="1"
        OUTPUT_PRETTY="0"
        shift
        ;;
      --pretty)
        OUTPUT_PRETTY="1"
        shift
        ;;
      -*)
        fail "unknown workflow list flag: $1"
        ;;
      *)
        [[ -z "$SYSTEM_FILTER" ]] || fail "workflow list accepts at most one positional system filter"
        SYSTEM_FILTER="$1"
        shift
        ;;
    esac
  done
}

capability_catalog_json() {
  local request
  request="$(build_simple_tool_request "capability-list-1" "ios.capability.list" "{}")"
  jsonrpc_tool_call "capability-list-1" "$request"
}

tool_catalog_json() {
  local request
  request="$(build_method_request "tools-list-1" "tools/list" "{}")"
  jsonrpc_request "tools-list-1" "$request" raw
}

select_workflow_json() {
  local workflow_ref="$1"
  python_cli workflow-select "$workflow_ref"
}

select_tool_json() {
  local tool_name="$1"
  python_cli tool-select "$tool_name"
}

filter_tool_catalog_json() {
  local mode="$1"
  python3 -c '
import json
import sys

mode = sys.argv[1]
payload = json.load(sys.stdin)
tools = payload.get("tools", [])

if mode == "direct":
    tools = [
        tool for tool in tools
        if tool.get("name") not in {"ios.capability.list", "ios.workflow.list", "ios.workflow.run", "ios.script.run"}
    ]

json.dump({"tools": tools}, sys.stdout)
sys.stdout.write("\n")
' "$mode"
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

read_source_to_file() {
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

resolve_sha_ref() {
  local archive_ref="$1"
  printf '%s.sha256\n' "$archive_ref"
}

resolve_sig_ref() {
  local archive_ref="$1"
  printf '%s.sig\n' "$archive_ref"
}

is_remote_ref() {
  case "$1" in
    http://*|https://*)
      return 0
      ;;
    *)
      return 1
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
  validate_release_version "$version"
  [[ -f "$RELEASE_ARCHIVE_HELPER" ]] || fail "release archive helper is missing at $RELEASE_ARCHIVE_HELPER"

  local archive_ref
  archive_ref="$(resolve_archive_ref "$source" "$version")"
  local tmpdir
  tmpdir="$(mktemp -d /tmp/rzn-phone-workflows.XXXXXX)"
  trap 'rm -rf "$tmpdir"' RETURN

  local archive_name
  archive_name="$(basename "${archive_ref%%\?*}")"
  [[ -n "$archive_name" && "$archive_name" == *.tar.gz ]] || fail "workflow archive must be a .tar.gz file: $archive_ref"
  local archive_path="$tmpdir/$archive_name"
  local sha_path="$tmpdir/$archive_name.sha256"
  local sig_path="$tmpdir/$archive_name.sig"
  read_source_to_file "$archive_ref" "$archive_path"
  read_source_to_file "$(resolve_sha_ref "$archive_ref")" "$sha_path"
  if is_remote_ref "$archive_ref"; then
    read_source_to_file "$(resolve_sig_ref "$archive_ref")" "$sig_path"
    python3 "$RELEASE_ARCHIVE_HELPER" verify-signature --archive "$archive_path" --signature "$sig_path" --public-key "$RELEASE_PUBLIC_KEY"
  fi
  python3 "$RELEASE_ARCHIVE_HELPER" verify-sha256 --archive "$archive_path" --sha256 "$sha_path"
  python3 "$RELEASE_ARCHIVE_HELPER" safe-extract --archive "$archive_path" --dest "$tmpdir" --root-name rzn-phone-workflows
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

WORKFLOW_REF=""
WORKFLOW_PARSE_REST=()

parse_workflow_ref_args() {
  local context="$1"
  shift

  local first="${1:-}"
  [[ -n "$first" ]] || fail "$context requires a workflow ref"
  shift || true

  local second=""
  if [[ "$#" -gt 0 && "${1:0:1}" != "-" ]]; then
    second="$1"
    shift
  fi

  WORKFLOW_REF="$(normalize_workflow_ref "$first" "$second")"
  WORKFLOW_PARSE_REST=("$@")
}

resolve_run_udid() {
  local provided="${1:-}"
  if [[ -n "$provided" ]]; then
    printf '%s\n' "$provided"
    return 0
  fi
  if [[ -n "${RZN_IOS_DEFAULT_UDID:-}" ]]; then
    printf '%s\n' "$RZN_IOS_DEFAULT_UDID"
    return 0
  fi
  local request
  request="$(build_simple_tool_request "devices-1" "ios.device.list" '{"includeSimulators":false}')"
  jsonrpc_tool_call "devices-1" "$request" | python_cli select-default-device
}

record_recent_run() {
  python_cli history-append \
    --workflow-ref "$1" \
    --udid "$2" \
    --args-json "$3" \
    --commit "$4" \
    --disconnect-on-finish "$5" \
    --stop-appium-on-finish "$6" \
    --background-on-exit "$7" \
    --lock-device-on-exit "$8" \
    --smart-cache "$9" >/dev/null
}

tool_exists_exact() {
  local tool_name="$1"
  tool_catalog_json | python3 -c '
import json
import sys

want = sys.argv[1]
payload = json.load(sys.stdin)
names = {tool.get("name") for tool in payload.get("tools", []) if isinstance(tool, dict)}
raise SystemExit(0 if want in names else 1)
' "$tool_name"
}

looks_like_tool_name() {
  local value="$1"
  case "$value" in
    ios.*|phone_messages.*|phone_calls.*|phone_notifications.*|rzn.*|util.*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

rerun_recent_entry() {
  local index="$1"
  local payload
  payload="$(python_cli rerun-show "$index")" || return 1
  mapfile -d '' -t rerun_args < <(
    python3 - "$payload" <<'PY'
import json
import sys

entry = json.loads(sys.argv[1])
args = [
    "run",
    entry["workflowRef"],
    "--udid",
    entry["udid"],
    "--args-json",
    json.dumps(entry.get("argsJson", {}), separators=(",", ":")),
    "--commit",
    "1" if entry.get("commit") else "0",
    "--disconnect-on-finish",
    "1" if entry.get("disconnectOnFinish") else "0",
    "--stop-appium-on-finish",
    "1" if entry.get("stopAppiumOnFinish") else "0",
    "--background-on-exit",
    "1" if entry.get("backgroundOnExit") else "0",
    "--lock-device-on-exit",
    "1" if entry.get("lockDeviceOnExit") else "0",
    "--fast",
    "1" if entry.get("smartCache") else "0",
]
for arg in args:
    sys.stdout.buffer.write(arg.encode("utf-8"))
    sys.stdout.buffer.write(b"\0")
PY
  )
  exec "$SELF_PATH" "${rerun_args[@]}"
}

run_workflow_command() {
  local context="$1"
  shift

  parse_workflow_ref_args "$context" "$@"
  set -- "${WORKFLOW_PARSE_REST[@]}"

  local UDID=""
  local ARGS_JSON="{}"
  local COMMIT="0"
  local DISCONNECT_ON_FINISH="1"
  local STOP_APPIUM_ON_FINISH="0"
  local BACKGROUND_ON_EXIT="0"
  local LOCK_DEVICE_ON_EXIT="0"
  local FAST_MODE_OVERRIDE=""
  local OUTPUT_JSON="0"
  local DISCONNECT_EXPLICIT="0"
  local STOP_APPIUM_EXPLICIT="0"
  local BACKGROUND_EXPLICIT="0"
  local LOCK_EXPLICIT="0"

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
      --dry-run)
        COMMIT="0"
        shift
        ;;
      --disconnect-on-finish)
        DISCONNECT_ON_FINISH="${2:-1}"
        DISCONNECT_EXPLICIT="1"
        shift 2
        ;;
      --stop-appium-on-finish)
        STOP_APPIUM_ON_FINISH="${2:-0}"
        STOP_APPIUM_EXPLICIT="1"
        shift 2
        ;;
      --background-on-exit)
        BACKGROUND_ON_EXIT="${2:-0}"
        BACKGROUND_EXPLICIT="1"
        shift 2
        ;;
      --lock-device-on-exit)
        LOCK_DEVICE_ON_EXIT="${2:-0}"
        LOCK_EXPLICIT="1"
        shift 2
        ;;
      --fast)
        if [[ "$#" -ge 2 && "${2:0:2}" != "--" ]]; then
          FAST_MODE_OVERRIDE="${2:-1}"
          shift 2
        else
          FAST_MODE_OVERRIDE="1"
          shift
        fi
        ;;
      --json)
        OUTPUT_JSON="1"
        shift
        ;;
      *)
        fail "unknown $context argument: $1"
        ;;
    esac
  done

  UDID="$(resolve_run_udid "$UDID")"
  load_ios_session_env
  maybe_cleanup_stale_runtime_cache
  SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
  ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
  ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
  COMMIT_JSON="$(bool_json "$COMMIT")"
  if [[ -n "$FAST_MODE_OVERRIDE" ]]; then
    if [[ "$(bool_json "$FAST_MODE_OVERRIDE")" == "true" ]]; then
      FAST_MODE_OVERRIDE="1"
    else
      FAST_MODE_OVERRIDE="0"
    fi
  fi
  DISCONNECT_ON_FINISH_JSON="$(bool_json "$DISCONNECT_ON_FINISH")"
  STOP_APPIUM_ON_FINISH_JSON="$(bool_json "$STOP_APPIUM_ON_FINISH")"
  BACKGROUND_ON_EXIT_JSON="$(bool_json "$BACKGROUND_ON_EXIT")"
  LOCK_DEVICE_ON_EXIT_JSON="$(bool_json "$LOCK_DEVICE_ON_EXIT")"
  local SMART_CACHE_ACTIVE
  SMART_CACHE_ACTIVE="$(smart_cache_default_enabled)"
  if [[ -n "$FAST_MODE_OVERRIDE" ]]; then
    SMART_CACHE_ACTIVE="$FAST_MODE_OVERRIDE"
  fi
  if [[ "$DISCONNECT_EXPLICIT" == "1" && "$DISCONNECT_ON_FINISH" == "1" ]]; then
    SMART_CACHE_ACTIVE="0"
  fi
  if [[ "$STOP_APPIUM_EXPLICIT" == "1" && "$STOP_APPIUM_ON_FINISH" == "1" ]]; then
    SMART_CACHE_ACTIVE="0"
  fi
  if [[ "$BACKGROUND_EXPLICIT" == "1" && "$BACKGROUND_ON_EXIT" == "1" ]]; then
    SMART_CACHE_ACTIVE="0"
  fi
  if [[ "$LOCK_EXPLICIT" == "1" && "$LOCK_DEVICE_ON_EXIT" == "1" ]]; then
    SMART_CACHE_ACTIVE="0"
  fi
  SMART_CACHE_JSON="$(bool_json "$SMART_CACHE_ACTIVE")"
  if [[ "$SMART_CACHE_JSON" == "true" ]]; then
    DISCONNECT_ON_FINISH_JSON="false"
    STOP_APPIUM_ON_FINISH_JSON="false"
    BACKGROUND_ON_EXIT_JSON="false"
    LOCK_DEVICE_ON_EXIT_JSON="false"
  fi
  record_recent_run \
    "$WORKFLOW_REF" \
    "$UDID" \
    "$ARGS_JSON" \
    "$COMMIT" \
    "$DISCONNECT_ON_FINISH" \
    "$STOP_APPIUM_ON_FINISH" \
    "$BACKGROUND_ON_EXIT" \
    "$LOCK_DEVICE_ON_EXIT" \
    "$SMART_CACHE_ACTIVE"
  maybe_print_cold_start_notice "$SMART_CACHE_ACTIVE" "$UDID" "$OUTPUT_JSON"
  REQUEST="$(
    python3 - "$WORKFLOW_REF" "$UDID" "$ARGS_JSON" "$COMMIT_JSON" "$DISCONNECT_ON_FINISH_JSON" "$STOP_APPIUM_ON_FINISH_JSON" "$BACKGROUND_ON_EXIT_JSON" "$LOCK_DEVICE_ON_EXIT_JSON" "$IOS_XCODE_ORG_ID" "$IOS_XCODE_SIGNING_ID" "$IOS_UPDATED_WDA_BUNDLE_ID" "$SHOW_XCODE_LOG_JSON" "$ALLOW_PROVISIONING_UPDATES_JSON" "$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON" "$IOS_SESSION_CREATE_TIMEOUT_MS" "$IOS_WDA_LAUNCH_TIMEOUT_MS" "$IOS_WDA_CONNECTION_TIMEOUT_MS" <<'PY'
import json
import sys

(
    workflow_ref,
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
            "workflow": workflow_ref,
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
  local run_status=0
  if [[ "$SMART_CACHE_JSON" == "true" ]]; then
    if RZN_IOS_PERSIST_RUNTIME=1 RZN_IOS_REUSE_ACTIVE_SESSION=1 RZN_PHONE_SHOW_TIMINGS=1 RZN_PHONE_PRETTY_WORKFLOW="$([[ "$OUTPUT_JSON" == "1" ]] && printf '0' || printf '1')" jsonrpc_tool_call "workflow-run-1" "$REQUEST"; then
      run_status=0
    else
      run_status=$?
    fi
  else
    if RZN_PHONE_SHOW_TIMINGS=1 RZN_PHONE_PRETTY_WORKFLOW="$([[ "$OUTPUT_JSON" == "1" ]] && printf '0' || printf '1')" jsonrpc_tool_call "workflow-run-1" "$REQUEST"; then
      run_status=0
    else
      run_status=$?
    fi
  fi
  if [[ "$SMART_CACHE_JSON" == "true" ]]; then
    arm_runtime_cache_reaper
  fi
  return "$run_status"
}

status_command() {
  maybe_cleanup_stale_runtime_cache
  REQUEST="$(build_simple_tool_request "status-1" "rzn.worker.health" "{}")"
  RZN_IOS_PERSIST_RUNTIME=1 jsonrpc_tool_call "status-1" "$REQUEST"
}

shutdown_command() {
  local STOP_APPIUM="1"
  local BACKGROUND_APP="0"
  local LOCK_DEVICE="0"

  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --stop-appium)
        STOP_APPIUM="${2:-1}"
        shift 2
        ;;
      --background-on-exit)
        BACKGROUND_APP="${2:-0}"
        shift 2
        ;;
      --lock-device-on-exit)
        LOCK_DEVICE="${2:-0}"
        shift 2
        ;;
      *)
        fail "unknown shutdown argument: $1"
        ;;
    esac
  done

  STOP_APPIUM_JSON="$(bool_json "$STOP_APPIUM")"
  BACKGROUND_APP_JSON="$(bool_json "$BACKGROUND_APP")"
  LOCK_DEVICE_JSON="$(bool_json "$LOCK_DEVICE")"
  REQUEST="$(
    python3 - "$STOP_APPIUM_JSON" "$BACKGROUND_APP_JSON" "$LOCK_DEVICE_JSON" <<'PY'
import json
import sys

stop_appium_json, background_json, lock_json = sys.argv[1:]
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "shutdown-1",
            "method": "tools/call",
            "params": {
                "name": "rzn.worker.shutdown",
                "arguments": {
                    "stopAppium": json.loads(stop_appium_json),
                    "shutdownWDA": True,
                    "backgroundApp": json.loads(background_json),
                    "lockDevice": json.loads(lock_json),
                },
            },
        }
    )
)
PY
  )"
  RZN_IOS_PERSIST_RUNTIME=1 jsonrpc_tool_call "shutdown-1" "$REQUEST"
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
    OUTPUT_JSON="0"
    OUTPUT_PRETTY="$(default_pretty_output)"
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --json)
          OUTPUT_JSON="1"
          OUTPUT_PRETTY="0"
          shift
          ;;
        --pretty)
          OUTPUT_PRETTY="1"
          shift
          ;;
        *)
          fail "unknown devices argument: $1"
          ;;
      esac
    done
    REQUEST="$(build_simple_tool_request "devices-1" "ios.device.list" '{"includeSimulators":false}')"
    jsonrpc_tool_call "devices-1" "$REQUEST" | render_devices_output "$OUTPUT_JSON" "$OUTPUT_PRETTY"
    ;;
  status)
    status_command
    ;;
  shutdown)
    shutdown_command "$@"
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
  run)
    run_workflow_command "run" "$@"
    ;;
  list)
    parse_workflow_list_args "$@"
    workflow_catalog_json "" "$FAMILY_FILTER" | render_workflow_list_output \
      "$OUTPUT_JSON" \
      "$OUTPUT_PRETTY" \
      "$COMPACT_VIEW" \
      "$SYSTEM_FILTER" \
      "$SEARCH_FILTER" \
      "$SURFACE_FILTER" \
      "$HAS_INPUT_FILTER" \
      "$MUTATING_FILTER" \
      "$FAVORITES_ONLY"
    ;;
  show)
    first="${1:-}"
    [[ -n "$first" ]] || fail "show requires a workflow ref or tool name"
    if [[ "$#" -eq 1 || "${2:0:1}" == "-" ]] && (tool_exists_exact "$first" || looks_like_tool_name "$first"); then
      TOOL_NAME="$first"
      shift || true
      OUTPUT_JSON="0"
      OUTPUT_PRETTY="$(default_pretty_output)"
      while [[ "$#" -gt 0 ]]; do
        case "$1" in
          --json)
            OUTPUT_JSON="1"
            OUTPUT_PRETTY="0"
            shift
            ;;
          --pretty)
            OUTPUT_PRETTY="1"
            shift
            ;;
          *)
            fail "unknown show argument: $1"
            ;;
        esac
      done
      TOOL_PAYLOAD="$(tool_catalog_json | select_tool_json "$TOOL_NAME")" || exit $?
      printf '%s\n' "$TOOL_PAYLOAD" | render_tool_show_output "$OUTPUT_JSON" "$OUTPUT_PRETTY"
    else
      parse_workflow_ref_args "show" "$@"
      set -- "${WORKFLOW_PARSE_REST[@]}"
      OUTPUT_JSON="0"
      OUTPUT_PRETTY="$(default_pretty_output)"
      SHOW_EXAMPLE="0"
      while [[ "$#" -gt 0 ]]; do
        case "$1" in
          --json)
            OUTPUT_JSON="1"
            OUTPUT_PRETTY="0"
            shift
            ;;
          --pretty)
            OUTPUT_PRETTY="1"
            shift
            ;;
          --example)
            SHOW_EXAMPLE="1"
            OUTPUT_PRETTY="1"
            shift
            ;;
          *)
            fail "unknown show argument: $1"
            ;;
        esac
      done
      WORKFLOW_PAYLOAD="$(workflow_catalog_json | select_workflow_json "$WORKFLOW_REF")" || exit $?
      printf '%s\n' "$WORKFLOW_PAYLOAD" | render_workflow_show_output "$OUTPUT_JSON" "$OUTPUT_PRETTY" "$SHOW_EXAMPLE"
    fi
    ;;
  workflow)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      list)
        parse_workflow_list_args "$@"
        workflow_catalog_json "" "$FAMILY_FILTER" | render_workflow_list_output \
          "$OUTPUT_JSON" \
          "$OUTPUT_PRETTY" \
          "$COMPACT_VIEW" \
          "$SYSTEM_FILTER" \
          "$SEARCH_FILTER" \
          "$SURFACE_FILTER" \
          "$HAS_INPUT_FILTER" \
          "$MUTATING_FILTER" \
          "$FAVORITES_ONLY"
        ;;
      show)
        parse_workflow_ref_args "workflow show" "$@"
        set -- "${WORKFLOW_PARSE_REST[@]}"
        OUTPUT_JSON="0"
        OUTPUT_PRETTY="$(default_pretty_output)"
        SHOW_EXAMPLE="0"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --json)
              OUTPUT_JSON="1"
              OUTPUT_PRETTY="0"
              shift
              ;;
            --pretty)
              OUTPUT_PRETTY="1"
              shift
              ;;
            --example)
              SHOW_EXAMPLE="1"
              OUTPUT_PRETTY="1"
              shift
              ;;
            *)
              fail "unknown workflow show argument: $1"
              ;;
          esac
        done
        WORKFLOW_PAYLOAD="$(workflow_catalog_json | select_workflow_json "$WORKFLOW_REF")" || exit $?
        printf '%s\n' "$WORKFLOW_PAYLOAD" | render_workflow_show_output "$OUTPUT_JSON" "$OUTPUT_PRETTY" "$SHOW_EXAMPLE"
        ;;
      *)
        fail "unknown workflow subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  capability)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      list)
        OUTPUT_JSON="0"
        OUTPUT_PRETTY="$(default_pretty_output)"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --json)
              OUTPUT_JSON="1"
              OUTPUT_PRETTY="0"
              shift
              ;;
            --pretty)
              OUTPUT_PRETTY="1"
              shift
              ;;
            *)
              fail "unknown capability list argument: $1"
              ;;
          esac
        done
        capability_catalog_json | render_capability_output "$OUTPUT_JSON" "$OUTPUT_PRETTY"
        ;;
      *)
        fail "unknown capability subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  tool)
    scope="$cmd"
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      list)
        DIRECT_ONLY="0"
        OUTPUT_JSON="0"
        OUTPUT_PRETTY="$(default_pretty_output)"
        SEARCH_FILTER=""
        TOOL_FAMILY_FILTER=""
        TOOL_TIER_FILTER=""
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --direct)
              DIRECT_ONLY="1"
              shift
              ;;
            --search)
              shift || fail "tool list requires a value after --search"
              [[ "$#" -gt 0 ]] || fail "tool list requires a value after --search"
              SEARCH_FILTER="$1"
              shift
              ;;
            --family)
              shift || fail "tool list requires a value after --family"
              [[ "$#" -gt 0 ]] || fail "tool list requires a value after --family"
              TOOL_FAMILY_FILTER="$1"
              shift
              ;;
            --tier)
              shift || fail "tool list requires a value after --tier"
              [[ "$#" -gt 0 ]] || fail "tool list requires a value after --tier"
              TOOL_TIER_FILTER="$1"
              shift
              ;;
            --json)
              OUTPUT_JSON="1"
              OUTPUT_PRETTY="0"
              shift
              ;;
            --pretty)
              OUTPUT_PRETTY="1"
              shift
              ;;
            *)
              fail "unknown tool list argument: $1"
              ;;
          esac
        done
        if [[ "$DIRECT_ONLY" == "1" ]]; then
          tool_catalog_json | filter_tool_catalog_json direct | render_tool_list_output "$OUTPUT_JSON" "$OUTPUT_PRETTY" "$SEARCH_FILTER" "$TOOL_FAMILY_FILTER" "$TOOL_TIER_FILTER"
        else
          tool_catalog_json | render_tool_list_output "$OUTPUT_JSON" "$OUTPUT_PRETTY" "$SEARCH_FILTER" "$TOOL_FAMILY_FILTER" "$TOOL_TIER_FILTER"
        fi
        ;;
      show)
        TOOL_NAME="${1:-}"
        [[ -n "$TOOL_NAME" ]] || fail "$scope show requires a tool name"
        shift || true
        OUTPUT_JSON="0"
        OUTPUT_PRETTY="$(default_pretty_output)"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --json)
              OUTPUT_JSON="1"
              OUTPUT_PRETTY="0"
              shift
              ;;
            --pretty)
              OUTPUT_PRETTY="1"
              shift
              ;;
            *)
              fail "$scope show accepts only a tool name plus --json/--pretty"
              ;;
          esac
        done
        TOOL_PAYLOAD="$(tool_catalog_json | select_tool_json "$TOOL_NAME")" || exit $?
        printf '%s\n' "$TOOL_PAYLOAD" | render_tool_show_output "$OUTPUT_JSON" "$OUTPUT_PRETTY"
        ;;
      call)
        TOOL_NAME="${1:-}"
        [[ -n "$TOOL_NAME" ]] || fail "$scope call requires a tool name"
        shift || true
        ARGS_JSON="{}"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --args-json)
              ARGS_JSON="$(read_json_input "${2:-}")"
              shift 2
              ;;
            *)
              fail "unknown $scope call argument: $1"
              ;;
          esac
        done
        REQUEST="$(build_simple_tool_request "tool-call-1" "$TOOL_NAME" "$ARGS_JSON")"
        jsonrpc_tool_call "tool-call-1" "$REQUEST"
        ;;
      *)
        fail "unknown $scope subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  tools)
    exec "$SELF_PATH" tool list "$@"
    ;;
  recent)
    OUTPUT_JSON="0"
    OUTPUT_PRETTY="$(default_pretty_output)"
    LIMIT="10"
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --limit)
          LIMIT="${2:-}"
          shift 2
          ;;
        --json)
          OUTPUT_JSON="1"
          OUTPUT_PRETTY="0"
          shift
          ;;
        --pretty)
          OUTPUT_PRETTY="1"
          shift
          ;;
        *)
          fail "unknown recent argument: $1"
          ;;
      esac
    done
    python_cli recent --limit "$LIMIT" $([[ "$OUTPUT_JSON" == "1" ]] && printf '%s' "--json") $([[ "$OUTPUT_PRETTY" == "1" ]] && printf '%s' "--pretty")
    ;;
  rerun)
    INDEX="${1:-}"
    [[ -n "$INDEX" ]] || fail "rerun requires a recent entry number"
    shift || true
    [[ "$#" -eq 0 ]] || fail "rerun accepts only the recent entry number"
    rerun_recent_entry "$INDEX"
    ;;
  favorite)
    subcmd="${1:-}"
    shift || true
    case "$subcmd" in
      add)
        REF="${1:-}"
        [[ -n "$REF" ]] || fail "favorite add requires a workflow ref"
        shift || true
        [[ "$#" -eq 0 ]] || fail "favorite add accepts only a workflow ref"
        python_cli favorite-add "$REF"
        ;;
      remove)
        REF="${1:-}"
        [[ -n "$REF" ]] || fail "favorite remove requires a workflow ref"
        shift || true
        [[ "$#" -eq 0 ]] || fail "favorite remove accepts only a workflow ref"
        python_cli favorite-remove "$REF"
        ;;
      list)
        OUTPUT_JSON="0"
        OUTPUT_PRETTY="$(default_pretty_output)"
        while [[ "$#" -gt 0 ]]; do
          case "$1" in
            --json)
              OUTPUT_JSON="1"
              OUTPUT_PRETTY="0"
              shift
              ;;
            --pretty)
              OUTPUT_PRETTY="1"
              shift
              ;;
            *)
              fail "unknown favorite list argument: $1"
              ;;
          esac
        done
        python_cli favorite-list $([[ "$OUTPUT_JSON" == "1" ]] && printf '%s' "--json") $([[ "$OUTPUT_PRETTY" == "1" ]] && printf '%s' "--pretty")
        ;;
      *)
        fail "unknown favorite subcommand: ${subcmd:-<empty>}"
        ;;
    esac
    ;;
  favorites)
    OUTPUT_JSON="0"
    OUTPUT_PRETTY="$(default_pretty_output)"
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --json)
          OUTPUT_JSON="1"
          OUTPUT_PRETTY="0"
          shift
          ;;
        --pretty)
          OUTPUT_PRETTY="1"
          shift
          ;;
        *)
          fail "unknown favorites argument: $1"
          ;;
      esac
    done
    python_cli favorite-list $([[ "$OUTPUT_JSON" == "1" ]] && printf '%s' "--json") $([[ "$OUTPUT_PRETTY" == "1" ]] && printf '%s' "--pretty")
    ;;
  completion)
    SHELL_NAME="${1:-}"
    [[ -n "$SHELL_NAME" ]] || fail "completion requires bash or zsh"
    shift || true
    [[ "$#" -eq 0 ]] || fail "completion accepts only bash or zsh"
    python_cli completion-script "$SHELL_NAME" --command-name "rzn-phone"
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
  __complete)
    ENTITY="${1:-}"
    [[ -n "$ENTITY" ]] || fail "__complete requires an entity"
    shift || true
    case "$ENTITY" in
      commands)
        python_cli complete commands
        ;;
      workflows)
        workflow_catalog_json | python_cli complete workflows
        ;;
      systems)
        workflow_catalog_json | python_cli complete systems
        ;;
      tools)
        tool_catalog_json | python_cli complete tools
        ;;
      families)
        capability_catalog_json | python_cli complete families
        ;;
      favorites)
        python_cli complete favorites
        ;;
      *)
        fail "unknown completion entity: $ENTITY"
        ;;
    esac
    ;;
  *)
    suggest_top_level_command "$cmd"
    ;;
esac
