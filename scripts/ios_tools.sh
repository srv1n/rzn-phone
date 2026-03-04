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
  linkedin-read-feed <udid> [--out <dir>] [--limit <n>]
                        Run linkedin.read_feed and write result.json + screenshot/ui source artifacts when present.
  linkedin-create-post <udid> <text> [--out <dir>] [--submit 0|1] [--commit 0|1]
                        Run linkedin.create_post. Default is dry-run draft capture (submit=0).
  linkedin-update-post <udid> <text> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]
                        Run linkedin.update_latest_post. Default is dry-run edit preparation (execute=0).
  linkedin-delete-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]
                        Run linkedin.delete_latest_post. Default is dry-run delete preparation (execute=0).
  linkedin-daily-scroll <udid> [--out <dir>] [--max-posts <n>] [--max-scrolls <n>] [--min-engagement-score <n>]
                        Run linkedin.daily_scroll_digest and emit digest.json + thread.md from structured feed rows.
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

build_session_json() {
  local udid="$1"
  jq -nc \
    --arg udid "$udid" \
    --argjson showXcodeLog "$SHOW_XCODE_LOG_JSON" \
    --argjson allowProvisioningUpdates "$ALLOW_PROVISIONING_UPDATES_JSON" \
    --argjson allowProvisioningDeviceRegistration "$ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON" \
    --argjson sessionCreateTimeoutMs "$IOS_SESSION_CREATE_TIMEOUT_MS" \
    --argjson wdaLaunchTimeoutMs "$IOS_WDA_LAUNCH_TIMEOUT_MS" \
    --argjson wdaConnectionTimeoutMs "$IOS_WDA_CONNECTION_TIMEOUT_MS" \
    --argjson signing "$SIGNING_JSON" \
    '{udid:$udid,showXcodeLog:$showXcodeLog,allowProvisioningUpdates:$allowProvisioningUpdates,allowProvisioningDeviceRegistration:$allowProvisioningDeviceRegistration,sessionCreateTimeoutMs:$sessionCreateTimeoutMs,wdaLaunchTimeoutMs:$wdaLaunchTimeoutMs,wdaConnectionTimeoutMs:$wdaConnectionTimeoutMs,signing:$signing}'
}

run_workflow_rpc() {
  local bin="$1"
  local workflow_name="$2"
  local session_json="$3"
  local args_json="$4"
  local commit_json="$5"
  local stop_appium_json="$6"
  local raw_out="$7"

  cat <<JSON | "$bin" > "$raw_out"
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-1","method":"tools/call","params":{"name":"ios.workflow.run","arguments":{"name":"$workflow_name","session":$session_json,"args":$args_json,"commit":$commit_json}}}
{"jsonrpc":"2.0","id":"shutdown-1","method":"tools/call","params":{"name":"rzn.worker.shutdown","arguments":{"stopAppium":$stop_appium_json}}}
JSON
}

extract_workflow_artifacts() {
  local raw_out="$1"
  local out_dir="$2"
  mkdir -p "$out_dir"

  jq -c 'select(.id=="wf-1") | .result.structuredContent' "$raw_out" | jq . > "$out_dir/result.json"

  local screenshot_b64
  screenshot_b64="$(jq -r 'select(.id=="wf-1") | (.result.structuredContent.screenshot.data // .result.structuredContent.draftScreenshot.data // .result.structuredContent.readyScreenshot.data // empty)' "$raw_out")"
  if [[ -n "$screenshot_b64" ]]; then
    printf '%s' "$screenshot_b64" | base64 --decode > "$out_dir/screenshot.png"
  fi

  local ui_source
  ui_source="$(jq -r 'select(.id=="wf-1") | (.result.structuredContent.uiSource.source // .result.structuredContent.draftUiSource.source // .result.structuredContent.readyUiSource.source // empty)' "$raw_out")"
  if [[ -n "$ui_source" ]]; then
    printf '%s' "$ui_source" > "$out_dir/ui_source.xml"
  fi
}

