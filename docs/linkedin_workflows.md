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

Use the public CLI directly for runtime invocation. Quick sanity check:

```bash
rzn-phone list linkedin
rzn-phone show linkedin/open_post
```

Read feed (read-only):

```bash
rzn-phone run linkedin/read_feed \
  --udid <udid> \
  --args-json '{"limit":5}' \
  --json > /tmp/linkedin-read.json
```

Daily scroll digest (read-only + parsed artifacts):

```bash
rzn-phone run linkedin/daily_scroll_digest \
  --udid <udid> \
  --args-json '{"max_posts":30,"max_scrolls":8}' \
  --json > /tmp/linkedin-daily.json
```

The public CLI returns structured workflow JSON directly. If you want extra digest files,
engagement scoring, or decoded screenshot/XML blobs, layer that on top of the JSON instead of
teaching the repo-local wrapper commands.

Interaction targeting (read-only open + commit-gated actions):

```bash
rzn-phone run linkedin/open_post \
  --udid <udid> \
  --args-json '{"post_index":0,"max_feed_scrolls":6}' \
  --json > /tmp/linkedin-open.json

rzn-phone run linkedin/like_post \
  --udid <udid> \
  --args-json '{"execute_like":false,"post_index":0,"max_feed_scrolls":6}' \
  --commit 0 \
  --json > /tmp/linkedin-like-dry.json

rzn-phone run linkedin/comment_post \
  --udid <udid> \
  --args-json '{"comment_text":"Thanks for sharing this.","execute_comment":false,"post_index":0,"max_feed_scrolls":6}' \
  --commit 0 \
  --json > /tmp/linkedin-comment-dry.json

rzn-phone run linkedin/reply_to_comment \
  --udid <udid> \
  --args-json '{"reply_text":"Great callout.","execute_reply":false,"post_index":0,"reply_index":0,"max_feed_scrolls":6,"max_comment_scrolls":6}' \
  --commit 0 \
  --json > /tmp/linkedin-reply-dry.json
```

Each command returns workflow JSON; action workflows only mutate when both the workflow arg
(`execute_like`, `execute_comment`, `execute_reply`, `submit`, `execute_update`, `execute_delete`)
is `true` and `--commit 1` is set.

Create post dry-run (prepare draft only, no submit):

```bash
rzn-phone run linkedin/create_post \
  --udid <udid> \
  --args-json '{"post_text":"Testing RZN LinkedIn workflow draft","submit":false}' \
  --commit 0 \
  --json > /tmp/linkedin-create-dry.json
```

Create post commit (actual publish):

```bash
rzn-phone run linkedin/create_post \
  --udid <udid> \
  --args-json '{"post_text":"Testing RZN LinkedIn workflow post","submit":true}' \
  --commit 1 \
  --json > /tmp/linkedin-create-live.json
```

Update post dry-run (open edit path and stage updated text):

```bash
rzn-phone run linkedin/update_latest_post \
  --udid <udid> \
  --args-json '{"updated_text":"Updated copy from workflow runner","execute_update":false,"post_index":0,"max_profile_scrolls":6}' \
  --commit 0 \
  --json > /tmp/linkedin-update-dry.json
```

Delete post dry-run (open delete path and stop before delete):

```bash
rzn-phone run linkedin/delete_latest_post \
  --udid <udid> \
  --args-json '{"execute_delete":false,"post_index":0,"max_profile_scrolls":6}' \
  --commit 0 \
  --json > /tmp/linkedin-delete-dry.json
```

## Override Selectors (Update/Delete)

Use environment overrides if your build/account differs:

```bash
LINKEDIN_POST_MENU_PREDICATE="label CONTAINS 'More actions'" \
LINKEDIN_EDIT_ACTION_PREDICATE="label CONTAINS 'Edit'" \
LINKEDIN_SAVE_ACTION_PREDICATE="label == 'Save'" \
LINKEDIN_DELETE_ACTION_PREDICATE="label CONTAINS 'Delete post'" \
LINKEDIN_CONFIRM_DELETE_PREDICATE="label == 'Delete'" \
rzn-phone run linkedin/update_latest_post \
  --udid <udid> \
  --args-json '{"updated_text":"Updated text","execute_update":true,"post_index":0,"max_profile_scrolls":6}' \
  --commit 1
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
rzn-phone run linkedin/comment_post \
  --udid <udid> \
  --args-json '{"comment_text":"Nice insight.","execute_comment":true,"post_index":0,"max_feed_scrolls":6}' \
  --commit 1
```

`linkedin/reply_to_comment` also supports `target_comment_contains` to scroll comments toward a
matching thread before tapping a reply button.

## Agentic Pattern

For autonomous LM usage, keep this deterministic loop:

1. Run `rzn-phone run linkedin/daily_scroll_digest ... --json`.
2. Score/select candidate posts by policy (topic fit, risk, engagement threshold, recency).
3. Dry-run interaction commands with `execute_*: false` and `--commit 0` to verify selectors.
4. Re-run with `execute_*: true` and `--commit 1` only when policy permits the action.
5. Decode screenshots or XML from the JSON only when you actually need them.

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
