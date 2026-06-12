#!/usr/bin/env bash
set -euo pipefail

ORIGINAL_CWD="$(pwd)"
RZN_PHONE_BIN="${RZN_PHONE_BIN:-rzn-phone}"

json_escape() {
  local value="${1:-}"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  printf '"%s"' "$value"
}

emit_json() {
  local status="$1"
  local action="$2"
  local repo_root="$3"
  local command_path="$4"
  local detail="$5"
  local version="$6"
  local workflow_count="$7"
  printf '{\n'
  printf '  "status": %s,\n' "$(json_escape "$status")"
  printf '  "action": %s,\n' "$(json_escape "$action")"
  printf '  "repoRoot": %s,\n' "$(json_escape "$repo_root")"
  printf '  "command": %s,\n' "$(json_escape "$command_path")"
  printf '  "detail": %s,\n' "$(json_escape "$detail")"
  printf '  "version": %s,\n' "$(json_escape "$version")"
  printf '  "workflowCount": %s\n' "$(json_escape "$workflow_count")"
  printf '}\n'
}

find_repo_root() {
  local candidate="${RZN_PHONE_REPO_ROOT:-}"
  if [[ -n "$candidate" && -d "$candidate" ]]; then
    candidate="$(cd "$candidate" && pwd)"
    if [[ -f "$candidate/Makefile" && -f "$candidate/scripts/rzn_phone.sh" && -d "$candidate/crates/rzn_phone_worker" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  fi

  local git_root
  if git_root="$(git -C "$ORIGINAL_CWD" rev-parse --show-toplevel 2>/dev/null)"; then
    if [[ -f "$git_root/Makefile" && -f "$git_root/scripts/rzn_phone.sh" && -d "$git_root/crates/rzn_phone_worker" ]]; then
      printf '%s\n' "$git_root"
      return 0
    fi
  fi

  local search="$ORIGINAL_CWD"
  while [[ "$search" != "/" ]]; do
    if [[ -f "$search/Makefile" && -f "$search/scripts/rzn_phone.sh" && -d "$search/crates/rzn_phone_worker" ]]; then
      printf '%s\n' "$search"
      return 0
    fi
    search="$(dirname "$search")"
  done

  return 1
}

extract_workflow_count() {
  sed -n 's/.*workflows: \([0-9][0-9]*\).*/\1/p' | head -n 1
}

run_runtime_checks() {
  local version_output list_output count runtime_version pack_version version_label
  version_output="$("$RZN_PHONE_BIN" version 2>&1)" || return 1
  "$RZN_PHONE_BIN" capability list >/dev/null 2>&1 || return 2
  list_output="$("$RZN_PHONE_BIN" list --compact 2>&1)" || return 3
  count="$(printf '%s\n' "$list_output" | extract_workflow_count)"
  if [[ -z "$count" || "$count" == "0" ]]; then
    printf '%s\n' "$version_output"
    printf '%s\n' "$count"
    return 4
  fi
  runtime_version="$(printf '%s\n' "$version_output" | sed -n 's/.*"runtimeVersion": "\([^"]*\)".*/\1/p' | head -n 1)"
  pack_version="$(printf '%s\n' "$version_output" | sed -n 's/.*"workflowPackVersion": "\([^"]*\)".*/\1/p' | head -n 1)"
  if [[ -n "$runtime_version" && -n "$pack_version" ]]; then
    version_label="runtime ${runtime_version}, workflows ${pack_version}"
  else
    version_label="$(printf '%s' "$version_output" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g')"
  fi
  printf '%s\n' "$version_label"
  printf '%s\n' "$count"
}

repair_runtime() {
  local repo_root="$1"
  if [[ -z "$repo_root" ]]; then
    return 1
  fi

  make -C "$repo_root" install
}

REPO_ROOT="$(find_repo_root || true)"
COMMAND_PATH="$(command -v "$RZN_PHONE_BIN" 2>/dev/null || true)"
ACTION="checked"
DETAIL=""
VERSION=""
WORKFLOW_COUNT=""

if [[ -n "$COMMAND_PATH" ]]; then
  if CHECK_OUTPUT="$(run_runtime_checks)"; then
    VERSION="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '1p')"
    WORKFLOW_COUNT="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '2p')"
    DETAIL="runtime, capability taxonomy, and workflow catalog are available"
    emit_json "ok" "$ACTION" "$REPO_ROOT" "$COMMAND_PATH" "$DETAIL" "$VERSION" "$WORKFLOW_COUNT"
    exit 0
  fi

  if "$RZN_PHONE_BIN" workflows update >/tmp/rzn-phone-bootstrap-update.log 2>&1; then
    ACTION="updated"
    if CHECK_OUTPUT="$(run_runtime_checks)"; then
      VERSION="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '1p')"
      WORKFLOW_COUNT="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '2p')"
      DETAIL="workflow pack refreshed with rzn-phone workflows update"
      emit_json "ok" "$ACTION" "$REPO_ROOT" "$COMMAND_PATH" "$DETAIL" "$VERSION" "$WORKFLOW_COUNT"
      exit 0
    fi
  fi
fi

if repair_runtime "$REPO_ROOT"; then
  ACTION="installed"
  COMMAND_PATH="$(command -v "$RZN_PHONE_BIN" 2>/dev/null || true)"
  if CHECK_OUTPUT="$(run_runtime_checks)"; then
    VERSION="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '1p')"
    WORKFLOW_COUNT="$(printf '%s\n' "$CHECK_OUTPUT" | sed -n '2p')"
    DETAIL="installed or repaired runtime from repo via make install"
    emit_json "ok" "$ACTION" "$REPO_ROOT" "$COMMAND_PATH" "$DETAIL" "$VERSION" "$WORKFLOW_COUNT"
    exit 0
  fi
fi

if [[ -n "$COMMAND_PATH" ]]; then
  DETAIL="rzn-phone exists but runtime checks failed; inspect rzn-phone doctor, rzn-phone list --compact, rzn-phone workflows path, and local Appium/Xcode setup"
elif [[ -n "$REPO_ROOT" ]]; then
  DETAIL="repo root was found, but make install failed or produced an unhealthy runtime"
else
  DETAIL="rzn-phone is not installed and no usable repo root was found for make install"
fi

emit_json "error" "none" "$REPO_ROOT" "$COMMAND_PATH" "$DETAIL" "$VERSION" "$WORKFLOW_COUNT"
exit 1
