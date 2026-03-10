# iOS Tools Tester Kit

This kit is the easiest way to hand the current build to a friend for evaluation.

It is designed for a tester who wants to run shipped workflows without rebuilding the Rust worker. The only local setup they need is the Apple/Appium stack and an MCP client or RZN host to launch the worker.

## What to send

Send the generated ZIP from:

```bash
./scripts/create_tester_kit.sh
```

That produces:

```text
dist/tester-kit/ios-tools-tester-kit-<version>.zip
```

The tester kit contains:

- `artifacts/ios-tools-<version>-macos_universal.zip`: signed plugin bundle with the worker binary and workflow JSON files
- `scripts/tester_doctor.sh`: local environment/device preflight
- `examples/ios-tools.mcp.json`: sample MCP server config for Codex/Claude-style local clients
- `examples/agent-handoff.md`: compact agent-facing setup and diagnosis instructions
- `AGENT_SETUP.md`: full agent playbook for setup, safe execution, and troubleshooting
- `cards/social/`: reference card catalogs for higher-level workflow orchestration

## Who this is for

Use this distribution path when the recipient:

- is on macOS
- has a real iPhone they can connect over USB
- wants to test existing workflows
- does not need to rebuild from source

If they want to change worker code or author new workflows in-repo, give them the repository instead of only the tester kit.

## Tester prerequisites

The tester needs:

- macOS with Xcode installed
- Xcode command line tools
- Node.js and npm
- Python 3
- Appium installed globally
- Appium XCUITest driver installed
- a trusted/unlocked iPhone connected over USB
- Apple developer signing access if WebDriverAgent needs manual provisioning on their machine

Recommended setup:

```bash
xcode-select --install
npm i -g appium
appium driver install xcuitest
```

Then run:

```bash
./scripts/tester_doctor.sh
```

## Device and Apple-side requirements

The most common onboarding failure is WebDriverAgent signing, not the worker itself.

Tell the tester to do this before trying workflows:

1. Install and open Xcode once.
2. Connect the iPhone by cable.
3. Unlock the phone and tap `Trust This Computer` if prompted.
4. Make sure the device is visible in Xcode and `xcrun xctrace list devices`.
5. If WDA provisioning fails, set these env vars before running the worker:

```bash
export IOS_XCODE_ORG_ID="<apple-team-id>"
export IOS_XCODE_SIGNING_ID="Apple Development"
export IOS_UPDATED_WDA_BUNDLE_ID="com.example.WebDriverAgentRunner"
```

Notes:

- `IOS_XCODE_ORG_ID` is the Apple team id used for signing.
- `IOS_UPDATED_WDA_BUNDLE_ID` should be unique for the tester's team if the default bundle id cannot be signed.
- If they already run Appium elsewhere, prefer pointing the worker at it with `RZN_IOS_APPIUM_URL`.

## Recommended runtime model

There are two good ways to let a friend test this.

### Option A: RZN host plus signed plugin ZIP

Use this if they already have the RZN host/runtime that accepts plugin bundles.

1. Unzip the tester kit.
2. Import or install `artifacts/ios-tools-<version>-macos_universal.zip` into the host.
3. Start Appium locally, or set `RZN_IOS_APPIUM_URL` to an already-running Appium endpoint.
4. Run the host and call the shipped tools/workflows.

This is the cleanest non-dev distribution because the workflows are already bundled and versioned.

### Option B: Local MCP client like Codex/Claude Desktop

Use this if they want an agent to orchestrate the shipped flows on their own machine.

1. Unzip the plugin artifact itself to a local folder.
2. Point their MCP client at the unpacked worker binary.
3. Set `RZN_PLUGIN_DIR` to the unpacked plugin root so the worker can find `resources/workflows`.
4. Set `RZN_IOS_APPIUM_URL=http://127.0.0.1:4723` unless they use a different Appium endpoint.

The sample config in `examples/ios-tools.mcp.json` shows the shape:

```json
{
  "mcpServers": {
    "ios-tools": {
      "command": "/absolute/path/to/unpacked/bin/macos/universal/rzn-ios-tools-worker",
      "args": [],
      "env": {
        "RZN_PLUGIN_DIR": "/absolute/path/to/unpacked",
        "RZN_IOS_APPIUM_URL": "http://127.0.0.1:4723"
      }
    }
  }
}
```

## First-run checklist for the tester

After prerequisites are installed:

1. Start Appium:

```bash
appium
```

2. Confirm the phone is visible:

```bash
xcrun xctrace list devices
```

3. Run a read-only smoke flow first from the host/client:

- `ios.device.list`
- `ios.env.doctor`
- `ios.workflow.run` with `name: safari.google_search`
- `ios.workflow.run` with a read-only App Store or Reddit/LinkedIn browse flow

Prefer read-only workflows first because they verify the environment without mutating app state.

## What you should ship with the artifact

For evaluation builds, ship these together:

- the tester kit ZIP
- the Apple team id / signing notes if the tester is not on your Apple account
- one short prompt file for their MCP client
- a short list of safe workflows to try first

Suggested starter workflows:

- `safari.google_search`
- `appstore.typeahead`
- `appstore.search_results`
- `reddit.read_first_post`
- `reddit.daily_scroll_digest`
- `linkedin.read_feed`
- `linkedin.daily_scroll_digest`

## Prompt to hand to Codex, Cloud Code, or Claude

If the tester is using a local coding/agent client, give them a prompt like:

```text
Use the ios-tools MCP server on this machine. Start with ios.env.doctor and ios.device.list. If the device is healthy, run a read-only workflow such as safari.google_search or appstore.search_results. Do not use mutating Reddit or LinkedIn workflows unless I explicitly ask.
```

That is enough to get safe exploration started without teaching them the full tool surface up front.

For a stricter setup workflow, include `AGENT_SETUP.md` and `examples/agent-handoff.md` from the tester kit when handing off to another agent.

## Distribution recommendation

The best default distribution model is:

1. You build and sign the plugin ZIP on your machine.
2. You generate the tester kit ZIP.
3. You send the tester kit ZIP plus a short message with Apple-signing expectations.
4. They use the included doctor script and sample MCP config locally.

That keeps build complexity on your side while still giving them enough structure to test and iterate on their machine.
