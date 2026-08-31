---
schema: "tusker.domain-canon/v7"
kind: "domain_canon"
id: "product/canon"
project: "rzn-phone"
domain: "product"
title: "Product Canon"
status: "current"
summary: "Current durable truth for Product."
capsule:
  skip_when: "Skip when you only need task proof, runtime events, or generated packets."
  use_when: "Use before changing behavior owned by product or reviewing a domain-impacting task."
  what: "Current durable truth, invariants, and constraints for Product."
source_of_truth:
  - "knowledge/domains/product/CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T14:25:44Z"
state_rev: "sha256:691d3a4fa1445a055edf0818319a6c1730a3cc6087a531c93f0742edd5479360"
---

# Product Canon

## Current Truth

- RZN Phone drives one trusted iPhone from a Mac.
- It provides a Rust CLI, an MCP worker, JSON workflows, and direct tools.
- The current workflow files are in
  `crates/rzn_phone_worker/resources/workflows/`.
- Tool definitions are in `crates/rzn_phone_worker/src/tools/`.
- The repository does not prove a shipped or public product.

## Source docs

- `docs/system/00-overview.md`
- `docs/system/canon.md`
- `docs/system/setup.md`
- `docs/system/safety.md`

## Stable Interfaces

- `rzn-phone` for terminal use.
- `rzn-phone worker` for stdio MCP use.
- `system/workflow` names for packaged workflows.

## Constraints

- The device path needs macOS, Xcode, Appium, XCUITest, and a trusted iPhone.
- App login and device state are part of each app flow.
- Write actions need `commit=true`.
- Private phone data needs the matching privacy gate.

## Open Questions

- Static checks do not prove that a workflow works on a live phone.
