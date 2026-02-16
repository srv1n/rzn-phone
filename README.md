# ios-tools (RZN Worker Plugin)

`ios-tools` is a standalone signed worker plugin that adds iOS real-device automation to RZN via Appium/XCUITest.

For the next-phase design (compact snapshots, encoded ids, deterministic runner, and workflow packs), see `docs/DEEP_DIVE.md`.
Workflow format standardization is described in `docs/specs/rzn_mobile_workflow_v1.md`.
App Store locator/output notes are documented in `docs/appstore_workflows.md`.

## What this repo contains

- Rust MCP stdio worker (`crates/rzn_ios_tools_worker`)
- Dev-mountable Claude-compatible plugin config (`claude_plugin/ios-tools`)
- Signed bundle config for `rzn-plugin-devkit` (`plugin_bundle/ios-tools.bundle.json`)
- Build/package/smoke scripts (`scripts/*`)

## MVP tool surface

- Worker lifecycle: `rzn.worker.health`, `rzn.worker.shutdown`
- Environment/device: `ios.env.doctor`, `ios.device.list`, `ios.appium.ensure`
- Session: `ios.session.create`, `ios.session.delete`, `ios.session.info`
- UI primitives: `ios.ui.source`, `ios.ui.screenshot`, `ios.ui.observe_compact`
- Targeting/actions: `ios.target.resolve`, `ios.action.tap`, `ios.action.type`, `ios.action.wait`, `ios.action.scroll`, `ios.action.scroll_until`
- Element getters (read-only): `ios.element.text`, `ios.element.attribute`, `ios.element.rect`
- Alerts: `ios.alert.text`, `ios.alert.wait`, `ios.alert.accept`, `ios.alert.dismiss`
- Deterministic runner: `ios.script.run`
- Safari primitives: `ios.web.goto`, `ios.web.wait_css`, `ios.web.click_css`, `ios.web.type_css`, `ios.web.press_key`, `ios.web.page_source`, `ios.web.screenshot`, `ios.web.eval_js`
- Workflows: `ios.workflow.list`, `ios.workflow.run` (`safari.google_search`, `reddit.read_first_post`, `reddit.comment_first_post`, `appstore.typeahead`, `appstore.search_results`, `appstore.app_details`, `appstore.reviews`, `appstore.version_history`, `appstore.screenshots`)

## Safety notes

- `ios.web.eval_js` is intentionally exposed and high-risk. It can mutate page state.
- Use host approval controls for `mcp:plugin.ios-tools.ios:*` when running in autonomous flows.
- `ios.workflow.run` supports a `commit` argument for future destructive workflows; current MVP workflow is read-only.
- `reddit.comment_first_post` requires `commit=true` to tap the submit button (`requiresCommit=true` step).
- App Store workflows in this repo are read-only (no purchase/install/review actions).

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
    "args": { "query": "best wireless headphones", "limit": 5 }
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

Reddit (read-only):

```bash
./scripts/ios_tools.sh reddit-read-smoke <udid>
```

Reddit (comment submit requires commit=1):

```bash
./scripts/ios_tools.sh reddit-comment-smoke <udid> "Nice post — thanks for sharing." 0  # dry run
./scripts/ios_tools.sh reddit-comment-smoke <udid> "Nice post — thanks for sharing." 1  # commit
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
