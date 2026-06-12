# Browser Automation CLI Run Alignment

## Recommendation

The Browser Automation and Phone Automation CLIs should share one execution verb:

- `run`

`native-run` is a bad name because it exposes plumbing instead of intent. Users are not trying to “do something native.” They are trying to run a workflow.

## Proposed shared grammar

| Intent | Shared command shape | Notes |
| --- | --- | --- |
| List workflows | `list [system|query]` | catalog browsing |
| Show workflow | `show <system>/<workflow>` | metadata + inputs |
| Run workflow | `run <system> <workflow> ...` | main execution path |
| Alternate ref form | `run system/workflow ...` | canonical id form |
| Direct low-level call | `tool list/show/call ...` | matches MCP terminology |

## Browser Automation transport

If browser still needs multiple execution backends, put that behind a flag instead of naming whole commands after the transport:

```bash
rzn-browser run google search --via native
rzn-browser run google/search --via desktop
```

If one backend is the obvious default, make `--via` optional and default it.

## Phone shape

Phone now uses:

```bash
rzn-phone run safari google_search --udid <udid>
rzn-phone run safari/google_search --udid <udid>
rzn-phone list safari
rzn-phone show safari/google_search
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
```

## Why this is better

1. The verb is stable across products.
2. The workflow reference grammar is stable across products.
3. Transport stops leaking into the user-facing noun.
4. LLM prompting gets simpler because there is one obvious action word: `run`.

## Naming judgment

| Name | Verdict | Why |
| --- | --- | --- |
| `run` | good | clear, short, intent-first |
| `workflow run` | acceptable alias | slightly redundant |
| `run-workflow` | clunky | sounds like an internal helper |
| `native-run` | bad | implementation detail disguised as UX |
| `desktop-run` | bad | same problem, different costume |

## Migration suggestion for `rzn-browser`

1. Add `run` as the preferred surface.
2. Keep `native-run` / `desktop-run` as temporary aliases if needed.
3. Update docs/examples/prompts to teach only `run`.
4. Remove the old command names after the alias window.
