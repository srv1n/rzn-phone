# Phone Messages Examples

These examples call the first-class `phone_messages.*` MCP tools shipped by this repo.

What is true today:

- The bundle carries `resources/systems/phone_messages/system.metadata.yaml`.
- The `phone_messages.*` tools are connector-owned wrappers around the lower-level `ios.*` primitives.
- The tools are read-only in this release.

Safe defaults:

- All starter examples are read-only.
- They background the app on exit rather than leaving Messages open.

Usage:

1. Replace `<UDID>` with the paired iPhone device id.
2. Send the JSON file as a `tools/call` payload to the worker.
3. Treat selectors as best-effort; tune the underlying iOS primitives if your iOS build labels differ.

Files:

- `list_recent_threads.tool_call.json`
- `read_latest_messages.tool_call.json`
