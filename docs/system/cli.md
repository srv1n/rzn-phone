---
subject: cli
title: "CLI reference"
keywords: [cli, commands, worker, history, workflows, tools]
part_of: overview
describes: [crates/rzn_phone_worker/src/bin/rzn_phone_cli, crates/rzn_phone_worker/src/bin/rzn-phone.rs]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need a command or option that is not in the quick start."
skip_when: "You need workflow fields or maintainer packaging details."
---

# CLI reference

The terminal interface has two jobs: it runs the worker and it provides
commands for setup, inspection, workflows, and tools.

## Common commands

The executable name is `rzn-phone`.

```bash
rzn-phone doctor
rzn-phone setup
rzn-phone devices
rzn-phone list --compact
rzn-phone show safari/google_search
rzn-phone run safari/google_search --args-json '{"query":"example"}'
rzn-phone tool list --direct
rzn-phone tool show ios.ui.observe_compact
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
rzn-phone worker
```

`run` accepts the slash ID used by the JSON pack. It accepts
`--udid` for device selection, `--args-json` with inline JSON or `@file`,
`--commit`, `--dry-run`, and cleanup flags. The CLI smart-cache path keeps a
healthy session warm unless a cleanup flag asks it to stop.

## State and history

`config show` and `config path` show local settings. `recent`, `history`,
`rerun`, and `favorite` inspect or repeat saved runs. History uses the platform
state directory from `crates/rzn_phone_worker/src/config.rs` by default. Set
`RZN_PHONE_STATE_DIR` to move it, or set
`RZN_PHONE_HISTORY_DISABLED=1` or `RZN_PHONE_HISTORY=off` to disable recording.
`history redact` removes sensitive history entries. `--purge-state` on the
installer deletes this state; use it only when that loss is intended.

## Workflow pack commands

```bash
rzn-phone workflows path
rzn-phone workflows update --source /path/to/pack
```

An update source can be a local directory, an archive path, a `file://` URL, or
an HTTPS base URL. A pack needs a `VERSION` file and a matching `.sha256`
sidecar. Remote signed releases can also provide `.sig`.

## Reports and skills

`skill install`, `skill update`, `skill remove`, `skill status`, and `skill list`
manage installed agent guidance.
`report workflow-broken` prints a sanitized draft for host review. It sends
nothing and keeps no local report queue.

`completion bash` and `completion zsh` print the command completion scripts.

For every command and option, use:

```bash
rzn-phone --help
rzn-phone run --help
rzn-phone workflows update --help
```
