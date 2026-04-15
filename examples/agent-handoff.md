# Agent Handoff For rzn-phone

Use this file when an agent is asked to set up or diagnose the local `rzn-phone` capability.

## Objective

Set up the shipped `rzn-phone` plugin on this machine and validate it with one read-only workflow.

## Required steps

1. Run:

```bash
./scripts/tester_doctor.sh
```

2. Confirm:

- Appium is installed
- the Appium `xcuitest` driver is installed
- a physical iPhone is visible in `xcrun xctrace list devices`

3. Configure the MCP client with:

- `command`: `bin/macos/universal/rzn-phone-worker`
- `RZN_PLUGIN_DIR`: the unpacked plugin root
- `RZN_IOS_APPIUM_URL`: usually `http://127.0.0.1:4723`

4. Start with:

- `ios.env.doctor`
- `ios.device.list`
- `ios.workflow.list`

5. Run one read-only workflow only.

## Safe workflow choices

- `safari.google_search`
- `appstore.typeahead`
- `appstore.search_results`
- `reddit.read_first_post`
- `reddit.daily_scroll_digest`
- `linkedin.read_feed`
- `linkedin.daily_scroll_digest`

## Do not do this by default

- do not use `commit=true`
- do not run mutating Reddit workflows
- do not run mutating LinkedIn workflows

## Most likely blockers

- Appium missing
- Appium `xcuitest` driver missing
- phone not trusted or unlocked
- WebDriverAgent signing failure
- wrong `RZN_PLUGIN_DIR`

## If WDA signing fails

Use or request:

```bash
export IOS_XCODE_ORG_ID="<apple-team-id>"
export IOS_XCODE_SIGNING_ID="Apple Development"
export IOS_UPDATED_WDA_BUNDLE_ID="com.example.WebDriverAgentRunner"
```

## Done criteria

The setup is complete only when:

1. prerequisites pass
2. the phone is visible
3. the workflow pack is loaded
4. one read-only workflow succeeds
