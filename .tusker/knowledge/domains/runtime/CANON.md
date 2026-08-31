---
schema: "tusker.domain-canon/v7"
kind: "domain_canon"
id: "runtime/canon"
project: "rzn-phone"
domain: "runtime"
title: "Runtime Canon"
status: "current"
summary: "Current durable truth for Runtime."
capsule:
  skip_when: "Skip when you only need task proof, runtime events, or generated packets."
  use_when: "Use before changing behavior owned by runtime or reviewing a domain-impacting task."
  what: "Current durable truth, invariants, and constraints for Runtime."
source_of_truth:
  - "knowledge/domains/runtime/CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T14:25:44Z"
state_rev: "sha256:8a55e2e0c904f09a2b5f3f2d9fcf89015658684859223fef718fc26aa78204e7"
---

# Runtime Canon

## Current Truth

- The Rust workspace has one crate: `rzn_phone_worker`.
- `rzn-phone-worker` runs the MCP worker over standard input and output.
- `rzn-phone` is the optional CLI binary behind the `cli` Cargo feature.
- Shared code handles Appium, WebDriverAgent, sessions, workflows, tools,
  state, and JSON results.
- The worker can start Appium or use `RZN_IOS_APPIUM_URL`.
- The CLI smart-cache path can reuse a healthy session for five minutes; a
  plain MCP worker keeps state in memory unless persistence is enabled.
- `RZN_PHONE_STATE_DIR` can set the state directory. The default comes from
  the platform state directory.

## Source docs

- `docs/system/architecture.md`
- `docs/system/setup.md`

## Stable Interfaces

- CLI commands are defined in `crates/rzn_phone_worker/src/bin/rzn_phone_cli/args.rs`.
- MCP initialization and prompts are defined in `src/mcp.rs`.
- Direct tools are grouped under `src/tools/`.

## Constraints

- Use the shared worker path for CLI and MCP behavior.
- Re-observe after a UI state change.
- Keep one active phone session at a time.
- Do not expose raw private data in default traces.

## Open Questions

- A non-macOS worker package cannot create a local iOS session without a Mac.
