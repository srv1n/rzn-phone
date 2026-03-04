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
  reddit-daily-scroll <udid> [--out <dir>] [--max-posts <n>] [--max-scrolls <n>] [--min-engagement-score <n>]
                        Run reddit.daily_scroll_digest and emit digest.json + thread.md from structured feed rows.
  reddit-open-post <udid> [--out <dir>] [--post-index <n>]
                        Run reddit.open_post for deterministic post targeting (read-only).
  reddit-like-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>]
                        Run reddit.like_post. Default is dry-run (execute=0).
  reddit-comment-post <udid> <comment> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>]
                        Run reddit.comment_post. Default is dry-run draft (execute=0).
  reddit-reply-comment <udid> <reply> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--reply-index <n>] [--max-comment-scrolls <n>] [--target-comment-contains <text>]
                        Run reddit.reply_to_comment. Default is dry-run draft (execute=0).
  appstore-typeahead <udid> <query> [--out <dir>] [--limit <n>] [--typing-mode <full|char-by-char>] [--country <cc>] [--locale <locale>]
                        Run appstore.typeahead and write result.json + screenshot.png + ui_source.xml.
  appstore-search-results <udid> <query> [--out <dir>] [--limit <n>] [--target-app-name <name>] [--max-scrolls <n>] [--country <cc>] [--locale <locale>]
  appstore-search-results <udid> <query> [--out <dir>] [--limit <n>] [--target-app-name <name>] [--max-scrolls <n>] [--submit-mode <suggestion|keyboard>] [--country <cc>] [--locale <locale>]
                        Run appstore.search_results and write result.json + screenshot.png + ui_source.xml.
  appstore-smoke <udid> [query]
                        Real-device smoke test; asserts at least 1 suggestion and 1 result row.
  linkedin-read-feed <udid> [--out <dir>] [--limit <n>]
                        Run linkedin.read_feed and write result.json + screenshot/ui source artifacts when present.
  linkedin-open-post <udid> [--out <dir>] [--post-index <n>] [--max-feed-scrolls <n>]
                        Run linkedin.open_post for deterministic post targeting (read-only).
  linkedin-like-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-feed-scrolls <n>]
                        Run linkedin.like_post. Default is dry-run (execute=0).
  linkedin-comment-post <udid> <comment> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-feed-scrolls <n>]
                        Run linkedin.comment_post. Default is dry-run draft (execute=0).
  linkedin-reply-comment <udid> <reply> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--reply-index <n>] [--max-feed-scrolls <n>] [--max-comment-scrolls <n>] [--target-comment-contains <text>]
                        Run linkedin.reply_to_comment. Default is dry-run draft (execute=0).
  linkedin-create-post <udid> <text> [--out <dir>] [--submit 0|1] [--commit 0|1]
                        Run linkedin.create_post. Default is dry-run draft capture (submit=0).
  linkedin-update-post <udid> <text> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]
                        Run linkedin.update_latest_post. Default is dry-run edit preparation (execute=0).
  linkedin-delete-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-profile-scrolls <n>]
                        Run linkedin.delete_latest_post. Default is dry-run delete preparation (execute=0).
  linkedin-daily-scroll <udid> [--out <dir>] [--max-posts <n>] [--max-scrolls <n>] [--min-engagement-score <n>]
                        Run linkedin.daily_scroll_digest and emit digest.json + thread.md from structured feed rows.
  social-card-list [--app <name>] [--json]
                        List card-based workflows from cards/social catalogs.
  social-card-run <card-id> <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--text <value>] [--set key=value ...]
                        Run a card workflow by id from cards/social catalogs.
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
  screenshot_b64="$(jq -r 'select(.id=="wf-1") | (
      .result.structuredContent.screenshot.data //
      .result.structuredContent.draftScreenshot.data //
      .result.structuredContent.readyScreenshot.data //
      .result.structuredContent.postScreenshot.data //
      .result.structuredContent.beforeLikeScreenshot.data //
      .result.structuredContent.afterLikeScreenshot.data //
      .result.structuredContent.draftCommentScreenshot.data //
      .result.structuredContent.afterCommentScreenshot.data //
      .result.structuredContent.draftReplyScreenshot.data //
      .result.structuredContent.afterReplyScreenshot.data //
      empty
    )' "$raw_out")"
  if [[ -n "$screenshot_b64" ]]; then
    printf '%s' "$screenshot_b64" | base64 --decode > "$out_dir/screenshot.png"
  fi

  local ui_source
  ui_source="$(jq -r 'select(.id=="wf-1") | (
      .result.structuredContent.uiSource.source //
      .result.structuredContent.draftUiSource.source //
      .result.structuredContent.readyUiSource.source //
      .result.structuredContent.postUiSource.source //
      .result.structuredContent.beforeLikeUiSource.source //
      .result.structuredContent.draftCommentUiSource.source //
      .result.structuredContent.draftReplyUiSource.source //
      empty
    )' "$raw_out")"
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
    def capture_int($text; $re):
      ([($text | capture($re; "i").n?)] | first | to_int);
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

