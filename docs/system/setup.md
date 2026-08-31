---
subject: setup
title: "Setup"
keywords: [install, prerequisites, Appium, device, first run]
part_of: overview
describes: [Makefile, scripts/install_rzn_phone.sh, scripts/tester_doctor.sh]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to install the runtime or prepare a phone."
skip_when: "You need to change code or maintain packaging scripts."
---

# Setup

## Requirements

You need:

- macOS with Xcode and command line tools.
- Node.js and npm.
- Appium with the `xcuitest` driver.
- A trusted, unlocked physical iPhone connected by USB.
- The target apps installed and logged in on the phone.
- An Apple signing team for WebDriverAgent.

## Build from the repository

Build the worker or CLI with the targets in `Makefile`:

```bash
make build
make build-cli
```

The installer at `scripts/install_rzn_phone.sh` is a packaging path. It is not
evidence that a public release exists. Use its `--help` output for source and
staging options.

## Prepare the Mac

```bash
xcode-select --install
node --version || brew install node
npm i -g appium
appium driver install xcuitest
```

Start Appium in a separate terminal when you want a fixed endpoint:

```bash
appium
export RZN_IOS_APPIUM_URL="http://127.0.0.1:4723"
```

If WebDriverAgent cannot sign, set `IOS_XCODE_ORG_ID`,
`IOS_XCODE_SIGNING_ID`, and, when needed, `IOS_UPDATED_WDA_BUNDLE_ID`.

## Check the phone

Unlock the phone. Tap **Trust** when iOS asks. Then run:

```bash
rzn-phone doctor
rzn-phone devices
rzn-phone list --compact
```

If one physical phone is connected, the CLI can select it. With zero or more
than one phone, pass `--udid <id>` or set `RZN_IOS_DEFAULT_UDID`.

Run one read-only workflow before a write workflow:

```bash
rzn-phone run safari/google_search \
  --args-json '{"query":"rzn-phone","limit":3}'
```

## MCP setup

Point an MCP client at the installed wrapper:

```json
{
  "mcpServers": {
    "rzn-phone": {
      "command": "/absolute/path/to/rzn-phone",
      "args": ["worker"]
    }
  }
}
```

Set `RZN_IOS_APPIUM_URL` in the client environment when Appium runs outside
the worker process.

The worker accepts JSON-lines requests. It has tool and prompt methods. Its
resource list is currently empty. A tool failure is a normal result with
`isError`, not a JSON-RPC transport error.

## Local development

Use Rust and Python 3. Run these commands from the repository root:

```bash
make build
make build-cli
make test
```

Use `make release-check` for repository and packaging checks. It does not need
a connected phone and does not prove a live workflow.
