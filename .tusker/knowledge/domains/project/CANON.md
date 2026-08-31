---
schema: "tusker.domain-canon/v7"
kind: "domain_canon"
id: "project/canon"
project: "rzn-phone"
domain: "project"
title: "Project Canon"
status: "current"
summary: "Current durable truth for Project."
capsule:
  skip_when: "Skip when you only need task proof, runtime events, or generated packets."
  use_when: "Use before changing behavior owned by project or reviewing a domain-impacting task."
  what: "Current durable truth, invariants, and constraints for Project."
source_of_truth:
  - "knowledge/domains/project/CANON.md"
created_at: "2026-08-23T10:55:07Z"
updated_at: "2026-08-23T14:25:44Z"
state_rev: "sha256:4ae515b8d12899b6f373f570236a8b73fb3b4cb76c4da9297b40eed2a35ba9ac"
---

# Project Canon

## Current Truth

- RZN Phone Automation is a Rust workspace with one worker crate.
- The source provides two entry points: `rzn-phone` and `rzn-phone worker`.
- Named workflows are data under `crates/rzn_phone_worker/resources/`.
- The worker uses Appium and XCUITest to drive a trusted physical iPhone.
- Human documentation is in `docs/system/`.
- `docs/system/canon.md` states the authority order.

## Domain map

- `product` — purpose, supported surfaces, and limits.
- `runtime` — Rust worker, CLI, MCP, sessions, and local state.
- `workflows` — JSON workflow files, schema, catalog, and safety fields.
- `release` — build and package scaffolding. It does not prove publication.

Read the narrow domain before editing that part of the repository.

## Stable Interfaces

- CLI: `rzn-phone doctor`, `setup`, `devices`, `list`, `show`, `run`, `tool`,
  `history`, `workflows`, `report`, and `worker`.
- MCP: stdio worker with the `ios.*` and phone-data tool families.
- Workflow schema: `schema/rzn-mobile-workflow.schema.json`.
- Release bundle: `plugin_bundle/rzn-phone.bundle.json`.

## Constraints

- Keep write actions behind workflow and runtime approval checks.
- Keep private phone-data tools behind privacy checks.
- Keep one active device session at a time.
- Treat static checks and real-device checks as different proof.
- Use short, simple sentences in canonical docs.

## Deprecated Or Stale

- The static catalog check does not provide live-device proof.
- App selectors can change with app versions, locale, and account state.
