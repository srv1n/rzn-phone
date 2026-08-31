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

- Replace `<UDID>` with the paired iPhone device id.
- These are private reads. Pass `privacyGate` with value `messages` for normal
  message reads or `otp` for the OTP example. The result can contain message
  text; do not log or index it.
- For normal local use, prefer the CLI:

```bash
rzn-phone tool call phone_messages.list_recent_threads \
  --args-json '{"deviceId":"<UDID>","privacyGate":"messages","maxThreads":25,"backgroundAppOnFinish":true}'
```

- Treat the `*.tool_call.json` files as raw MCP `tools/call` payload examples when you are integrating at the worker protocol layer.
- Treat selectors as best-effort; tune the underlying iOS primitives if your iOS build labels differ.

Files:

- `list_recent_threads.tool_call.json`
- `read_latest_messages.tool_call.json`
- `find_recent_otp.tool_call.json`
