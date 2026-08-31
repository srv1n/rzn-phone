---
subject: safety
title: "Safety"
keywords: [safety, commit, dry run, privacy, mutation]
part_of: overview
describes: [crates/rzn_phone_worker/src/tools/policy.rs, crates/rzn_phone_worker/src/bin/rzn_phone_cli/args.rs, crates/rzn_phone_worker/resources/workflows]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need to read or write phone data, or change a safety rule."
skip_when: "You need normal read-only setup or packaging steps."
---

# Safety

## Default mode

Read-only work is the default. A workflow can prepare a write without sending
it. Review the result before you allow a side effect.

Direct write tools have an additional policy gate. They need `commit=true` and
the tool must be allowed for direct use. High-risk tools and raw JavaScript
also need the trusted-direct setting. This is an unsafe override; keep it off
unless a person accepts the risk.

## Checks for a write

A data-changing workflow step runs only when both explicit checks pass:

1. The workflow-specific execute or submit input is true.
2. The run has `commit=true` (`--commit 1` in the CLI).

`--dry-run` forces `commit=false`. Keep it in the first test of every write
flow.

Example:

```bash
rzn-phone run linkedin/create_post \
  --args-json '{"post_text":"Draft only","submit":false}' \
  --commit 0
```

## Private phone data

Messages, one-time codes, calls, and notifications are private data. The
matching tool or workflow must receive its privacy grant. A grant allows the
read; it does not redact the result. OTP reads can also return message text,
sender, and time. Notification rows can contain message or OTP text. Do not
index, log, or share this data without a clear retention and consent rule.

`RZN_PHONE_PRIVACY_GATES=all` grants every private class. Trusted-direct and
failure-artifact environment settings can also weaken normal boundaries. Treat
these settings as local emergency controls, never as normal configuration.

## Session cleanup

`rzn-phone shutdown` sends the required `commit=true` value for the explicit
cleanup request. Run options also support disconnect, Appium stop, app
backgrounding, and device lock controls.

## Change rules

- Keep selectors and app actions narrow.
- Re-observe after each state change.
- Workflow traces do not include raw screenshots or UI source by default. The
  `RZN_IOS_FAILURE_ARTIFACTS=full` setting can capture them; treat those files
  as private.
- Do not bypass `commit` or privacy checks to make a test pass.
- Treat a real-device check as separate from a static catalog check.
