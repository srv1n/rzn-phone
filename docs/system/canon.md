---
subject: canon
title: "Current canon"
keywords: [canon, source, authority]
part_of: overview
describes: [crates/rzn_phone_worker/src, crates/rzn_phone_worker/resources, schema, scripts]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to know which source is authoritative."
skip_when: "You need an operator command; use cli.md or setup.md."
---

# Current canon

This repository has one current implementation. It has no product generations
or public release claim.

| Concern | Canonical source |
| --- | --- |
| Runtime behavior | `crates/rzn_phone_worker/src/` |
| Workflow definitions | `crates/rzn_phone_worker/resources/workflows/` |
| Workflow contract | `schema/rzn-mobile-workflow.schema.json` |
| Catalog and static checks | `scripts/validate_workflow_catalog.py` and `Makefile` |
| Phone-data metadata | `crates/rzn_phone_worker/resources/systems/` |
| Packaging behavior | `scripts/`, `plugin_bundle/rzn-phone.bundle.json` |
| Current human-readable documentation | `docs/system/` |

## What was canonical before this reset

The earlier documentation graph had ten canonical subjects: overview,
architecture, CLI, contributing, documentation audit, product overview,
release, safety, setup, and workflows. The audit found that some of these
pages described plans, history, aliases, or release state that the current
code did not support. The current pages merge the useful facts and remove the
rest.

The earlier documentation did not override the runtime. Rust code, workflow
JSON, the schema, and validation scripts were still the behavior authority.

Cargo package versions, bundle versions, workflow `version` fields, and schema
names are technical identifiers used by code and packaging. They do not define
product generations.

The offline catalog check is static proof. A real-device run is separate proof.
Read [Testing and proof](testing.md) before reporting a result.
