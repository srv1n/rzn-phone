# Direct Tool Loop

Use this when no shipped workflow matches the task cleanly.

Installed CLI path:

```bash
rzn-phone tool list --direct
rzn-phone tool show ios.session.create
rzn-phone tool call ios.appium.ensure
rzn-phone tool call ios.session.create --args-json '{"udid":"<UDID>","kind":"native_app","bundleId":"com.apple.mobilesafari"}'
rzn-phone tool call ios.ui.observe_compact --args-json '{"maxNodes":120}'
```

Recommended loop:

1. Ensure Appium with `ios.appium.ensure`.
2. Create or reuse a session with `ios.session.create`.
3. Observe with `ios.ui.observe_compact` for native apps or `ios.web.page_source` / `ios.web.wait_css` for Safari.
4. Act with `ios.action.*` or `ios.web.*`.
5. Re-observe before the next action.

Rules that keep this sane:

- Prefer shipped workflows first when they already fit.
- Stay read-only unless the task explicitly calls for commit-gated mutation.
- Use `ios.target.resolve` only when you actually need a raw locator.
- Use `ios.ui.screenshot` or `ios.ui.source` when the loop gets stuck.
