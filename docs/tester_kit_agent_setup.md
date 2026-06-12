# RZN Phone Automation Agent Setup Guide

Use this guide when an agent is asked to set up or diagnose the tester kit on a local macOS machine.

Prefer read-only verification first. Do not run mutating Reddit, LinkedIn, Instagram, or X workflows unless the user explicitly asks.

## Setup Order

1. Confirm you are in the unzipped tester kit root.
2. Run:

```bash
./scripts/prepare_mcp_plugin.sh
```

3. If it fails, fix the exact prerequisite it reports and rerun it.
4. Start Appium:

```bash
appium
```

5. Configure the MCP client using the generated config path or the values printed by the script.
6. Call:

- `ios.env.doctor`
- `ios.device.list`
- `ios.workflow.list`

7. Run exactly one read-only workflow before broader testing.

## MCP Requirements

The prepared MCP config uses:

- `command`: the unpacked `plugin/rzn-phone/bin/macos/universal/rzn-phone-worker`
- `RZN_PLUGIN_DIR`: the unpacked `plugin/rzn-phone` root
- `RZN_IOS_APPIUM_URL`: `http://127.0.0.1:4723`

The sample shape is in `examples/rzn-phone.mcp.json`; prefer the generated config from `./scripts/prepare_mcp_plugin.sh` because it has absolute paths for this machine.

## Safe First-Run Workflow Sequence

Use this exact progression:

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

## Common Setup Failures

### Appium Missing

```bash
npm i -g appium
appium driver install xcuitest
```

### No Physical Device Visible

- reconnect the phone by cable
- unlock it
- tap `Trust This Computer`
- open Xcode once and accept prompts

### WebDriverAgent Signing Failure

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

Describe this as an Apple signing/provisioning issue, not a workflow-pack issue.

### Worker Starts But Workflows Are Missing

Check:

- the unpacked plugin root contains `resources/workflows/*.json`
- `RZN_PLUGIN_DIR` points at the unpacked plugin root
- `ios.workflow.list` returns the shipped set
- `ios.capability.list` returns the grouped capability families

## Report Back

Report:

- whether prerequisites passed
- whether Appium and the XCUITest driver are installed
- whether a physical iPhone is visible
- whether the MCP config path was generated
- whether the workflow pack is loaded
- whether a read-only workflow succeeded
- the exact blocker if setup is still incomplete
