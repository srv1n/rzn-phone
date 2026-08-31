---
subject: contributing
title: "Contributing"
keywords: [contributing, change, test, review, documentation]
part_of: overview
describes: [Makefile, docs, scripts, crates/rzn_phone_worker]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to change code, workflow data, scripts, or documentation."
skip_when: "You only need to run the current CLI."
---

# Contributing

Use this page before you change the project. Start with the smallest source
that owns the behavior.

## Before you edit

1. Read this page and the narrow page in `docs/system/`.
2. Check `git status --short --branch`.
3. Read the relevant Rust module, workflow, schema, or script.
4. Keep the change small.

Do not remove or rewrite a dirty file that another person owns.

## Change the right source

- Change Rust for shared runtime behavior.
- Change workflow JSON for one named app flow.
- Change the schema when the workflow contract changes.
- Change scripts for build, release, or test helpers.
- Change `docs/system/` for the short canonical explanation.

## Checks

Use the smallest check that proves the change:

```bash
make test
python3 scripts/validate_workflow_catalog.py --offline
```

Use `make release-check` for release work. Use a real iPhone check when the
change depends on an app screen, selector, signing, or Appium behavior.

## Documentation rules

- Use short sentences and common words.
- Use one term for one thing.
- Use active voice.
- State limits and unknowns.
- Give an exact command when a command is needed.
- Update `last_verified` after a source check.
- Run `tusker docs map` and `tusker validate` after canonical doc changes.

## Task tracking

Use the local V7 tracker for work records. Do not hand-edit task status, proof,
or generated indexes. Use the CLI to create, move, verify, and close work.
