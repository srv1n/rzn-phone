# LinkedIn Workflow Notes (Real Device)

This repo now includes first-pass LinkedIn workflows for iOS real devices:

- `linkedin.read_feed`
- `linkedin.daily_scroll_digest`
- `linkedin.create_post`
- `linkedin.update_latest_post`
- `linkedin.delete_latest_post`

These are data-only workflows loaded from `crates/rzn_ios_tools_worker/resources/workflows/`.

## Selector Baseline (Observed)

The defaults were seeded from live probes against the LinkedIn iOS app on March 4, 2026:

- Home tab button: `accessibility id = 12000` (`label=Home`)
- Post tab button: `accessibility id = 13634` (`label=Post`)
- Composer field: `accessibility id = 13617` (`label=What do you want to talk about?`)
- Composer cancel: `accessibility id = 13603` (`label=Cancel`)
- Composer submit: `accessibility id = 13602` (`label=Post`)
- Left nav menu entry: `accessibility id = 5600` (`label=Menu`)
- View profile in nav panel: `accessibility id = NavPanelIdentityViewProfileImageViewA11yID`
- Feed post cell prefix: `name BEGINSWITH feedUpdateCardA11yID`
- Premium overlay close: `LINPremiumFeedFullPageTakeoverUpsellCloseButtonViewA11yID`
- Premium overlay no-thanks: `LINPremiumFeedFullPageTakeoverUpsellCancelCTAButtonA11yID`

LinkedIn IDs vary by account, locale, and app build; update/delete workflows are intentionally parameterized for override.

## Run Commands

List workflows:

```bash
./scripts/ios_tools.sh build >/dev/null
./target/release/rzn_ios_tools_worker <<'JSON'
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ios-tools-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-list-1","method":"tools/call","params":{"name":"ios.workflow.list","arguments":{}}}
JSON
```

Read feed (read-only):

```bash
./scripts/ios_tools.sh linkedin-read-feed <udid> --limit 5 --out /tmp/linkedin-read
```

Daily scroll digest (read-only + parsed artifacts):

```bash
./scripts/ios_tools.sh linkedin-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/linkedin-daily
```

This command writes:

- `result.json`: raw workflow output (`rows`, screenshots, UI source)
- `digest.json`: structured posts with parsed author/title/body/media/engagement
- `thread.md`: thread-style summary of high-engagement posts (`score >= min-engagement-score`)

Create post dry-run (prepare draft only, no submit):

```bash
./scripts/ios_tools.sh linkedin-create-post <udid> "Testing RZN LinkedIn workflow draft" --submit 0 --commit 0 --out /tmp/linkedin-create-dry
```

Create post commit (actual publish):

```bash
./scripts/ios_tools.sh linkedin-create-post <udid> "Testing RZN LinkedIn workflow post" --submit 1 --commit 1 --out /tmp/linkedin-create-live
```

Update post dry-run (open edit path and stage updated text):

```bash
./scripts/ios_tools.sh linkedin-update-post <udid> "Updated copy from workflow runner" --execute 0 --commit 0 --out /tmp/linkedin-update-dry
```

Delete post dry-run (open delete path and stop before delete):

```bash
./scripts/ios_tools.sh linkedin-delete-post <udid> --execute 0 --commit 0 --out /tmp/linkedin-delete-dry
```

## Override Selectors (Update/Delete)

Use environment overrides if your build/account differs:

```bash
LINKEDIN_POST_MENU_PREDICATE="label CONTAINS 'More actions'" \
LINKEDIN_EDIT_ACTION_PREDICATE="label CONTAINS 'Edit'" \
LINKEDIN_SAVE_ACTION_PREDICATE="label == 'Save'" \
LINKEDIN_DELETE_ACTION_PREDICATE="label CONTAINS 'Delete post'" \
LINKEDIN_CONFIRM_DELETE_PREDICATE="label == 'Delete'" \
./scripts/ios_tools.sh linkedin-update-post <udid> "Updated text" --execute 1 --commit 1
```

## Safety Notes

- `linkedin.daily_scroll_digest`: read-only feed sweep; no commit-gated actions.
- `linkedin.create_post`: submit step is gated by `requiresCommit` and only runs when `args.submit=true`.
- `linkedin.update_latest_post`: save step is gated by `requiresCommit` and only runs when `args.execute_update=true`.
- `linkedin.delete_latest_post`: delete + confirm taps are gated by `requiresCommit` and only run when `args.execute_delete=true`.
- Keep the phone unlocked through the full run; LinkedIn workflows fail fast if iOS locks during session bootstrap.
