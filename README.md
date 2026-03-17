# ios-tools (RZN Worker Plugin)

`ios-tools` is a standalone signed worker plugin that adds iOS real-device automation to RZN via Appium/XCUITest.

For the next-phase design (compact snapshots, encoded ids, deterministic runner, and workflow packs), see `docs/DEEP_DIVE.md`.
Workflow format standardization is described in `docs/specs/rzn_mobile_workflow_v1.md`.
Social card standardization is described in `docs/specs/rzn_social_card_v1.md`.
App Store locator/output notes are documented in `docs/appstore_workflows.md`.
LinkedIn workflow notes are documented in `docs/linkedin_workflows.md`.
Reddit workflow notes are documented in `docs/reddit_workflows.md`.

## What this repo contains

- Rust MCP stdio worker (`crates/rzn_ios_tools_worker`)
- Dev-mountable Claude-compatible plugin config (`claude_plugin/ios-tools`)
- Signed bundle config for `rzn-plugin-devkit` (`plugin_bundle/ios-tools.bundle.json`)
- System metadata for phone-facing surfaces (`crates/rzn_ios_tools_worker/resources/systems/*`)
- Starter phone-system examples (`examples/phone_messages`, `examples/phone_calls`, `examples/phone_notifications`)
- Build/package/smoke scripts (`scripts/*`)

## Phone system surface

The signed bundle now carries three system metadata slices so the host can treat phone automation as
coherent systems instead of one opaque device worker:

| System | Read path | Actuation status |
| --- | --- | --- |
| `phone_messages` | list threads, read latest messages, find recent OTPs | not promoted yet |
| `phone_calls` | inspect recents / call history | not promoted yet |
| `phone_notifications` | list/filter notifications | not promoted yet |

Current implementation status:

- Metadata lives under `resources/systems/<system_id>/system.metadata.yaml`.
- Starter examples live under `examples/<system_id>/`.
- The worker now exposes first-class read-oriented `phone_*` tools that wrap the lower-level
  `ios.*` primitives, including OTP lookup in recent Messages threads.
- Side-effectful phone actions are intentionally not promoted in this release; the metadata only
  advertises the real read surface.

## MVP tool surface

- Worker lifecycle: `rzn.worker.health`, `rzn.worker.shutdown`
- Environment/device: `ios.env.doctor`, `ios.device.list`, `ios.appium.ensure`
- Session: `ios.session.create`, `ios.session.delete`, `ios.session.info`
- UI primitives: `ios.ui.source`, `ios.ui.screenshot`, `ios.ui.observe_compact`
- Targeting/actions: `ios.target.resolve`, `ios.action.tap`, `ios.action.type`, `ios.action.wait`, `ios.action.scroll`, `ios.action.scroll_until`
- Element getters (read-only): `ios.element.text`, `ios.element.attribute`, `ios.element.rect`
- Alerts: `ios.alert.text`, `ios.alert.wait`, `ios.alert.accept`, `ios.alert.dismiss`
- Deterministic runner: `ios.script.run`
- Utilities: `util.list.length`, `util.list.first`, `util.list.nth`, `util.rank_by_name`, `util.date.bucket_counts`, `util.sleep`
- Safari primitives: `ios.web.goto`, `ios.web.wait_css`, `ios.web.click_css`, `ios.web.type_css`, `ios.web.press_key`, `ios.web.page_source`, `ios.web.screenshot`, `ios.web.eval_js`
- Workflows: `ios.workflow.list`, `ios.workflow.run` (`safari.google_search`, `phone_messages.find_recent_otp`, `reddit.read_first_post`, `reddit.comment_first_post`, `reddit.open_post`, `reddit.daily_scroll_digest`, `reddit.like_post`, `reddit.comment_post`, `reddit.reply_to_comment`, `reddit.open_inbox`, `reddit.open_dm_thread`, `reddit.send_dm`, `reddit.send_dm_by_username`, `reddit.reply_dm_thread`, `appstore.typeahead`, `appstore.search_results`, `appstore.app_details`, `appstore.reviews`, `appstore.version_history`, `appstore.screenshots`, `appstore.post_review`, `linkedin.read_feed`, `linkedin.open_post`, `linkedin.daily_scroll_digest`, `linkedin.like_post`, `linkedin.comment_post`, `linkedin.reply_to_comment`, `linkedin.create_post`, `linkedin.update_latest_post`, `linkedin.delete_latest_post`)

