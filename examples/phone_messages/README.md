# Phone Messages Examples

These examples bridge the conceptual `phone_messages.*` system operations onto the current
`ios.script.run` primitives shipped by this repo.

What is true today:

- The bundle now carries `resources/systems/phone_messages/system.metadata.yaml`.
- The worker still exposes generic `ios.*` primitives rather than dedicated `phone_messages.*` tools.
- These example payloads are the compatibility bridge until first-class phone workflows land.

Safe defaults:

- All starter examples are read-only.
- They use `commit=false`.
- They background the app on exit rather than leaving Messages open.

Usage:

1. Replace `<UDID>` with the paired iPhone device id.
2. Send the JSON file as a `tools/call` payload to the worker.
3. Treat selectors as best-effort; tune the predicates if your iOS build labels differ.

Files:

- `list_recent_threads.tool_call.json`
- `read_latest_messages.tool_call.json`
