---
subject: workflows
title: "Workflows"
keywords: [workflow, JSON, schema, catalog, authoring]
part_of: overview
describes: [crates/rzn_phone_worker/resources/workflows, schema/rzn-mobile-workflow.schema.json, scripts/validate_workflow_catalog.py]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to inspect, add, or validate a named workflow."
skip_when: "You need direct Rust tools or maintainer packaging details."
---

# Workflows

A workflow is a small set of steps for one phone task. It can read the screen,
act on the phone, and return structured data.

## Source files

Workflow files live in
`crates/rzn_phone_worker/resources/workflows/`. System metadata for phone-data
surfaces lives in `crates/rzn_phone_worker/resources/systems/`; it is copied
into packages and is not loaded by the worker as a runtime policy file. The
JSON schema is
`schema/rzn-mobile-workflow.schema.json`.

The catalog check reports the current workflow and tool counts. Run it after
any catalog change:

```bash
python3 scripts/validate_workflow_catalog.py --offline
```

## Minimum workflow shape

Each file must have:

- a non-empty `name`
- a non-empty `version` string. The current validator checks that it is
  non-empty; it does not enforce semantic-version syntax.
- a `capability` object with family, intent, surface, and `mutating`
- inputs with clear types and examples when a caller must provide a value
- a `steps` array that uses a tool

Read the JSON schema and the validator source for the complete field set.
The schema has no product-generation field. Package and workflow `version`
values are technical metadata used by validation and packaging.

## Add a workflow

1. Find a matching app and naming pattern.
2. Read the app metadata and one nearby workflow.
3. Write the smallest data-only flow.
4. Add a read-only example first.
5. Add `requiresCommit` to every data-changing step. This is an explicit step
   marker, not an automatic check derived from the tool name.
6. Add an execute input for a write flow. Default it to false.
7. Run the offline catalog check and focused Rust tests.
8. Test on a real phone when the change needs an app screen.

Do not hide a new selector rule in a shell script. Keep reusable behavior in
Rust or in workflow data.

## Run and inspect

```bash
rzn-phone list --compact
rzn-phone show safari/google_search
rzn-phone run safari/google_search --args-json '{"query":"example"}'
```

Use `--dry-run` or `--commit 0` for a write workflow. Use `--commit 1` only
after the workflow-specific execute flag is true and a person has approved
the action. Current workflow JSON uses `requiresCommit` and `saveAs`.

Private phone-data workflows need a matching `privacyGate` or
`privacyGates` value. A privacy grant controls access; it does not redact the
returned data. Treat Messages, one-time codes, Calls, and Notifications as
private. The catalog validator checks workflow JSON, not the YAML system
metadata.

Workflow names in the current JSON use slash IDs such as
`safari/google_search`. Use the same ID with `list`, `show`, and `run`.

## Direct tools

When no workflow fits, use the observe and act loop:

1. `ios.appium.ensure`
2. `ios.session.create`
3. `ios.ui.observe_compact`
4. `ios.action.*` or `ios.web.*`
5. Observe again and verify the result.

The direct tool list is available with `rzn-phone tool list --direct`.
