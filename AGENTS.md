# Agent Instructions

## Specs And Patterns (Start Here For New Apps)

- Mobile workflow format (portable): `docs/specs/rzn_mobile_workflow_v1.md`
  - Completion/cleanup controls: see `### 1.5 Completion and cleanup (runner options)`
- Social “card” layer (cross-app browse/read/engage): `docs/specs/rzn_social_card_v1.md`
  - Completion controls guidance: see `## 4.1 Completion controls (close out of the app)`
- Local CLI entrypoint for validating flows end-to-end: `scripts/rzn_phone.sh`
  - Global flags for optional close-out behavior: `--disconnect-on-finish`, `--background-on-exit`, `--lock-device-on-exit`

## Plugin Release Requirement

If the task includes building or publishing the public `rzn-phone` capability bundle, release
completion also requires backend notification using the contract documented at:

- `/Users/sarav/Downloads/side/rzn/backend/docs/runbook/plugin_team_release_guide.md`

For plugin release work:

- Building a ZIP alone is not enough.
- Notify the backend through the release registration and catalog publish API flow.
- Publish to local `http://localhost:8082` first, then cloud `https://cloud.rzn.ai`, unless the user explicitly says otherwise.
- The release script supports `cloud` directly and retains `prod` as a legacy alias.
- If local or cloud publish fails at any stage, stop and report exactly what failed.

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