ensure_workflow_success() {
  local raw_out="$1"
  local default_msg="$2"

  if jq -e 'select(.id=="wf-1") | .result.isError == true' "$raw_out" >/dev/null; then
    jq -r 'select(.id=="wf-1") | .result.content[]?.text // empty' "$raw_out" >&2
    return 1
  fi

  if jq -e 'select(.id=="wf-1") | .result.structuredContent.ok == false' "$raw_out" >/dev/null; then
    jq -r 'select(.id=="wf-1") | .result.structuredContent.error // .result.content[]?.text // "'"$default_msg"'"' "$raw_out" >&2
    return 1
  fi

  return 0
}

build_linkedin_digest() {
  local raw_out="$1"
  local out_dir="$2"
  local min_score="$3"

  local digest_json="$out_dir/digest.json"
  local thread_md="$out_dir/thread.md"

  jq --argjson minScore "$min_score" '
    def trim: gsub("^\\s+|\\s+$"; "");
    def to_int:
      if . == null then 0
      else (tostring | gsub(","; "") | tonumber? // 0)
      end;
    def lines_from($raw):
      ($raw | tostring | split("\n") | map(trim) | map(select(length > 0)));
    def author_from($lines):
      (
        if ($lines | length) == 0 then ""
        elif ($lines[0] | test("^(Suggested|Promoted)$"; "i")) then ($lines[1] // $lines[0] // "")
        else ($lines[0] // "")
        end
      ) | split(",")[0] | trim;
    def engagement_line($lines):
      ($lines | map(select(test("Reactions?|Comments?|Reposts?"; "i"))) | first // "");
    def content_lines($lines; $author):
      (
        $lines
        | map(select(test("^(Suggested|Promoted)$"; "i") | not))
        | map(select(test("^React, Comment on"; "i") | not))
        | map(select(test("View (image|video) in fullscreen"; "i") | not))
        | map(select(test("\\bReactions?\\b|\\bComments?\\b|\\bReposts?\\b"; "i") | not))
        | map(select(test("Visible to anyone on or off LinkedIn"; "i") | not))
        | map(select(. != $author))
      );
    def post_from_row:
      . as $row
      | ($row.rawLabel // "") as $raw
      | lines_from($raw) as $lines
      | author_from($lines) as $author
      | engagement_line($lines) as $eng
      | content_lines($lines; $author) as $content
      | (
          $content
          | map(
              select(
                test("^[^,]{1,80}, .+\\b(\\d+[smhdw]|\\d+\\s*(min|mins|minute|minutes|hour|hours|day|days|week|weeks))\\b"; "i")
                | not
              )
            )
        ) as $content_clean
      | (($eng | capture("(?<n>[0-9][0-9,]*)\\s+Reactions?"; "i").n?) | to_int) as $reactions
      | (($eng | capture("(?<n>[0-9][0-9,]*)\\s+Comments?"; "i").n?) | to_int) as $comments
      | (($eng | capture("(?<n>[0-9][0-9,]*)\\s+Reposts?"; "i").n?) | to_int) as $reposts
      | {
          position: ($row.position // 0),
          author: $author,
          title: ($content_clean[0] // $content[0] // ""),
          body: ((($content_clean[1:] // $content[1:] // []) | join("\n"))),
          media: {
            has_media: ($raw | test("View (image|video) in fullscreen"; "i")),
            type: (
              if ($raw | test("View image in fullscreen"; "i")) then "image"
              elif ($raw | test("View video in fullscreen"; "i")) then "video"
              else "unknown"
              end
            )
          },
          engagement: {
            reactions: $reactions,
            comments: $comments,
            reposts: $reposts,
            score: ($reactions + ($comments * 2) + ($reposts * 3)),
            raw: $eng
          },
          rawLabel: $raw
        };

    select(.id == "wf-1")
    | .result.structuredContent
    | {
        generatedAt: (now | todateiso8601),
        workflow: "linkedin.daily_scroll_digest",
        scannedPosts: (.rowCount // ((.rows // []) | length)),
        scrolls: (.scrolls // 0),
        thresholdScore: $minScore,
        posts: ((.rows // []) | map(post_from_row))
      }
    | .engagingPosts = (.posts | map(select(.engagement.score >= $minScore)) | sort_by(.engagement.score) | reverse)
  ' "$raw_out" > "$digest_json"

  jq -r '
    "# LinkedIn Daily Scroll Digest\n\n"
    + "Generated: \(.generatedAt)\n"
    + "Scanned posts: \(.scannedPosts)\n"
    + "Scrolls: \(.scrolls)\n"
    + "Threshold score: \(.thresholdScore)\n\n"
    + "## Engaging Posts\n\n"
    + (
        if (.engagingPosts | length) == 0 then
          "No posts met the engagement threshold.\n"
        else
          (
            .engagingPosts
            | to_entries
            | map(
                "### \(.key + 1). \(.value.author)\n"
                + "Score: \(.value.engagement.score) | Reactions: \(.value.engagement.reactions) | Comments: \(.value.engagement.comments) | Reposts: \(.value.engagement.reposts)\n"
                + "Title: \((.value.title // "") | if . == "" then "(none)" else . end)\n"
                + "Body:\n\((.value.body // "") | if . == "" then "(none)" else . end)\n"
                + "Media: \(.value.media.type)\n"
              )
            | join("\n")
          )
        end
      )
  ' "$digest_json" > "$thread_md"
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
  linkedin-read-feed)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    LIMIT=5
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --limit)
          LIMIT="${2:-5}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-read-feed: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-read-feed <udid> [--out <dir>] [--limit <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-read-feed.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    SESSION_JSON="$(build_session_json "$UDID")"
    ARGS_JSON="$(jq -nc --argjson limit "$LIMIT" '{limit:$limit}')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.read_feed" "$SESSION_JSON" "$ARGS_JSON" "false" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-read-feed failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin read_feed saved artifacts to: $OUT_DIR"
    ;;
  linkedin-create-post)
    UDID="${1:-}"
    POST_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    SUBMIT=0
    COMMIT=0
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --submit)
          SUBMIT="${2:-0}"
          shift 2
          ;;
        --commit)
          COMMIT="${2:-0}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-create-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$POST_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-create-post <udid> <text> [--out <dir>] [--submit 0|1] [--commit 0|1]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-create-post.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    SUBMIT_JSON="$(bool_json "$SUBMIT")"
    COMMIT_JSON="$(bool_json "$COMMIT")"
    SESSION_JSON="$(build_session_json "$UDID")"
    ARGS_JSON="$(jq -nc \
      --arg post_text "$POST_TEXT" \
      --argjson submit "$SUBMIT_JSON" \
      '{post_text:$post_text,submit:$submit}')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.create_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-create-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin create_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-update-post)
    UDID="${1:-}"
    UPDATED_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    MAX_PROFILE_SCROLLS=6
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --execute)
          EXECUTE="${2:-0}"
          shift 2
          ;;
        --commit)
          COMMIT="${2:-0}"
          shift 2
          ;;
        --post-index)
          POST_INDEX="${2:-0}"
          shift 2
          ;;
        --max-profile-scrolls)
          MAX_PROFILE_SCROLLS="${2:-6}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-update-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$UPDATED_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-update-post <udid> <text> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-update-post.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    EXECUTE_JSON="$(bool_json "$EXECUTE")"
    COMMIT_JSON="$(bool_json "$COMMIT")"
    SESSION_JSON="$(build_session_json "$UDID")"
    ARGS_JSON="$(jq -nc \
      --arg updated_text "$UPDATED_TEXT" \
      --argjson execute_update "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson max_profile_scrolls "$MAX_PROFILE_SCROLLS" \
      --arg post_menu_predicate "${LINKEDIN_POST_MENU_PREDICATE:-}" \
      --arg edit_action_predicate "${LINKEDIN_EDIT_ACTION_PREDICATE:-}" \
      --arg save_action_predicate "${LINKEDIN_SAVE_ACTION_PREDICATE:-}" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg menu_button_id "${LINKEDIN_MENU_BUTTON_ID:-}" \
      --arg profile_button_id "${LINKEDIN_PROFILE_BUTTON_ID:-}" \
      '{updated_text:$updated_text,execute_update:$execute_update,post_index:$post_index,max_profile_scrolls:$max_profile_scrolls}
       + (if $post_menu_predicate == "" then {} else {post_menu_predicate:$post_menu_predicate} end)
       + (if $edit_action_predicate == "" then {} else {edit_action_predicate:$edit_action_predicate} end)
       + (if $save_action_predicate == "" then {} else {save_action_predicate:$save_action_predicate} end)
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $menu_button_id == "" then {} else {menu_button_id:$menu_button_id} end)
       + (if $profile_button_id == "" then {} else {profile_button_id:$profile_button_id} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.update_latest_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-update-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin update_latest_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-delete-post)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    MAX_PROFILE_SCROLLS=6
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --execute)
          EXECUTE="${2:-0}"
          shift 2
          ;;
        --commit)
          COMMIT="${2:-0}"
          shift 2
          ;;
        --post-index)
          POST_INDEX="${2:-0}"
          shift 2
          ;;
        --max-profile-scrolls)
          MAX_PROFILE_SCROLLS="${2:-6}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-delete-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-delete-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-delete-post.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    EXECUTE_JSON="$(bool_json "$EXECUTE")"
    COMMIT_JSON="$(bool_json "$COMMIT")"
    SESSION_JSON="$(build_session_json "$UDID")"
    ARGS_JSON="$(jq -nc \
      --argjson execute_delete "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson max_profile_scrolls "$MAX_PROFILE_SCROLLS" \
      --arg post_menu_predicate "${LINKEDIN_POST_MENU_PREDICATE:-}" \
      --arg delete_action_predicate "${LINKEDIN_DELETE_ACTION_PREDICATE:-}" \
      --arg confirm_delete_predicate "${LINKEDIN_CONFIRM_DELETE_PREDICATE:-}" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg menu_button_id "${LINKEDIN_MENU_BUTTON_ID:-}" \
      --arg profile_button_id "${LINKEDIN_PROFILE_BUTTON_ID:-}" \
      '{execute_delete:$execute_delete,post_index:$post_index,max_profile_scrolls:$max_profile_scrolls}
       + (if $post_menu_predicate == "" then {} else {post_menu_predicate:$post_menu_predicate} end)
       + (if $delete_action_predicate == "" then {} else {delete_action_predicate:$delete_action_predicate} end)
       + (if $confirm_delete_predicate == "" then {} else {confirm_delete_predicate:$confirm_delete_predicate} end)
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $menu_button_id == "" then {} else {menu_button_id:$menu_button_id} end)
       + (if $profile_button_id == "" then {} else {profile_button_id:$profile_button_id} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.delete_latest_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-delete-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin delete_latest_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-daily-scroll)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    MAX_POSTS=30
    MAX_SCROLLS=8
    MIN_ENGAGEMENT_SCORE=20
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --max-posts)
          MAX_POSTS="${2:-30}"
          shift 2
          ;;
        --max-scrolls)
          MAX_SCROLLS="${2:-8}"
          shift 2
          ;;
        --min-engagement-score)
          MIN_ENGAGEMENT_SCORE="${2:-20}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-daily-scroll: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-daily-scroll <udid> [--out <dir>] [--max-posts <n>] [--max-scrolls <n>] [--min-engagement-score <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-daily-scroll.XXXXXX)"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    SESSION_JSON="$(build_session_json "$UDID")"
    ARGS_JSON="$(jq -nc \
      --argjson max_posts "$MAX_POSTS" \
      --argjson max_scrolls "$MAX_SCROLLS" \
      '{max_posts:$max_posts,max_scrolls:$max_scrolls}')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.daily_scroll_digest" "$SESSION_JSON" "$ARGS_JSON" "false" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-daily-scroll failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    build_linkedin_digest "$RAW_OUT" "$OUT_DIR" "$MIN_ENGAGEMENT_SCORE"
    echo "linkedin daily scroll digest saved artifacts to: $OUT_DIR"
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
