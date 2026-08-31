# Workflow Authoring

This repo wants deterministic workflow JSON, not a pile of one-off scripts. Use direct `rzn-phone` CLI commands while discovering behavior, then promote stable steps into `crates/rzn_phone_worker/resources/workflows/*.json`.

## Decision Rule

| Situation | Move |
| --- | --- |
| A current workflow is close | Patch that JSON and preserve its style |
| Navigation/selectors are unclear | Discover with direct `ios.*` tools first |
| The task is a one-off but deterministic | Use `ios.script.run` with a step array |
| The task should be reusable | Add or patch workflow JSON |
| The bug is generic session/observe/scroll behavior | Fix runtime/helpers, not only the workflow |
| The job is social browse/read/engage | Reuse social-card and commit-gate patterns |

## Build Paths

### Path A: Existing Workflow

Use this when the catalog has the right app and roughly the right outcome.

```bash
rzn-phone list linkedin
rzn-phone show linkedin/comment_post --example
rzn-phone run linkedin/comment_post \
  --args-json '{"comment_text":"Draft only","execute_comment":false}' \
  --dry-run \
  --json
```

Patch the nearest sibling workflow. Keep its naming, input style, pacing, `saveAs`, and output shape unless the task is specifically to change the contract.

### Path B: LLM-Auto / Direct Discovery

Use this when the app path is fuzzy or the workflow does not exist.

```bash
rzn-phone capability list
rzn-phone tool list --direct
rzn-phone tool show ios.session.create
rzn-phone tool call ios.appium.ensure
rzn-phone tool call ios.session.create \
  --args-json '{"udid":"<udid>","kind":"native_app","bundleId":"com.apple.mobilesafari"}'
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
```

Then loop:

1. Observe compact UI state.
2. Act with one `ios.action.*` or `ios.web.*` command.
3. Re-observe immediately.
4. Save only the steps that survive repeated runs.
5. Convert the stable sequence into workflow JSON.

Use encoded ids for short-lived discovery. For workflow JSON, prefer stable locators such as accessibility id or iOS predicate when available. Encoded ids come from the latest observation and are not a durable workflow contract.

### Path C: Inline Script Run

Use `ios.script.run` when you need a deterministic step list before committing to a named workflow.

```bash
rzn-phone tool call ios.script.run \
  --args-json '{"steps":[{"tool":"ios.appium.ensure","arguments":{}},{"tool":"ios.ui.observe_compact","arguments":{"maxNodes":120}}],"commit":false}'
```

If the step list becomes reusable, move it into workflow JSON rather than keeping it as a shell snippet.

## What Good Workflow JSON Looks Like

- Small deterministic steps.
- `help.examples` that become runnable CLI guidance.
- Explicit `required_variables` and input metadata for required args.
- `saveAs` for important intermediate output.
- `output` composition instead of giant opaque blobs.
- Mutating steps guarded by both an input flag and `requiresCommit: true`.
- Runtime cleanup controlled by invocation flags, not workflow teardown steps.
- Current-screen targeting. If you scroll, re-observe or search again before tapping.

## Validation Loop

For one workflow:

```bash
rzn-phone run reddit/open_post \
  --udid <udid> \
  --args-json '{"post_index":0}' \
  --json
```

For mutating workflows:

```bash
rzn-phone run reddit/like_post \
  --udid <udid> \
  --args-json '{"execute_like":false,"post_index":0}' \
  --dry-run \
  --json
```

Then, only after explicit approval:

```bash
rzn-phone run reddit/like_post \
  --udid <udid> \
  --args-json '{"execute_like":true,"post_index":0}' \
  --commit \
  --json
```

Use catalog validators only when you are doing repo-maintainer work and need pack-wide confidence. Public agent instructions should stick to `rzn-phone` CLI and MCP tools.

## Packaging And Refresh

If workflow pack content changed and the installed runtime needs the update:

```bash
make workflows-pack
rzn-phone workflows update
```

If the installed runtime is stale or broken while you are inside the repo, `make install` is the blunt repair move.
