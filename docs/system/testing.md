---
subject: testing
title: "Testing and proof"
keywords: [testing, validation, proof, device]
part_of: overview
describes: [Makefile, scripts/validate_workflow_catalog.py, crates/rzn_phone_worker]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to check a change or state the proof boundary."
skip_when: "You need a runtime command; use cli.md."
---

# Testing and proof

## Static catalog check

Run:

```bash
python3 scripts/validate_workflow_catalog.py --offline
```

The validator reads workflow JSON from
`crates/rzn_phone_worker/resources/workflows/` and tool definitions from the
Rust sources. It checks schema and catalog rules. It does not connect to a
phone. The current repository check reports 51 workflows, 61 registered tools,
and zero errors.

## Rust and repository checks

Use the commands in `Makefile`:

```bash
make test
make release-check
```

`make test` runs the Rust test targets. `make release-check` also runs the
catalog, packaging, Python, and archive checks listed in the Makefile. Neither
command proves that an app screen, account, selector, signing setup, or phone
works.

## Tester kit

`scripts/create_tester_kit.sh` builds a maintainer test package from the
current plugin bundle. In an unpacked kit, run:

```bash
./scripts/prepare_mcp_plugin.sh
```

This command checks the Mac, extracts the one plugin archive, and writes an
MCP configuration under `generated/`. The package is a test artifact. It is
not proof of a public release or a live-device result.

## Device proof

Run a real-device check only after static checks pass:

```bash
rzn-phone doctor
rzn-phone devices
rzn-phone run safari/google_search \
  --args-json '{"query":"rzn-phone","limit":3}'
```

The CLI command and its flags are defined in
`crates/rzn_phone_worker/src/bin/rzn_phone_cli/args.rs`. Appium and XCUITest
behavior is implemented in `crates/rzn_phone_worker/src/`. A successful static
check is not a live-device result.

## Reporting

Report the command, result, and proof class: static, runtime, package artifact,
or human acceptance. Do not report a static catalog count as workflow success.