build_reddit_digest() {
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
    def capture_int($text; $re):
      ([($text | capture($re; "i").n?)] | first | to_int);
    def lines_from($raw):
      ($raw | tostring | split("\n") | map(trim) | map(select(length > 0)));
    def first_match($lines; $re):
      ($lines | map(select(test($re; "i"))) | first // "");
    def clean_lines($lines):
      (
        $lines
        | map(select(test("^(join|share|save|award)$"; "i") | not))
        | map(select(test("^(promoted|ad)$"; "i") | not))
      );
    def author_from($lines):
      (
        first_match($lines; "(u/|r/)")
        | if . == "" then ($lines[0] // "") else . end
      );
    def title_from($lines):
      (
        $lines
        | map(select(test("u/|r/|\\b(upvotes?|comments?|shares?)\\b"; "i") | not))
        | .[0] // ""
      );
    def body_from($lines; $title):
      (
        $lines
        | map(select(. != $title))
        | map(select(test("\\b(upvotes?|comments?|shares?)\\b"; "i") | not))
        | join("\n")
      );
    def engagement_line($lines):
      first_match($lines; "(upvotes?|comments?|shares?)");
    def post_from_row:
      . as $row
      | ($row.rawLabel // "") as $raw
      | lines_from($raw) as $lines_raw
      | clean_lines($lines_raw) as $lines
      | author_from($lines) as $author
      | title_from($lines) as $title
      | body_from($lines; $title) as $body
      | engagement_line($lines) as $eng
      | capture_int($eng; "(?<n>[0-9][0-9,]*)\\s+upvotes?") as $upvotes
      | capture_int($eng; "(?<n>[0-9][0-9,]*)\\s+comments?") as $comments
      | capture_int($eng; "(?<n>[0-9][0-9,]*)\\s+shares?") as $shares
      | {
          position: ($row.position // 0),
          author: $author,
          title: $title,
          body: $body,
          engagement: {
            upvotes: $upvotes,
            comments: $comments,
            shares: $shares,
            score: ($upvotes + ($comments * 2) + ($shares * 2)),
            raw: $eng
          },
          rawLabel: $raw
        };

    select(.id == "wf-1")
    | .result.structuredContent
    | {
        generatedAt: (now | todateiso8601),
        workflow: "reddit.daily_scroll_digest",
        scannedPosts: (.rowCount // ((.rows // []) | length)),
        scrolls: (.scrolls // 0),
        thresholdScore: $minScore,
        posts: ((.rows // []) | map(post_from_row))
      }
    | .engagingPosts = (.posts | map(select(.engagement.score >= $minScore)) | sort_by(.engagement.score) | reverse)
  ' "$raw_out" > "$digest_json"

  jq -r '
    "# Reddit Daily Scroll Digest\n\n"
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
                "### \(.key + 1). \((.value.author // "") | if . == "" then "(unknown)" else . end)\n"
                + "Score: \(.value.engagement.score) | Upvotes: \(.value.engagement.upvotes) | Comments: \(.value.engagement.comments) | Shares: \(.value.engagement.shares)\n"
                + "Title: \((.value.title // "") | if . == "" then "(none)" else . end)\n"
                + "Body:\n\((.value.body // "") | if . == "" then "(none)" else . end)\n"
              )
            | join("\n")
          )
        end
      )
  ' "$digest_json" > "$thread_md"
}

social_cards_json() {
  local app_filter="${1:-}"
  local cards_glob=("$ROOT"/cards/social/*.json)
  if [[ ! -e "${cards_glob[0]}" ]]; then
    echo "[]"
    return 0
  fi

  jq -sc --arg app "$app_filter" '
    [ .[] | (.cards // [])[] | select($app == "" or .app == $app) ]
    | sort_by(.app, .id)
  ' "${cards_glob[@]}"
}

merge_arg_override() {
  local args_json="$1"
  local key="$2"
  local raw="$3"
  local parsed
  parsed="$(jq -cn --arg raw "$raw" '$raw | (fromjson? // .)')"
  jq -cn --argjson base "$args_json" --arg key "$key" --argjson value "$parsed" \
    '$base + {($key): $value}'
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
  reddit-daily-scroll)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    MAX_POSTS=30
    MAX_SCROLLS=8
    MIN_ENGAGEMENT_SCORE=20
    MIN_DWELL_MS=650
    MAX_DWELL_MS=1800
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
        --min-dwell-ms)
          MIN_DWELL_MS="${2:-650}"
          shift 2
          ;;
        --max-dwell-ms)
          MAX_DWELL_MS="${2:-1800}"
          shift 2
          ;;
        *)
          echo "unknown option for reddit-daily-scroll: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-daily-scroll <udid> [--out <dir>] [--max-posts <n>] [--max-scrolls <n>] [--min-engagement-score <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/reddit-daily-scroll.XXXXXX)"
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
      --argjson min_dwell_ms "$MIN_DWELL_MS" \
      --argjson max_dwell_ms "$MAX_DWELL_MS" \
      '{max_posts:$max_posts,max_scrolls:$max_scrolls,min_dwell_ms:$min_dwell_ms,max_dwell_ms:$max_dwell_ms}')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "reddit.daily_scroll_digest" "$SESSION_JSON" "$ARGS_JSON" "false" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "reddit-daily-scroll failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    build_reddit_digest "$RAW_OUT" "$OUT_DIR" "$MIN_ENGAGEMENT_SCORE"
    echo "reddit daily_scroll_digest saved artifacts to: $OUT_DIR"
    ;;
  reddit-open-post)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    POST_INDEX=0
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --post-index)
          POST_INDEX="${2:-0}"
          shift 2
          ;;
        *)
          echo "unknown option for reddit-open-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-open-post <udid> [--out <dir>] [--post-index <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/reddit-open-post.XXXXXX)"
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
      --argjson post_index "$POST_INDEX" \
      --arg post_cell_predicate "${REDDIT_POST_CELL_PREDICATE:-}" \
      --arg post_open_predicate "${REDDIT_POST_OPEN_PREDICATE:-}" \
      --arg post_ready_predicate "${REDDIT_POST_READY_PREDICATE:-}" \
      '{post_index:$post_index}
       + (if $post_cell_predicate == "" then {} else {post_cell_predicate:$post_cell_predicate} end)
       + (if $post_open_predicate == "" then {} else {post_open_predicate:$post_open_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "reddit.open_post" "$SESSION_JSON" "$ARGS_JSON" "false" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "reddit-open-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "reddit open_post saved artifacts to: $OUT_DIR"
    ;;
  reddit-like-post)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
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
        *)
          echo "unknown option for reddit-like-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-like-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/reddit-like-post.XXXXXX)"
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
      --argjson execute_like "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --arg post_cell_predicate "${REDDIT_POST_CELL_PREDICATE:-}" \
      --arg post_open_predicate "${REDDIT_POST_OPEN_PREDICATE:-}" \
      --arg post_ready_predicate "${REDDIT_POST_READY_PREDICATE:-}" \
      --arg like_button_predicate "${REDDIT_LIKE_BUTTON_PREDICATE:-}" \
      '{execute_like:$execute_like,post_index:$post_index}
       + (if $post_cell_predicate == "" then {} else {post_cell_predicate:$post_cell_predicate} end)
       + (if $post_open_predicate == "" then {} else {post_open_predicate:$post_open_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $like_button_predicate == "" then {} else {like_button_predicate:$like_button_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "reddit.like_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "reddit-like-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "reddit like_post saved artifacts to: $OUT_DIR"
    ;;
  reddit-comment-post)
    UDID="${1:-}"
    COMMENT_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
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
        *)
          echo "unknown option for reddit-comment-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$COMMENT_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-comment-post <udid> <comment> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/reddit-comment-post.XXXXXX)"
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
      --arg comment_text "$COMMENT_TEXT" \
      --argjson execute_comment "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --arg post_cell_predicate "${REDDIT_POST_CELL_PREDICATE:-}" \
      --arg post_open_predicate "${REDDIT_POST_OPEN_PREDICATE:-}" \
      --arg post_ready_predicate "${REDDIT_POST_READY_PREDICATE:-}" \
      --arg comment_field_predicate "${REDDIT_COMMENT_FIELD_PREDICATE:-}" \
      --arg comment_submit_predicate "${REDDIT_COMMENT_SUBMIT_PREDICATE:-}" \
      '{comment_text:$comment_text,execute_comment:$execute_comment,post_index:$post_index}
       + (if $post_cell_predicate == "" then {} else {post_cell_predicate:$post_cell_predicate} end)
       + (if $post_open_predicate == "" then {} else {post_open_predicate:$post_open_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $comment_field_predicate == "" then {} else {comment_field_predicate:$comment_field_predicate} end)
       + (if $comment_submit_predicate == "" then {} else {comment_submit_predicate:$comment_submit_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "reddit.comment_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "reddit-comment-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "reddit comment_post saved artifacts to: $OUT_DIR"
    ;;
  reddit-reply-comment)
    UDID="${1:-}"
    REPLY_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    REPLY_INDEX=0
    MAX_COMMENT_SCROLLS=6
    TARGET_COMMENT_CONTAINS=""
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
        --reply-index)
          REPLY_INDEX="${2:-0}"
          shift 2
          ;;
        --max-comment-scrolls)
          MAX_COMMENT_SCROLLS="${2:-6}"
          shift 2
          ;;
        --target-comment-contains)
          TARGET_COMMENT_CONTAINS="${2:-}"
          shift 2
          ;;
        *)
          echo "unknown option for reddit-reply-comment: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$REPLY_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh reddit-reply-comment <udid> <reply> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--reply-index <n>] [--max-comment-scrolls <n>] [--target-comment-contains <text>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/reddit-reply-comment.XXXXXX)"
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
      --arg reply_text "$REPLY_TEXT" \
      --argjson execute_reply "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson reply_index "$REPLY_INDEX" \
      --argjson max_comment_scrolls "$MAX_COMMENT_SCROLLS" \
      --arg target_comment_contains "$TARGET_COMMENT_CONTAINS" \
      --arg post_cell_predicate "${REDDIT_POST_CELL_PREDICATE:-}" \
      --arg post_open_predicate "${REDDIT_POST_OPEN_PREDICATE:-}" \
      --arg post_ready_predicate "${REDDIT_POST_READY_PREDICATE:-}" \
      --arg reply_button_predicate "${REDDIT_REPLY_BUTTON_PREDICATE:-}" \
      --arg reply_field_predicate "${REDDIT_REPLY_FIELD_PREDICATE:-}" \
      --arg reply_submit_predicate "${REDDIT_REPLY_SUBMIT_PREDICATE:-}" \
      '{reply_text:$reply_text,execute_reply:$execute_reply,post_index:$post_index,reply_index:$reply_index,max_comment_scrolls:$max_comment_scrolls}
       + (if $target_comment_contains == "" then {} else {target_comment_contains:$target_comment_contains} end)
       + (if $post_cell_predicate == "" then {} else {post_cell_predicate:$post_cell_predicate} end)
       + (if $post_open_predicate == "" then {} else {post_open_predicate:$post_open_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $reply_button_predicate == "" then {} else {reply_button_predicate:$reply_button_predicate} end)
       + (if $reply_field_predicate == "" then {} else {reply_field_predicate:$reply_field_predicate} end)
       + (if $reply_submit_predicate == "" then {} else {reply_submit_predicate:$reply_submit_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "reddit.reply_to_comment" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "reddit-reply-comment failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "reddit reply_to_comment saved artifacts to: $OUT_DIR"
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
  linkedin-open-post)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    POST_INDEX=0
    MAX_FEED_SCROLLS=6
    OUT_DIR=""
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --post-index)
          POST_INDEX="${2:-0}"
          shift 2
          ;;
        --max-feed-scrolls)
          MAX_FEED_SCROLLS="${2:-6}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-open-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-open-post <udid> [--out <dir>] [--post-index <n>] [--max-feed-scrolls <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-open-post.XXXXXX)"
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
      --argjson post_index "$POST_INDEX" \
      --argjson max_feed_scrolls "$MAX_FEED_SCROLLS" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg post_ready_predicate "${LINKEDIN_POST_READY_PREDICATE:-}" \
      '{post_index:$post_index,max_feed_scrolls:$max_feed_scrolls}
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.open_post" "$SESSION_JSON" "$ARGS_JSON" "false" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-open-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin open_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-like-post)
    UDID="${1:-}"
    if [[ "$#" -ge 1 ]]; then
      shift 1
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    MAX_FEED_SCROLLS=6
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
        --max-feed-scrolls)
          MAX_FEED_SCROLLS="${2:-6}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-like-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-like-post <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-feed-scrolls <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-like-post.XXXXXX)"
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
      --argjson execute_like "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson max_feed_scrolls "$MAX_FEED_SCROLLS" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg post_ready_predicate "${LINKEDIN_POST_READY_PREDICATE:-}" \
      --arg like_button_predicate "${LINKEDIN_LIKE_BUTTON_PREDICATE:-}" \
      '{execute_like:$execute_like,post_index:$post_index,max_feed_scrolls:$max_feed_scrolls}
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $like_button_predicate == "" then {} else {like_button_predicate:$like_button_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.like_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-like-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin like_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-comment-post)
    UDID="${1:-}"
    COMMENT_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    MAX_FEED_SCROLLS=6
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
        --max-feed-scrolls)
          MAX_FEED_SCROLLS="${2:-6}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-comment-post: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$COMMENT_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-comment-post <udid> <comment> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--max-feed-scrolls <n>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-comment-post.XXXXXX)"
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
      --arg comment_text "$COMMENT_TEXT" \
      --argjson execute_comment "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson max_feed_scrolls "$MAX_FEED_SCROLLS" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg post_ready_predicate "${LINKEDIN_POST_READY_PREDICATE:-}" \
      --arg comment_button_predicate "${LINKEDIN_COMMENT_BUTTON_PREDICATE:-}" \
      --arg comment_field_predicate "${LINKEDIN_COMMENT_FIELD_PREDICATE:-}" \
      --arg comment_submit_predicate "${LINKEDIN_COMMENT_SUBMIT_PREDICATE:-}" \
      '{comment_text:$comment_text,execute_comment:$execute_comment,post_index:$post_index,max_feed_scrolls:$max_feed_scrolls}
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $comment_button_predicate == "" then {} else {comment_button_predicate:$comment_button_predicate} end)
       + (if $comment_field_predicate == "" then {} else {comment_field_predicate:$comment_field_predicate} end)
       + (if $comment_submit_predicate == "" then {} else {comment_submit_predicate:$comment_submit_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.comment_post" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-comment-post failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin comment_post saved artifacts to: $OUT_DIR"
    ;;
  linkedin-reply-comment)
    UDID="${1:-}"
    REPLY_TEXT="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    EXECUTE=0
    COMMIT=0
    POST_INDEX=0
    REPLY_INDEX=0
    MAX_FEED_SCROLLS=6
    MAX_COMMENT_SCROLLS=6
    TARGET_COMMENT_CONTAINS=""
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
        --reply-index)
          REPLY_INDEX="${2:-0}"
          shift 2
          ;;
        --max-feed-scrolls)
          MAX_FEED_SCROLLS="${2:-6}"
          shift 2
          ;;
        --max-comment-scrolls)
          MAX_COMMENT_SCROLLS="${2:-6}"
          shift 2
          ;;
        --target-comment-contains)
          TARGET_COMMENT_CONTAINS="${2:-}"
          shift 2
          ;;
        *)
          echo "unknown option for linkedin-reply-comment: $1" >&2
          exit 1
          ;;
      esac
    done
    if [[ -z "$UDID" || -z "$REPLY_TEXT" ]]; then
      echo "usage: scripts/ios_tools.sh linkedin-reply-comment <udid> <reply> [--out <dir>] [--execute 0|1] [--commit 0|1] [--post-index <n>] [--reply-index <n>] [--max-feed-scrolls <n>] [--max-comment-scrolls <n>] [--target-comment-contains <text>]" >&2
      exit 1
    fi
    if [[ -z "$OUT_DIR" ]]; then
      OUT_DIR="$(mktemp -d /tmp/linkedin-reply-comment.XXXXXX)"
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
      --arg reply_text "$REPLY_TEXT" \
      --argjson execute_reply "$EXECUTE_JSON" \
      --argjson post_index "$POST_INDEX" \
      --argjson reply_index "$REPLY_INDEX" \
      --argjson max_feed_scrolls "$MAX_FEED_SCROLLS" \
      --argjson max_comment_scrolls "$MAX_COMMENT_SCROLLS" \
      --arg target_comment_contains "$TARGET_COMMENT_CONTAINS" \
      --arg post_card_predicate "${LINKEDIN_POST_CARD_PREDICATE:-}" \
      --arg post_ready_predicate "${LINKEDIN_POST_READY_PREDICATE:-}" \
      --arg comment_button_predicate "${LINKEDIN_COMMENT_BUTTON_PREDICATE:-}" \
      --arg reply_button_predicate "${LINKEDIN_REPLY_BUTTON_PREDICATE:-}" \
      --arg reply_field_predicate "${LINKEDIN_REPLY_FIELD_PREDICATE:-}" \
      --arg reply_submit_predicate "${LINKEDIN_REPLY_SUBMIT_PREDICATE:-}" \
      '{reply_text:$reply_text,execute_reply:$execute_reply,post_index:$post_index,reply_index:$reply_index,max_feed_scrolls:$max_feed_scrolls,max_comment_scrolls:$max_comment_scrolls}
       + (if $target_comment_contains == "" then {} else {target_comment_contains:$target_comment_contains} end)
       + (if $post_card_predicate == "" then {} else {post_card_predicate:$post_card_predicate} end)
       + (if $post_ready_predicate == "" then {} else {post_ready_predicate:$post_ready_predicate} end)
       + (if $comment_button_predicate == "" then {} else {comment_button_predicate:$comment_button_predicate} end)
       + (if $reply_button_predicate == "" then {} else {reply_button_predicate:$reply_button_predicate} end)
       + (if $reply_field_predicate == "" then {} else {reply_field_predicate:$reply_field_predicate} end)
       + (if $reply_submit_predicate == "" then {} else {reply_submit_predicate:$reply_submit_predicate} end)')"

    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "linkedin.reply_to_comment" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "linkedin-reply-comment failed" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"
    echo "linkedin reply_to_comment saved artifacts to: $OUT_DIR"
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
  social-card-list)
    APP_FILTER=""
    OUTPUT_JSON=0
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --app)
          APP_FILTER="${2:-}"
          shift 2
          ;;
        --json)
          OUTPUT_JSON=1
          shift 1
          ;;
        *)
          echo "unknown option for social-card-list: $1" >&2
          exit 1
          ;;
      esac
    done

    CARDS_JSON="$(social_cards_json "$APP_FILTER")"
    if [[ "$OUTPUT_JSON" == "1" ]]; then
      jq . <<<"$CARDS_JSON"
      exit 0
    fi

    if [[ "$(jq 'length' <<<"$CARDS_JSON")" -eq 0 ]]; then
      echo "no social cards found"
      exit 0
    fi

    printf '%-28s %-10s %-10s %-32s %s\n' "card_id" "app" "mode" "workflow" "mutating"
    jq -r '.[] | "\(.id)\t\(.app)\t\(.mode)\t\(.workflow)\t\(.mutating)"' <<<"$CARDS_JSON" \
      | while IFS=$'\t' read -r cid app mode wf mutating; do
          printf '%-28s %-10s %-10s %-32s %s\n' "$cid" "$app" "$mode" "$wf" "$mutating"
        done
    ;;
  social-card-run)
    CARD_ID="${1:-}"
    UDID="${2:-}"
    if [[ "$#" -ge 2 ]]; then
      shift 2
    else
      shift "$#"
    fi
    if [[ -z "$CARD_ID" || -z "$UDID" ]]; then
      echo "usage: scripts/ios_tools.sh social-card-run <card-id> <udid> [--out <dir>] [--execute 0|1] [--commit 0|1] [--text <value>] [--set key=value ...]" >&2
      exit 1
    fi

    OUT_DIR=""
    EXECUTE_SET=0
    EXECUTE_VALUE=0
    COMMIT=0
    TEXT_VALUE=""
    SET_OVERRIDES=()
    while [[ "$#" -gt 0 ]]; do
      case "$1" in
        --out)
          OUT_DIR="${2:-}"
          shift 2
          ;;
        --execute)
          EXECUTE_SET=1
          EXECUTE_VALUE="${2:-0}"
          shift 2
          ;;
        --commit)
          COMMIT="${2:-0}"
          shift 2
          ;;
        --text)
          TEXT_VALUE="${2:-}"
          shift 2
          ;;
        --set)
          SET_OVERRIDES+=("${2:-}")
          shift 2
          ;;
        *)
          echo "unknown option for social-card-run: $1" >&2
          exit 1
          ;;
      esac
    done

    CARDS_JSON="$(social_cards_json "")"
    CARD_JSON="$(jq -c --arg id "$CARD_ID" '[ .[] | select(.id == $id) ][0]' <<<"$CARDS_JSON")"
    if [[ "$CARD_JSON" == "null" || -z "$CARD_JSON" ]]; then
      echo "unknown card id: $CARD_ID" >&2
      exit 1
    fi

    WORKFLOW_NAME="$(jq -r '.workflow // empty' <<<"$CARD_JSON")"
    EXECUTE_ARG="$(jq -r '.executeArg // empty' <<<"$CARD_JSON")"
    TEXT_ARG="$(jq -r '.textArg // empty' <<<"$CARD_JSON")"
    ARGS_JSON="$(jq -c '.defaults // {}' <<<"$CARD_JSON")"

    if [[ "$EXECUTE_SET" == "1" ]]; then
      if [[ -z "$EXECUTE_ARG" ]]; then
        echo "card '$CARD_ID' does not define executeArg; --execute is not supported" >&2
        exit 1
      fi
      EXECUTE_JSON="$(bool_json "$EXECUTE_VALUE")"
      ARGS_JSON="$(jq -cn --argjson base "$ARGS_JSON" --arg key "$EXECUTE_ARG" --argjson value "$EXECUTE_JSON" '$base + {($key): $value}')"
    fi

    if [[ -n "$TEXT_VALUE" ]]; then
      if [[ -z "$TEXT_ARG" ]]; then
        echo "card '$CARD_ID' does not define textArg; --text is not supported" >&2
        exit 1
      fi
      ARGS_JSON="$(jq -cn --argjson base "$ARGS_JSON" --arg key "$TEXT_ARG" --arg value "$TEXT_VALUE" '$base + {($key): $value}')"
    fi

    for kv in "${SET_OVERRIDES[@]}"; do
      if [[ "$kv" != *=* ]]; then
        echo "--set expects key=value, got: $kv" >&2
        exit 1
      fi
      key="${kv%%=*}"
      value="${kv#*=}"
      if [[ -z "$key" ]]; then
        echo "--set key must not be empty" >&2
        exit 1
      fi
      ARGS_JSON="$(merge_arg_override "$ARGS_JSON" "$key" "$value")"
    done

    MISSING_REQUIRED="$(jq -r --argjson args "$ARGS_JSON" '
      (.requiredArgs // [])
      | map(select(($args[.] // null) == null or (($args[.] | type) == "string" and ($args[.] | length) == 0)))
      | join(",")
    ' <<<"$CARD_JSON")"
    if [[ -n "$MISSING_REQUIRED" ]]; then
      echo "card '$CARD_ID' missing required args: $MISSING_REQUIRED" >&2
      exit 1
    fi

    if [[ -z "$OUT_DIR" ]]; then
      SAFE_ID="$(printf '%s' "$CARD_ID" | tr '/: ' '---')"
      OUT_DIR="$(mktemp -d "/tmp/${SAFE_ID}.XXXXXX")"
    fi
    mkdir -p "$OUT_DIR"

    load_ios_session_env
    SHOW_XCODE_LOG_JSON="$(bool_json "$IOS_SHOW_XCODE_LOG")"
    ALLOW_PROVISIONING_UPDATES_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_UPDATES")"
    ALLOW_PROVISIONING_DEVICE_REGISTRATION_JSON="$(bool_json "$IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION")"
    STOP_APPIUM_ON_EXIT_JSON="$(bool_json "$IOS_STOP_APPIUM_ON_EXIT")"
    SIGNING_JSON="$(build_signing_json)"

    COMMIT_JSON="$(bool_json "$COMMIT")"
    SESSION_JSON="$(build_session_json "$UDID")"
    BIN="$(worker_bin)"
    RAW_OUT="$OUT_DIR/.raw.jsonl"
    run_workflow_rpc "$BIN" "$WORKFLOW_NAME" "$SESSION_JSON" "$ARGS_JSON" "$COMMIT_JSON" "$STOP_APPIUM_ON_EXIT_JSON" "$RAW_OUT"
    ensure_workflow_success "$RAW_OUT" "social-card-run failed ($CARD_ID)" || exit 1
    extract_workflow_artifacts "$RAW_OUT" "$OUT_DIR"

    if [[ "$WORKFLOW_NAME" == "linkedin.daily_scroll_digest" ]]; then
      MIN_SCORE="$(jq -r '.min_engagement_score // 20' <<<"$ARGS_JSON")"
      build_linkedin_digest "$RAW_OUT" "$OUT_DIR" "$MIN_SCORE"
    elif [[ "$WORKFLOW_NAME" == "reddit.daily_scroll_digest" ]]; then
      MIN_SCORE="$(jq -r '.min_engagement_score // 20' <<<"$ARGS_JSON")"
      build_reddit_digest "$RAW_OUT" "$OUT_DIR" "$MIN_SCORE"
    fi

    echo "social card run complete: card=$CARD_ID workflow=$WORKFLOW_NAME artifacts=$OUT_DIR"
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
