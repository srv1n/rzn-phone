# LinkedIn Workflow Notes (Real Device)

This repo now includes first-pass LinkedIn workflows for iOS real devices:

- `linkedin.read_feed`
- `linkedin.open_post`
- `linkedin.daily_scroll_digest`
- `linkedin.like_post`
- `linkedin.comment_post`
- `linkedin.reply_to_comment`
- `linkedin.create_post`
- `linkedin.update_latest_post`
- `linkedin.delete_latest_post`

These are data-only workflows loaded from `crates/rzn_phone_worker/resources/workflows/`.

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
./scripts/rzn_phone.sh build >/dev/null
./target/release/rzn-phone-worker <<'JSON'
{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"rzn-phone-cli","version":"0.1"}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":"wf-list-1","method":"tools/call","params":{"name":"ios.workflow.list","arguments":{}}}
JSON
```

Read feed (read-only):

```bash
./scripts/rzn_phone.sh linkedin-read-feed <udid> --limit 5 --out /tmp/linkedin-read
```

Daily scroll digest (read-only + parsed artifacts):

```bash
./scripts/rzn_phone.sh linkedin-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/linkedin-daily
```

This command writes:

- `result.json`: raw workflow output (`rows`, screenshots, UI source)
- `digest.json`: structured posts with parsed author/title/body/media/engagement
- `thread.md`: thread-style summary of high-engagement posts (`score >= min-engagement-score`)

Interaction targeting (read-only open + commit-gated actions):

```bash
./scripts/rzn_phone.sh linkedin-open-post <udid> --post-index 0 --max-feed-scrolls 6 --out /tmp/linkedin-open
./scripts/rzn_phone.sh linkedin-like-post <udid> --execute 0 --commit 0 --post-index 0 --out /tmp/linkedin-like-dry
./scripts/rzn_phone.sh linkedin-comment-post <udid> "Thanks for sharing this." --execute 0 --commit 0 --post-index 0 --out /tmp/linkedin-comment-dry
./scripts/rzn_phone.sh linkedin-reply-comment <udid> "Great callout." --execute 0 --commit 0 --post-index 0 --reply-index 0 --out /tmp/linkedin-reply-dry
```

Each command writes `result.json` and best-effort screenshot/UI-source artifacts; action workflows only mutate when both `--execute 1` and `--commit 1` are set.

Create post dry-run (prepare draft only, no submit):

```bash
./scripts/rzn_phone.sh linkedin-create-post <udid> "Testing RZN LinkedIn workflow draft" --submit 0 --commit 0 --out /tmp/linkedin-create-dry
```

Create post commit (actual publish):

```bash
./scripts/rzn_phone.sh linkedin-create-post <udid> "Testing RZN LinkedIn workflow post" --submit 1 --commit 1 --out /tmp/linkedin-create-live
```

Update post dry-run (open edit path and stage updated text):

```bash
./scripts/rzn_phone.sh linkedin-update-post <udid> "Updated copy from workflow runner" --execute 0 --commit 0 --out /tmp/linkedin-update-dry
```

Delete post dry-run (open delete path and stop before delete):

```bash
./scripts/rzn_phone.sh linkedin-delete-post <udid> --execute 0 --commit 0 --out /tmp/linkedin-delete-dry
```

## Override Selectors (Update/Delete)

Use environment overrides if your build/account differs:

```bash
LINKEDIN_POST_MENU_PREDICATE="label CONTAINS 'More actions'" \
LINKEDIN_EDIT_ACTION_PREDICATE="label CONTAINS 'Edit'" \
LINKEDIN_SAVE_ACTION_PREDICATE="label == 'Save'" \
LINKEDIN_DELETE_ACTION_PREDICATE="label CONTAINS 'Delete post'" \
LINKEDIN_CONFIRM_DELETE_PREDICATE="label == 'Delete'" \
./scripts/rzn_phone.sh linkedin-update-post <udid> "Updated text" --execute 1 --commit 1
```

## Override Selectors (Interaction Flows)

Use these environment overrides when your LinkedIn build/locale differs:

```bash
LINKEDIN_POST_CARD_PREDICATE="name CONTAINS 'feedUpdateCardA11yID'" \
LINKEDIN_POST_READY_PREDICATE="label CONTAINS 'Like' OR label CONTAINS 'Comment'" \
LINKEDIN_LIKE_BUTTON_PREDICATE="label CONTAINS 'Like'" \
LINKEDIN_COMMENT_BUTTON_PREDICATE="label CONTAINS 'Comment'" \
LINKEDIN_COMMENT_FIELD_PREDICATE="label CONTAINS 'Add a comment'" \
LINKEDIN_COMMENT_SUBMIT_PREDICATE="label CONTAINS 'Post comment'" \
LINKEDIN_REPLY_BUTTON_PREDICATE="label CONTAINS 'Reply'" \
LINKEDIN_REPLY_FIELD_PREDICATE="label CONTAINS 'Add a reply'" \
LINKEDIN_REPLY_SUBMIT_PREDICATE="label CONTAINS 'Post reply'" \
./scripts/rzn_phone.sh linkedin-comment-post <udid> "Nice insight." --execute 1 --commit 1
```

`linkedin-reply-comment` also supports `--target-comment-contains "<text>"` to scroll comments toward a matching thread before tapping a reply button.

## Agentic Pattern

For autonomous LM usage, keep this deterministic loop:

1. Run `linkedin-daily-scroll` to produce `digest.json` + `thread.md`.
2. Score/select candidate posts by policy (topic fit, risk, engagement threshold, recency).
3. Dry-run interaction command with `--execute 0 --commit 0` to verify selectors.
4. Re-run with `--execute 1 --commit 1` only when policy permits the action.
5. Persist artifacts for audit (`result.json`, screenshots, XML source).

## Safety Notes

- `linkedin.daily_scroll_digest`: read-only feed sweep; no commit-gated actions.
- `linkedin.open_post`: read-only targeting helper; no commit-gated actions.
- `linkedin.like_post`: Like tap is gated by `requiresCommit` and only runs when `args.execute_like=true`.
- `linkedin.comment_post`: Comment submit is gated by `requiresCommit` and only runs when `args.execute_comment=true`.
- `linkedin.reply_to_comment`: Reply submit is gated by `requiresCommit` and only runs when `args.execute_reply=true`.
- `linkedin.create_post`: submit step is gated by `requiresCommit` and only runs when `args.submit=true`.
- `linkedin.update_latest_post`: save step is gated by `requiresCommit` and only runs when `args.execute_update=true`.
- `linkedin.delete_latest_post`: delete + confirm taps are gated by `requiresCommit` and only run when `args.execute_delete=true`.
- Keep the phone unlocked through the full run; LinkedIn workflows fail fast if iOS locks during session bootstrap.
