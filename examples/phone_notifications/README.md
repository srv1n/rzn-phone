# Phone Notifications Examples

These examples cover the read-only starter flow for `phone_notifications`.

Important limitation:

- Notification Center is a system surface, not a normal app screen.
- The bundled worker exposes dedicated `phone_notifications.*` tools, but they still rely on generic gestures and UI extraction under the hood, so selector tuning is more likely than for Messages or Phone.

Starter file:

- `list_recent_notifications.tool_call.json`
- `filter_notifications_by_app.tool_call.json`

Usage:

1. Replace `<UDID>` before running the example.
2. Expect to tune the row predicate on some devices or locales.
3. Keep this read-only. Device-mutating notification actions are intentionally not promoted in this release.
