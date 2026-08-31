---
schema: "tusker.domain-canon/v7"
kind: "domain_canon"
id: "release/canon"
project: "rzn-phone"
domain: "release"
title: "Release Canon"
status: "current"
summary: "Current durable truth for Release."
capsule:
  skip_when: "Skip when you only need task proof, runtime events, or generated packets."
  use_when: "Use before changing behavior owned by release or reviewing a domain-impacting task."
  what: "Current durable truth, invariants, and constraints for Release."
source_of_truth:
  - "knowledge/domains/release/CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T14:25:44Z"
state_rev: "sha256:33401bed0e5c624d9fd530e3159755a70c8d5c2eed23236931e86d7362b91f0c"
---

# Release Canon

## Current Truth

- This repository has build, package, sign, and publish scripts.
- The repository does not prove that the product is shipped or public.
- Cargo and bundle versions are build identifiers. They are not product
  generations.
- `make release-check` is a static and local gate. It is not device proof.
- `make build` builds the worker.
- `make build-cli` builds the terminal CLI.
- `make install-artifacts` builds install files and the workflow pack.
- `make plugin-bundle` builds the plugin bundle.
- `make release-artifacts` runs both artifact paths.

## Source docs

- `docs/system/testing.md`
- `docs/system/canon.md`
- `Makefile`
- `scripts/release.py`
- `plugin_bundle/rzn-phone.bundle.json`

## Stable Interfaces

- `make release-check` for the non-device gate.
- `scripts/install_rzn_phone.sh` for local install artifacts.
- `scripts/package_plugin.sh` for the bundle.

## Constraints

- Keep Cargo, bundle, and lock build identifiers equal.
- Verify checksums and signatures when supplied.
- Do not claim a public release from a local archive or a Git tag.
- Do not claim a physical-device test from a static release gate.