## Safety notes

- `ios.web.eval_js` is intentionally exposed and high-risk. It can mutate page state.
- Use host approval controls for `mcp:plugin.ios-tools.ios:*` when running in autonomous flows.
- `ios.workflow.run` supports `commit`; mutating workflows enforce `requiresCommit` at step level.
- `ios.workflow.run` supports post-run controls: `disconnectOnFinish`, `stopAppiumOnFinish`, `backgroundAppOnFinish`, and `lockDeviceOnFinish`.
- Reddit and LinkedIn engagement workflows use a dual gate: action arg (`execute_*`/`submit`) plus `commit=true`.
- `appstore.post_review` is commit-gated and also requires `execute_submit=true`; the browse-oriented App Store workflows remain read-only.
- LinkedIn write/delete/interaction workflows use `requiresCommit` on mutating taps; run dry with `--commit 0` first.
- Reddit write/interaction/DM workflows use `requiresCommit` on mutating taps; run dry with `--commit 0` first.

## Prerequisites

- macOS with Xcode installed and a trusted/unlocked iPhone connected
- App Store signed in on device (for stable search/result rendering)
- Xcode command line tools (`xcodebuild`, `xcrun`, `xctrace`)
- Node.js available to the runtime environment
- Rust toolchain (`cargo`, `rustup`)
- Rust targets for universal build:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

- Python 3 (for plugin ZIP packaging script)
- `rzn-plugin-devkit` binary (for signing/verifying plugin manifest)
- Appium + XCUITest driver installed:

```bash
npm i -g appium
appium driver install xcuitest
```

## Appium resolution order

1. `RZN_IOS_APPIUM_URL` (recommended)
2. Spawn Appium from: `RZN_IOS_APPIUM_BIN`, `appium`, `/opt/homebrew/bin/appium`, `/usr/local/bin/appium`

If you launch RZN from Finder and PATH is minimal, `RZN_IOS_APPIUM_URL` is the most reliable setup.

## Build

Build release worker:

```bash
cargo build -p rzn_ios_tools_worker --release
```

Build universal macOS binary:

```bash
./scripts/build_universal.sh
```

Unified local CLI:

```bash
./scripts/ios_tools.sh build
./scripts/ios_tools.sh test
./scripts/ios_tools.sh smoke
./scripts/ios_tools.sh doctor
./scripts/ios_tools.sh devices
```

Binary build behavior:

- Default: reuse existing `target/release/rzn_ios_tools_worker`; build only if missing.
- `IOS_TOOLS_FORCE_BUILD=1`: force rebuild before command.
- `IOS_TOOLS_SKIP_BUILD=1`: never build (fails if binary is missing).

## Smoke test (stdin/stdout MCP)

```bash
./scripts/run_smoke.sh
```

The script sends `initialize` and `tools/list` JSON-RPC calls and prints responses.

## Dev mount in host

Start host (example):

```bash
rzn-host --port 18789
```

Mount this plugin directory:

```bash
rznctl claude plugins dev-mount /Users/sarav/Downloads/side/rzn/phone/claude_plugin/ios-tools
```

List tools:

```bash
rznctl tools list
```

## Package signed plugin bundle

```bash
./scripts/package_plugin.sh
```

Optional key overrides:

```bash
./scripts/package_plugin.sh /path/to/ed25519.private /path/to/ed25519.public
```

Or via unified CLI:

```bash
./scripts/ios_tools.sh package
```

This writes and verifies:

- `dist/plugins/ios-tools/0.1.0/macos_universal/ios-tools-0.1.0-macos_universal.zip`
- `dist/plugins/ios-tools/0.1.0/macos_universal/plugin.json`
- `dist/plugins/ios-tools/0.1.0/macos_universal/plugin.sig`

The resulting ZIP now also includes:

