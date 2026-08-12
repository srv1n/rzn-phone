# RZN Phone Docs Map

Read the smallest thing that answers the current question. Do not dump the repo into context.

| Need | Read | Why |
| --- | --- | --- |
| Direct CLI usage | `references/cli-playbook.md`, then repo `README.md` | Runtime commands, workflows, direct tools, MCP setup |
| Install or repair runtime | `references/troubleshooting.md`, then repo `docs/agent_setup.md` | Appium, device, WDA, workflow-pack setup |
| Write or edit workflow JSON | `references/authoring.md`, then repo `docs/specs/rzn_mobile_workflow_v1.md` | Build loop first, full schema second |
| Use social browse/read/engage wrappers | repo `docs/specs/rzn_social_card_v1.md` and `cards/social/*.json` | Card contract and commit-gated social patterns |
| Understand runtime vs workflow-pack boundaries | repo `docs/DEEP_DIVE.md` | What belongs in Rust vs workflow packs |
| Diagnose repeat failures or fake-green validation | repo `docs/repeatable_workflow_validation_notes.md` | Stale geometry, Appium ownership, validation notes |
| Match app-specific patterns | repo `docs/reddit_workflows.md`, `docs/linkedin_workflows.md`, `docs/appstore_workflows.md` | Concrete args, selectors, overrides |
| Use packaged tester path | repo `docs/tester_kit.md` | Zip/install/MCP instructions for non-dev users |

## Repo Paths That Matter

```text
README.md
docs/agent_setup.md
docs/DEEP_DIVE.md
docs/specs/rzn_mobile_workflow_v1.md
docs/specs/rzn_social_card_v1.md
docs/repeatable_workflow_validation_notes.md
cards/social/*.json
crates/rzn_phone_worker/resources/workflows/*.json
crates/rzn_phone_worker/resources/systems/**/*
schema/rzn-mobile-workflow-v1.schema.json
scripts/rzn_phone.sh
```

`scripts/rzn_phone.sh` is repo-maintainer convenience glue. For agent-facing usage, prefer the installed `rzn-phone` CLI directly.

## Read Order

1. Run the direct CLI health/catalog commands before reading lots of docs.
2. Read one or two nearby workflow JSON files before authoring anything new.
3. Read the full spec only when changing workflow structure, step semantics, or output composition.
4. Read app-specific notes only for the app family being touched.
