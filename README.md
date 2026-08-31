# RZN Phone Automation

Use a trusted iPhone from a Mac. Use the command line tool, an MCP client, or
a named workflow. The runtime uses Appium and XCUITest.

This project is for personal work. It is not a cloud device farm. It is not a
large-scale scraping system.

## Read first

The short, current documentation is in [`docs/system/`](docs/system/).

- [System overview](docs/system/00-overview.md)
- [Setup](docs/system/setup.md)
- [Architecture](docs/system/architecture.md)
- [Workflows](docs/system/workflows.md)
- [Safety](docs/system/safety.md)
- [CLI reference](docs/system/cli.md)
- [Contributing](docs/system/contributing.md)
- [Current canon](docs/system/canon.md)
- [Testing and proof](docs/system/testing.md)

The pages in `docs/system/` are the current documentation. Rust code,
workflow JSON, schemas, and scripts are the behavior authority.

## What is included

- A Rust worker that speaks MCP over standard input and output.
- An optional `rzn-phone` terminal CLI.
- 51 named workflows and 61 registered tool definitions (57 direct tools in
  the CLI direct view) in the current static catalog.
- Workflow data for Safari, App Store, Google Maps, Reddit, LinkedIn,
  Instagram, X, and selected Messages flows.
- Direct read tools for Messages, Calls, and Notifications.
- Build inputs for a plugin bundle and a separate workflow pack. Packaging
  scripts are maintainer tooling.

The catalog counts came from `python3 scripts/validate_workflow_catalog.py
--offline` on 2026-08-23. They do not prove live success on every phone.

## Requirements

For device work, use:

- macOS with Xcode and command line tools
- Node.js and npm
- Appium with the XCUITest driver
- A trusted, unlocked physical iPhone
- The target apps installed and logged in on the phone
- An Apple signing team for WebDriverAgent

## Build from this repository

```bash
make build-cli
target/release/rzn-phone doctor
```

Only source-build instructions are documented here. Packaging and installer
scripts remain available for maintainer work.

## First run

```bash
rzn-phone doctor
rzn-phone devices
rzn-phone list --compact
rzn-phone run safari/google_search \
  --args-json '{"query":"rzn-phone","limit":3}'
```

The last command is a read-only Safari search. Start with it before a write
workflow. Other workflow names may write, so check the workflow inputs first.

## CLI examples

List and inspect workflows:

```bash
rzn-phone list --compact
rzn-phone show safari/google_search
```

Use a direct tool when no workflow fits:

```bash
rzn-phone tool list --direct
rzn-phone tool show ios.ui.observe_compact
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
```

Run the MCP worker:

```bash
rzn-phone worker
```

## Safety

Read-only work is the default. A write workflow needs both:

1. Its workflow-specific execute or submit input set to true.
2. `--commit 1` at runtime.

Use `--dry-run` or `--commit 0` first. Read [Safety](docs/system/safety.md)
before you send a message, post, review, like, or navigation command.

## Checks

```bash
make test
make release-check
```

These are static and repository checks. They do not prove a live-device run.

## License

`rzn-phone` is licensed under the GNU Affero General Public License v3.0 only.
See [`LICENSE`](LICENSE).