- `resources/systems/phone_messages/system.metadata.yaml`
- `resources/systems/phone_calls/system.metadata.yaml`
- `resources/systems/phone_notifications/system.metadata.yaml`
- `examples/phone_messages/...`
- `examples/phone_calls/...`
- `examples/phone_notifications/...`

## Example `tools/call`

Create session:

```json
{
  "name": "ios.session.create",
  "arguments": {
    "udid": "00008110-001C12340E87801E",
    "kind": "safari_web",
    "sessionCreateTimeoutMs": 600000,
    "wdaLaunchTimeoutMs": 240000,
    "wdaConnectionTimeoutMs": 120000
  }
}
```

Run workflow:

```json
{
  "name": "ios.workflow.run",
  "arguments": {
    "name": "safari.google_search",
    "session": { "udid": "00008110-001C12340E87801E" },
    "args": { "query": "best wireless headphones", "limit": 5 },
    "disconnectOnFinish": true,
    "backgroundAppOnFinish": true,
    "lockDeviceOnFinish": false
  }
}
```

## Notification behavior

- JSON-RPC `id` is treated as opaque JSON.
- Notifications (including `initialized` and `shutdown`) are accepted and never receive responses.

## Known limitations (MVP)

- Single active session
- Native automation is best-effort (depends heavily on accessibility ids)
- No resource store for large artifacts (screenshots are returned inline)
- If iOS shows **"Automation Running (hold volume buttons to stop)"**, the worker will attempt `GET /wda/shutdown` on cleanup. If it persists, run `./scripts/ios_tools.sh wda-shutdown` (or `./scripts/ios_tools.sh shutdown`) and ensure the device stays unlocked.

## Real-device workflow smoke

```bash
./scripts/ios_tools.sh workflow-smoke <udid> "best headphones 2026" 5
```

App Store typeahead + artifact export:

```bash
./scripts/ios_tools.sh appstore-typeahead <udid> "voice notes" --out /tmp/appstore-typeahead
```

App Store search results + rank spot-check:

```bash
./scripts/ios_tools.sh appstore-search-results <udid> "voice notes" --target-app-name "Voicenotes AI Notes & Meetings" --out /tmp/appstore-results
```

App Store smoke (asserts at least 1 suggestion + 1 result row):

```bash
./scripts/ios_tools.sh appstore-smoke <udid> "voice notes"
```

App Store review job wrapper:

```bash
python3 scripts/appstore_review_job.py <udid> /path/to/job.json
python3 scripts/appstore_review_job.py <udid> /path/to/job.json --dry-run --skip-upload
```

Messages OTP lookup:

```bash
./scripts/ios_tools.sh messages-find-otp <udid> --thread-contains "OpenAI"
```

Reddit (read-only):

```bash
./scripts/ios_tools.sh reddit-read-smoke <udid>
```

Reddit (comment submit requires commit=1):

```bash
./scripts/ios_tools.sh reddit-comment-smoke <udid> "Nice post — thanks for sharing." 0  # dry run
./scripts/ios_tools.sh reddit-comment-smoke <udid> "Nice post — thanks for sharing." 1  # commit
```

Reddit interaction flows (LM-safe dry-run first):

```bash
./scripts/ios_tools.sh reddit-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/reddit-daily
./scripts/ios_tools.sh reddit-open-post <udid> --post-index 0 --out /tmp/reddit-open
./scripts/ios_tools.sh reddit-like-post <udid> --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-like-dry
./scripts/ios_tools.sh reddit-comment-post <udid> "Thanks for sharing this." --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-comment-dry
./scripts/ios_tools.sh reddit-reply-comment <udid> "Great callout." --execute 0 --commit 0 --post-index 0 --reply-index 0 --out /tmp/reddit-reply-dry
```

Reddit DM flows (LM-safe dry-run first):

