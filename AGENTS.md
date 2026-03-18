# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## Specs And Patterns (Start Here For New Apps)

- Mobile workflow format (portable): `docs/specs/rzn_mobile_workflow_v1.md`
  - Completion/cleanup controls: see `### 1.5 Completion and cleanup (runner options)`
- Social “card” layer (cross-app browse/read/engage): `docs/specs/rzn_social_card_v1.md`
  - Completion controls guidance: see `## 4.1 Completion controls (close out of the app)`
- Local CLI entrypoint for validating flows end-to-end: `scripts/ios_tools.sh`
  - Global flags for optional close-out behavior: `--disconnect-on-finish`, `--background-on-exit`, `--lock-device-on-exit`

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
