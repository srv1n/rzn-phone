# Phone Notifications Examples

These examples cover the read-only starter flow for `phone_notifications`.

Important limitation:

- Notification Center is a system surface, not a normal app screen.
- The bundled worker exposes dedicated `phone_notifications.*` tools, but they still rely on generic gestures and UI extraction under the hood, so selector tuning is more likely than for Messages or Phone.

Starter file:

- `list_recent_notifications.tool_call.json`
- `filter_notifications_by_app.tool_call.json`

Usage:

- Replace `<UDID>` before running the example.
- For normal local use, prefer:

```bash
rzn-phone tool call phone_notifications.list_recent_notifications \
  --args-json '{"deviceId":"<UDID>","maxNotifications":25,"backgroundAppOnFinish":false}'
```

- Treat the `*.tool_call.json` files as raw MCP payload examples only when you are integrating at the worker protocol layer.
- Expect to tune the row predicate on some devices or locales.
- Keep this read-only. Device-mutating notification actions are intentionally not promoted in this release.
