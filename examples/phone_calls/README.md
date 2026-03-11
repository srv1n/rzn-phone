# Phone Calls Examples

These examples define the initial read-only quick start for the `phone_calls` system metadata.

Current bridge:

- System metadata talks in terms of `phone_calls.*` operations.
- The worker still runs them through `ios.script.run` and low-level `ios.*` tools.

Starter file:

- `list_recent_calls.tool_call.json`

Notes:

- Replace `<UDID>` before running the example.
- The selectors are intentionally broad because Phone app accessibility labels vary by locale and iOS version.
- The quick start is read-only and keeps call placement out of the default path.
