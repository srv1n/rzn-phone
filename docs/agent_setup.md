# RZN Phone Automation Agent Setup Guide

This guide is for agents that need to set up, diagnose, or safely operate the shipped RZN Phone Automation runtime on a local macOS machine. The repo slug and command surface stay `rzn-phone`.

Use it when the user asks an agent to:

- set up RZN Phone Automation locally
- connect an iPhone and get workflows running
- diagnose why the worker or workflows are failing
- use Codex, Cloud Code, or another MCP-capable agent client to drive the plugin

Prefer read-only verification first. Do not run mutating Reddit or LinkedIn workflows unless the user explicitly asks.

## What the agent is setting up

The shipped release now has two install surfaces:

- installable runtime archive: `rzn-phone-<version>-macos_universal.tar.gz`
- refreshable workflow/examples pack: `rzn-phone-workflows-<version>.tar.gz`

Once installed, the runtime contains:

- the `rzn-phone-worker` MCP binary
- the shipped workflow pack under `resources/workflows/`
- examples under `examples/`
- the compiled `rzn-phone` CLI for `run`, `list/show`, `tool` calls, and `workflows update`

The current packaged set includes 51 workflows across:

- Safari
- App Store
- Google Maps
- Reddit
- LinkedIn
- Instagram
- X
- Phone Messages

## Setup order

Follow this sequence in order:

1. Confirm macOS.
2. Confirm local toolchain:
   - `xcodebuild`
   - `xcrun`
   - `node`
   - `npm`
   - `python3` only if you are building repo release artifacts instead of just installing the shipped runtime
3. Confirm Appium is installed.
4. Confirm the Appium `xcuitest` driver is installed.
5. Confirm a trusted/unlocked physical iPhone is visible in `xcrun xctrace list devices`.
6. Install the shipped runtime:
   - local repo path: `make install`
   - release artifact path: `sh install.sh --source <release-dir-or-base-url>`
7. Verify the installed CLI:
   - `rzn-phone version`
   - `rzn-phone list`
   - `rzn-phone tool list --direct`
8. Configure the MCP client:
   - `command`: `/absolute/path/to/rzn-phone`
   - `args`: `["worker"]`
   - `RZN_IOS_APPIUM_URL`: typically `http://127.0.0.1:4723`
9. Start Appium if needed.
10. Call:
   - `ios.env.doctor`
   - `ios.device.list`
   - `ios.workflow.list`
11. Run exactly one read-only workflow.

If any prerequisite fails, stop and fix it before attempting workflow execution.

## Fast path when using the tester kit

If the user received the generated tester kit ZIP:

1. Unzip `rzn-phone-tester-kit-<version>.zip`.
2. Run:

```bash
./scripts/tester_doctor.sh
```

3. If the doctor passes, install the runtime from the shipped release directory:

```bash
sh install.sh --source /absolute/path/to/release-dir
```

4. Use `rzn-phone info` to confirm the runtime root, workflow dir, and examples dir.
5. Keep this guide and `examples/agent-handoff.md` next to the installed runtime for future agents.

## MCP requirements

Use this minimum MCP server shape:

```json
{
  "mcpServers": {
    "rzn-phone": {
      "command": "/absolute/path/to/rzn-phone",
      "args": ["worker"],
      "env": {
        "RZN_IOS_APPIUM_URL": "http://127.0.0.1:4723"
      }
    }
  }
}
```

The installed `rzn-phone worker` wrapper sets `RZN_PLUGIN_DIR` itself. If an agent bypasses the
wrapper and launches `rzn-phone-worker` directly, it must still provide `RZN_PLUGIN_DIR`.

## Safe first-run workflow sequence

Agents should use this exact progression:

1. `ios.env.doctor`
2. `ios.device.list`
3. `ios.capability.list`
4. `ios.workflow.list`
5. one read-only workflow from this list:
   - `safari/google_search`
   - `appstore/typeahead`
   - `appstore/search_results`
   - `reddit/read_first_post`
   - `reddit/daily_scroll_digest`
   - `linkedin/read_feed`
   - `linkedin/daily_scroll_digest`

Only after one read-only workflow succeeds should the agent continue with broader testing.

If no shipped workflow fits, the installed CLI now exposes direct tools too:

1. `rzn-phone tool show ios.session.create`
2. `rzn-phone tool call ios.appium.ensure`
3. `rzn-phone tool call ios.session.create --args-json '{"udid":"<udid>","kind":"native_app","bundleId":"com.apple.mobilesafari"}'`
4. `rzn-phone tool call ios.ui.observe_compact`

That is the same observe -> act -> verify loop the worker exposes over MCP, without making the agent hand-roll JSON-RPC transport.

## Mutating workflow policy

These workflows can change app state:

- Reddit like/comment/reply/DM workflows
- LinkedIn like/comment/reply/create/update/delete workflows

Agents must:

1. Avoid these by default.
2. Prefer dry-run or draft-style execution first.
3. Require explicit user confirmation before using `commit=true`.
4. Preserve cleanup behavior with:
   - `disconnectOnFinish=true`
   - `backgroundAppOnFinish=true`
   - `lockDeviceOnFinish=false` unless the user asks

## Common setup failures

### Appium missing

Fix:

```bash
npm i -g appium
appium driver install xcuitest
```

### No physical device visible

Fix:

- reconnect the phone by cable
- unlock it
- tap `Trust This Computer`
- open Xcode once and accept prompts

### WebDriverAgent signing failure

Typical signals:

- session creation fails
- `xcodebuild` exits with code 65
- WDA will not install or launch

Fix with env vars if needed:

```bash
export IOS_XCODE_ORG_ID="<apple-team-id>"
export IOS_XCODE_SIGNING_ID="Apple Development"
export IOS_UPDATED_WDA_BUNDLE_ID="com.example.WebDriverAgentRunner"
```

Agents should describe this as an Apple signing/provisioning issue, not a workflow-pack issue.

### Worker starts but workflows are missing

Check:

- the installed runtime root contains `resources/workflows/*.json`
- `rzn-phone list` returns the shipped set
- `rzn-phone capability list` returns the Tier-1/Tier-2 taxonomy
- `ios.workflow.list` returns the shipped set
- `ios.capability.list` returns the grouped capability families
- `rzn-phone show safari/google_search` resolves a workflow definition
- if workflows were refreshed separately, rerun `rzn-phone workflows update`

## Suggested prompts for agents

### Setup

```text
Set up the installed RZN Phone Automation capability on this machine. Verify local prerequisites, confirm the connected iPhone is visible, ensure the shipped workflow pack is loaded, and stop after one read-only workflow succeeds. Do not run mutating Reddit or LinkedIn workflows.
```

### Diagnose

```text
Diagnose why RZN Phone Automation is not working on this machine. Check Appium, the XCUITest driver, device visibility, the installed `rzn-phone` runtime, MCP config, `RZN_PLUGIN_DIR` handling, and WebDriverAgent signing. Fix local setup issues where possible and clearly report any remaining Apple-signing blockers.
```

### Safe exploration

```text
Use RZN Phone Automation in read-only mode on this machine. Start with `ios.env.doctor`, `ios.device.list`, `ios.capability.list`, and `ios.workflow.list`, then run one read-only workflow. Do not use `commit=true`.
```

## What the agent should report back

Report:

- whether prerequisites passed
- whether Appium and the XCUITest driver are installed
- whether a physical iPhone is visible
- whether the MCP config is valid
- whether the workflow pack is loaded
- whether a read-only workflow succeeded
- the exact blocker if setup is still incomplete
