# RZN Social Card Spec v1 (Draft)

`rzn.social.card.v1` defines a thin, portable card layer on top of existing `ios.workflow.run` workflows.
The objective is to standardize how agents browse, read, and engage across social apps while preserving strict safety controls.

## 1) Goals

- Normalize common social tasks: daily browsing, open/read, and engagement actions.
- Keep implementation data-driven: card metadata points to existing workflow ids.
- Preserve safety by default: mutating actions must use explicit `execute` args and `commit=true`.
- Support human-like pacing with bounded jitter (`util.sleep`) rather than fixed delays.

## 2) Card Object

```json
{
  "id": "reddit.comment_post",
  "title": "Comment on Reddit post",
  "app": "reddit",
  "mode": "engage",
  "workflow": "reddit.comment_post",
  "mutating": true,
  "commitRequired": true,
  "executeArg": "execute_comment",
  "defaults": {
    "post_index": 0,
    "execute_comment": false
  },
  "requiredArgs": ["comment_text"],
  "artifacts": ["result.json", "screenshot.png", "ui_source.xml"]
}
```

### Required fields

- `id`: globally unique card id (`<app>.<operation>`).
- `title`: short operator-facing label.
- `app`: app key (`linkedin`, `reddit`, ...).
- `mode`: one of `browse`, `read`, `engage`, `publish`, `moderate`.
- `workflow`: `ios.workflow.run` workflow name.
- `mutating`: whether it can change remote state.
- `commitRequired`: whether mutating steps are expected to be commit-gated.
- `defaults`: default workflow args object.

### Optional fields

- `description`: expanded behavior summary.
- `executeArg`: arg name that toggles the mutating path (`execute_like`, `execute_comment`, ...).
- `textArg`: arg name for freeform user/agent text (`comment_text`, `reply_text`, ...).
- `requiredArgs`: runtime args that must be present for successful execution.
- `artifacts`: expected output files produced by wrapper scripts.

## 3) Catalog Document

Cards are grouped into catalog files:

```json
{
  "schema_version": "rzn.social.card.v1",
  "catalog": "reddit",
  "version": "1.0.0",
  "cards": [ ... ]
}
```

## 4) Agent Safety Model

For mutating cards:

1. Run with dry defaults (`execute=false`, `commit=false`).
2. Inspect artifacts and policy checks.
3. Re-run with `execute=true` and `commit=true` only when approved.

This creates two independent gates:

- Workflow gate (`executeArg`) in card args.
- Runner gate (`commit=true`) enforced by `requiresCommit` steps.

## 5) Human-Like Pacing

Cards should reference workflows that include bounded sleep windows:

```json
{ "tool": "util.sleep", "arguments": { "minMs": 650, "maxMs": 1800 } }
```

Guidance:

- Use ranges, not single fixed durations.
- Keep waits bounded (`<= 60s`) to avoid runaway runs.
- Place pacing around high-signal interactions (opening posts, before mutating taps).

## 6) CLI Mapping

The wrapper commands use the catalog directly:

- `social-card-list`: enumerates cards from `cards/social/*.json`.
- `social-card-run`: resolves card id -> workflow + defaults, merges overrides, then runs `ios.workflow.run`.

Digest cards (for example `linkedin.daily_scroll` / `reddit.daily_scroll`) may emit derived files like `digest.json` and `thread.md` in addition to raw workflow artifacts.
