# CLI Playbook

Use the current CLI directly. Repo helper scripts are maintainer tooling, not
the agent runtime path.

## Health And Catalog

```bash
rzn-phone doctor
rzn-phone devices
rzn-phone status
rzn-phone capability list
rzn-phone list --compact
rzn-phone list --search review
rzn-phone list --family extract
rzn-phone show safari/google_search --example
rzn-phone tool list --direct
rzn-phone tool show ios.ui.observe_compact
```

If `rzn-phone list --compact` shows no workflows, refresh the pack:

```bash
rzn-phone workflows update
```

## Run A Workflow

Use slash ids:

```bash
rzn-phone run safari/google_search \
  --args-json '{"query":"best headphones 2026","limit":5}' \
  --json
```

The two-token form also works:

```bash
rzn-phone run safari google_search \
  --args-json '{"query":"best headphones 2026","limit":5}' \
  --json
```

Useful run flags:

| Flag | Use |
| --- | --- |
| `--udid <udid>` | Pin a device when auto-selection is not enough |
| `--json` | Return machine-readable result |
| `--dry-run` | Alias for no commit |
| `--commit` | Unlock `requiresCommit` steps |
| `--fast false` | Disable session smart cache for one run |
| `--disconnect-on-finish` | End automation session after the run |
| `--background-on-exit` | Press Home before teardown |
| `--lock-device-on-exit` | Lock the device before teardown |

## Mutating Workflow Safety

Mutating flows need two gates:

1. Workflow arg: `execute_like`, `execute_comment`, `execute_send`, `submit`, etc.
2. Runner gate: `--commit`

Dry-run:

```bash
rzn-phone run reddit/comment_post \
  --args-json '{"comment_text":"Draft only","execute_comment":false}' \
  --dry-run \
  --json
```

Approved live run:

```bash
rzn-phone run reddit/comment_post \
  --args-json '{"comment_text":"Approved text","execute_comment":true}' \
  --commit \
  --json
```

## LLM-Auto / Direct Tool Loop

Use this when no current workflow fits. It is the CLI form of the MCP
`ios.autonomy.loop` prompt.

```bash
rzn-phone tool call ios.appium.ensure
rzn-phone tool call ios.session.create \
  --args-json '{"udid":"<udid>","kind":"native_app","bundleId":"com.apple.mobilesafari"}'
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
rzn-phone tool call ios.action.tap --args-json '{"target":{"encodedId":"btn_1"},"commit":true}'
rzn-phone tool call ios.action.type \
  --args-json '{"target":{"encodedId":"fld_1"},"text":"hello","clearFirst":true,"commit":true}'
rzn-phone tool call ios.action.scroll --args-json '{"direction":"down","distance":0.65}'
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
```

Rules for this loop:

- Observe before acting.
- Take one action at a time.
- Re-observe after every state change.
- Use encoded ids only within the same observed screen.
- Use `ios.target.resolve` or `ios.ui.source` when you need durable locators for workflow JSON.
- Use `ios.web.*` only for Safari/web contexts.

## MCP Mapping

MCP clients can use the same runtime via:

```json
{
  "command": "/absolute/path/to/rzn-phone",
  "args": ["worker"],
  "env": {
    "RZN_IOS_APPIUM_URL": "http://127.0.0.1:4723"
  }
}
```

The worker exposes:

- tools such as `ios.workflow.run`, `ios.script.run`, `ios.ui.observe_compact`, and `ios.action.tap`
- prompt `ios.autonomy.loop` for workflow-first then direct-tool planning

For CLI use, prefer `rzn-phone run ...` over hand-written JSON-RPC.
