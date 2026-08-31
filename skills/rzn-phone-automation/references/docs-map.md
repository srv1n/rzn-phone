# RZN Phone documentation map

Use the current system pages for product and runtime behavior. Use the source
files named by those pages when a claim needs proof.

| Need | Read | Source authority |
| --- | --- | --- |
| Product purpose and limits | `README.md`, `docs/system/canon.md` | `README.md`, `plugin_bundle/rzn-phone.bundle.json` |
| Install and diagnose | `docs/system/setup.md` | `scripts/install_rzn_phone.sh`, `scripts/tester_doctor.sh` |
| Runtime path and state | `docs/system/architecture.md` | `crates/rzn_phone_worker/src/` |
| CLI command surface | `docs/system/cli.md` | `crates/rzn_phone_worker/src/bin/rzn_phone_cli/` |
| Workflow fields and authoring | `docs/system/workflows.md`, `references/authoring.md` | `schema/rzn-mobile-workflow.schema.json`, `scripts/validate_workflow_catalog.py`, `crates/rzn_phone_worker/resources/workflows/` |
| Safety and private data | `docs/system/safety.md` | `crates/rzn_phone_worker/src/tools/policy.rs`, workflow JSON |
| Static checks and proof limits | `docs/system/testing.md` | `Makefile`, `scripts/validate_workflow_catalog.py`, Rust tests |

The schema and workflow files use current unversioned contract names. Package
version fields remain build metadata, not product generations.
