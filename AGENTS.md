# Agent Instructions

## Current product map

- Start with `docs/system/00-overview.md`.
- Use `docs/system/` for the current setup, runtime, safety, CLI, testing, and
  contribution answer.
- Workflow data lives in `crates/rzn_phone_worker/resources/workflows/`.
- The workflow contract is `schema/rzn-mobile-workflow.schema.json`.
- Local end-to-end helper: `scripts/rzn_phone.sh`.
  Its close-out flags are `--disconnect-on-finish`, `--background-on-exit`,
  and `--lock-device-on-exit`.

For the full terminal command surface, read `docs/system/cli.md`. For source
authority, read `docs/system/canon.md`.

## Review Before Git Actions

Unless the user explicitly says to commit, push, or open a pull request, stop after making the
requested changes and present the diff/context for review.

Default rule for this repo:

- Do not commit during back-and-forth review iterations.
- Do not push branches during back-and-forth review iterations.
- Do not open or suggest a pull request until the user says the work is approved and ready.
- Use local diffs, file references, and write-ups for review until the user gives the go-ahead.

## Landing the Plane (Session Completion)

**When ending a work session after explicit user approval to land the work**, you MUST complete ALL
steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **Document remaining work** - Include anything that still needs follow-up in the handoff
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
4. **Clean up** - Clear stashes, prune remote branches
5. **Verify** - All changes committed AND pushed
6. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
