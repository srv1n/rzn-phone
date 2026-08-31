---
schema: "tusker.domain-canon/v7"
kind: "domain_canon"
id: "workflows/canon"
project: "rzn-phone"
domain: "workflows"
title: "Workflows Canon"
status: "current"
summary: "Current durable truth for Workflows."
capsule:
  skip_when: "Skip when you only need task proof, runtime events, or generated packets."
  use_when: "Use before changing behavior owned by workflows or reviewing a domain-impacting task."
  what: "Current durable truth, invariants, and constraints for Workflows."
source_of_truth:
  - "knowledge/domains/workflows/CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T14:25:44Z"
state_rev: "sha256:137eb7fa81effbef482afe516bfc9b2aae7e6ad37ef27ac08444ce548a6b92f8"
---

# Workflows Canon

## Current Truth

- Workflow files live in `crates/rzn_phone_worker/resources/workflows/`.
- System metadata for phone-data surfaces lives in
  `crates/rzn_phone_worker/resources/systems/`; the worker does not load it as
  runtime policy.
- The schema is `schema/rzn-mobile-workflow.schema.json`.
- Every workflow has a slash name and a technical workflow version.
- The capability object declares family, intent, surface, and mutation.
- A write step uses `requiresCommit`.

## Source docs

- `docs/system/workflows.md`
- `docs/system/safety.md`

## Stable Interfaces

- Workflow IDs use `system/workflow`, such as `safari/google_search`.
- Inputs are JSON values described in the workflow file.
- Catalog validation uses `python3 scripts/validate_workflow_catalog.py --offline`.

## Constraints

- Default write inputs to false.
- Require both the workflow write flag and runtime `commit=true`.
- Keep direct tool calls in Rust when data-only workflow steps cannot express
  the shared behavior.
- Add a read-only example before a write example.

## Open Questions

- A static catalog pass does not prove a flow on a live phone.
