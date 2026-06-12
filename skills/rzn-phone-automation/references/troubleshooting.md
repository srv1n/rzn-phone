# Troubleshooting

## Fast Triage

| Symptom | Likely problem | Fix |
| --- | --- | --- |
| `rzn-phone: command not found` | Runtime not installed | Run `scripts/bootstrap_runtime.sh`; with a repo root it tries `make install` |
| `list` shows zero workflows | Broken or stale workflow pack | Run `rzn-phone workflows update`; if still empty, reinstall from repo or release artifact |
| `appium` missing | Local Node/Appium setup incomplete | `npm i -g appium` then `appium driver install xcuitest` |
| No phone in `rzn-phone devices` or `xcrun xctrace list devices` | Cable/trust/unlock/device transport issue | Reconnect, unlock, trust the computer, open Xcode once |
| Session create fails with code 65 | WebDriverAgent signing/provisioning issue | Set signing env vars and retry |
| Workflow taps the wrong row after scrolling | Stale geometry / stale candidate | Re-observe or search the current viewport before tapping |
| Mutating workflow only "works" when started with commit | Safety model is being bypassed | Dry-run first with `execute_*: false` and no `--commit` |

## Appium And XCUITest

Install local prerequisites:

```bash
xcode-select --install
npm i -g appium
appium driver install xcuitest
```

If Appium is already running elsewhere, point the runtime at it:

```bash
export RZN_IOS_APPIUM_URL="http://127.0.0.1:4723"
```

## WebDriverAgent Signing

When `ios.session.create` fails with code 65, stop pretending it is a workflow bug. It is usually Apple signing or provisioning.

Use or request:

```bash
export IOS_XCODE_ORG_ID="<apple-team-id>"
export IOS_XCODE_SIGNING_ID="Apple Development"
export IOS_UPDATED_WDA_BUNDLE_ID="com.example.WebDriverAgentRunner"
```

## Device Visibility

The phone must be:

- connected by cable
- unlocked
- trusted
- visible in `rzn-phone devices`

If the device is not visible, the workflow layer is dead on arrival.

## Workflow Authoring Failure Modes

- Do not keep app-specific hacks in Rust unless the failure is generic.
- Do not tap coordinates from one scroll pass after another scroll pass.
- Do not invent custom teardown logic inside every workflow.
- Do not mark a workflow done because it opened something. Verify it opened the right thing.
- Do not convert an encoded id from one observation into a durable workflow selector.

## Safe Recovery Order

```bash
scripts/bootstrap_runtime.sh
rzn-phone doctor
rzn-phone devices
rzn-phone list --compact
rzn-phone capability list
rzn-phone tool list --direct
```

Then run one read-only workflow. If a read-only workflow does not work, mutating flows have no business running yet.