```bash
./scripts/ios_tools.sh reddit-open-inbox <udid> --out /tmp/reddit-open-inbox
./scripts/ios_tools.sh reddit-open-dm-thread <udid> --thread-index 0 --out /tmp/reddit-open-dm-thread
./scripts/ios_tools.sh reddit-send-dm <udid> "Hey there" --execute 0 --commit 0 --thread-index 0 --out /tmp/reddit-send-dm-dry
./scripts/ios_tools.sh reddit-send-dm-user <udid> "chorefit" "Hey there" --execute 0 --commit 0 --out /tmp/reddit-send-dm-user-dry
./scripts/ios_tools.sh reddit-reply-dm <udid> "Following up here" --execute 0 --commit 0 --thread-index 0 --out /tmp/reddit-reply-dm-dry
```

Single-session Reddit operation (open + like + comment + optional reply in one worker run):

```bash
IOS_TOOLS_SKIP_BUILD=1 \
./scripts/ios_tools.sh reddit-engage-seq <udid> "Draft comment text" \
  --execute-like 0 --execute-comment 0 --commit 0 --out /tmp/reddit-engage-seq
```

LinkedIn read/create/update/delete:

```bash
./scripts/ios_tools.sh linkedin-read-feed <udid> --limit 5 --out /tmp/linkedin-read
./scripts/ios_tools.sh linkedin-create-post <udid> "Testing workflow draft" --submit 0 --commit 0 --out /tmp/linkedin-create-dry
./scripts/ios_tools.sh linkedin-update-post <udid> "Updated text from workflow" --execute 0 --commit 0 --out /tmp/linkedin-update-dry
./scripts/ios_tools.sh linkedin-delete-post <udid> --execute 0 --commit 0 --out /tmp/linkedin-delete-dry
```

LinkedIn interaction flows (LM-safe dry-run first):

```bash
./scripts/ios_tools.sh linkedin-open-post <udid> --post-index 0 --max-feed-scrolls 6 --out /tmp/linkedin-open
./scripts/ios_tools.sh linkedin-like-post <udid> --execute 0 --commit 0 --post-index 0 --out /tmp/linkedin-like-dry
./scripts/ios_tools.sh linkedin-comment-post <udid> "Nice perspective, thanks for sharing." --execute 0 --commit 0 --post-index 0 --out /tmp/linkedin-comment-dry
./scripts/ios_tools.sh linkedin-reply-comment <udid> "Great point." --execute 0 --commit 0 --post-index 0 --reply-index 0 --out /tmp/linkedin-reply-dry
```

LinkedIn daily scroll digest (thread-ready output):

```bash
./scripts/ios_tools.sh linkedin-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/linkedin-daily
```

Card-based social workflows (catalog-backed):

```bash
./scripts/ios_tools.sh social-card-list
./scripts/ios_tools.sh social-card-list --app linkedin
./scripts/ios_tools.sh social-card-run linkedin.daily_scroll <udid> --set max_posts=20
./scripts/ios_tools.sh social-card-run reddit.comment_post <udid> --text "Nice breakdown." --execute 0 --commit 0
```

Optional end-of-run cleanup on any workflow command:

```bash
./scripts/ios_tools.sh linkedin-like-post <udid> --execute 1 --commit 1 \
  --background-on-exit 1 --lock-device-on-exit 1
```

With explicit WDA signing + xcodebuild logs:

```bash
security find-identity -v -p codesigning  # find your Team ID, e.g. "(7A99W929U5)"

IOS_XCODE_ORG_ID="<team_id>" \
IOS_XCODE_SIGNING_ID="Apple Development" \
IOS_UPDATED_WDA_BUNDLE_ID="com.example.webDriveAgentRunner" \
IOS_SHOW_XCODE_LOG=1 \
IOS_ALLOW_PROVISIONING_UPDATES=1 \
IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION=1 \
IOS_SESSION_CREATE_TIMEOUT_MS=600000 \
IOS_WDA_LAUNCH_TIMEOUT_MS=240000 \
IOS_WDA_CONNECTION_TIMEOUT_MS=120000 \
IOS_STOP_APPIUM_ON_EXIT=1 \
./scripts/ios_tools.sh workflow-smoke <udid> "best headphones 2026" 5
```

`workflow-smoke` now sends a final `rzn.worker.shutdown` call to ensure any active XCTest/Appium session is terminated after the run.
Set `IOS_STOP_APPIUM_ON_EXIT=0` if you want to keep a local Appium server running after the smoke.
