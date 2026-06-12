# RZN Phone Automation Tester Kit

This ZIP contains a signed RZN Phone plugin artifact, local setup scripts, MCP examples, and agent handoff notes for macOS testers.

## Contents

- `artifacts/rzn-phone-<version>-macos_universal.zip`: plugin bundle with the MCP worker and workflows
- `scripts/tester_doctor.sh`: prerequisite and device preflight
- `scripts/prepare_mcp_plugin.sh`: runs the preflight, unpacks the plugin artifact, validates the worker/workflows, and prints MCP next steps
- `examples/rzn-phone.mcp.json`: sample MCP server config
- `examples/agent-handoff.md`: compact setup instructions for another agent
- `AGENT_SETUP.md`: agent playbook for safe setup and first run
- `cards/social/`: reference card catalogs for higher-level workflow orchestration

## Quick Start

From the unzipped tester kit root, run:

```bash
./scripts/prepare_mcp_plugin.sh
```

That one command checks local prerequisites, unpacks the bundled plugin artifact into `plugin/rzn-phone`, verifies the worker and workflow files, writes an MCP config with absolute local paths, and prints the exact next action. If the machine is missing Appium, Xcode tools, the XCUITest driver, Python, or a trusted physical iPhone, it stops with the command to fix.

If you only want the prerequisite/device check, run:

```bash
./scripts/tester_doctor.sh
```

## Required Local Setup

- macOS
- Xcode and Xcode command line tools
- Node.js and npm
- Python 3
- Appium installed globally
- Appium XCUITest driver installed
- trusted/unlocked physical iPhone connected over USB

Recommended setup commands:

```bash
xcode-select --install
npm i -g appium
appium driver install xcuitest
```

## MCP Client Setup

After `./scripts/prepare_mcp_plugin.sh` succeeds:

1. Start Appium in another terminal:

```bash
appium
```

2. Add the generated MCP config printed by the script to your local client.
3. Start with these read-only checks:

- `ios.env.doctor`
- `ios.device.list`
- `ios.workflow.list`
- `ios.workflow.run` with `workflow: safari/google_search`

Use read-only workflows first. Do not run mutating Reddit, LinkedIn, Instagram, or X workflows unless you explicitly want account state changed.

## WebDriverAgent Signing

The most common setup blocker is Apple signing for WebDriverAgent. If session creation fails with an Xcode/provisioning error, set these before running workflows:

```bash
export IOS_XCODE_ORG_ID="<apple-team-id>"
export IOS_XCODE_SIGNING_ID="Apple Development"
export IOS_UPDATED_WDA_BUNDLE_ID="com.example.WebDriverAgentRunner"
```

Use a bundle id that is unique for the tester's Apple team if the default cannot be provisioned.
