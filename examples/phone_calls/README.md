# Phone Calls Examples

These examples call the first-class read-only `phone_calls.*` MCP tool shipped by this repo.

Starter file:

- `list_recent_calls.tool_call.json`

Notes:

- Replace `<UDID>` before running the example.
- Pass `privacyGate: "calls"`. Call records are private data; do not log or
  index the result.
- For normal local use, prefer:

```bash
rzn-phone tool call phone_calls.list_recent_calls \
  --args-json '{"deviceId":"<UDID>","privacyGate":"calls","maxCalls":25,"backgroundAppOnFinish":true}'
```

- Treat the `*.tool_call.json` file as a raw MCP payload example only when you are integrating at the worker protocol layer.
- The selectors under the wrapper are intentionally broad because Phone app accessibility labels vary by locale and iOS version.
- The quick start is read-only and keeps call placement out of the default path.
