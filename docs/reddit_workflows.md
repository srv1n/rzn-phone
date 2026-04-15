# Reddit Workflow Notes (Real Device)

This repo now includes first-pass Reddit workflows for iOS real devices:

- `reddit.read_first_post`
- `reddit.comment_first_post`
- `reddit.open_post`
- `reddit.daily_scroll_digest`
- `reddit.like_post`
- `reddit.comment_post`
- `reddit.reply_to_comment`
- `reddit.open_inbox`
- `reddit.open_dm_thread`
- `reddit.send_dm`
- `reddit.send_dm_by_username`
- `reddit.reply_dm_thread`

These are data-only workflows loaded from `crates/rzn_phone_worker/resources/workflows/`.

## Run Commands

Read-only sweep:

```bash
./scripts/rzn_phone.sh reddit-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/reddit-daily
```

This command writes:

- `result.json`: raw workflow output (`rows`, screenshot, UI source)
- `digest.json`: structured posts (`author`, `title`, `body`, `engagement`)
- `thread.md`: thread-style summary of high-engagement posts

Interaction targeting (dry-run first):

```bash
./scripts/rzn_phone.sh reddit-open-post <udid> --post-index 0 --out /tmp/reddit-open
./scripts/rzn_phone.sh reddit-like-post <udid> --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-like-dry
./scripts/rzn_phone.sh reddit-comment-post <udid> "Interesting perspective." --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-comment-dry
./scripts/rzn_phone.sh reddit-reply-comment <udid> "Good point." --execute 0 --commit 0 --post-index 0 --reply-index 0 --out /tmp/reddit-reply-dry
```

DM targeting + send/reply (dry-run first):

```bash
./scripts/rzn_phone.sh reddit-open-inbox <udid> --out /tmp/reddit-open-inbox
./scripts/rzn_phone.sh reddit-open-dm-thread <udid> --thread-index 0 --out /tmp/reddit-open-dm-thread
./scripts/rzn_phone.sh reddit-send-dm <udid> "Hey there" --execute 0 --commit 0 --thread-index 0 --out /tmp/reddit-send-dm-dry
./scripts/rzn_phone.sh reddit-send-dm-user <udid> "chorefit" "Hey there" --execute 0 --commit 0 --out /tmp/reddit-send-dm-user-dry
./scripts/rzn_phone.sh reddit-reply-dm <udid> "Following up" --execute 0 --commit 0 --thread-index 0 --out /tmp/reddit-reply-dm-dry
```

Single-session operation (reduces repeated session bootstrap between actions):

```bash
RZN_PHONE_SKIP_BUILD=1 \
./scripts/rzn_phone.sh reddit-engage-seq <udid> "Test dry-run comment" \
  --execute-like 0 --execute-comment 0 --commit 0 --out /tmp/reddit-engage-seq
```

Optional completion controls (any workflow command):

```bash
./scripts/rzn_phone.sh reddit-like-post <udid> --execute 1 --commit 1 \
  --background-on-exit 1 --lock-device-on-exit 1
```

Each command writes `result.json` and best-effort screenshot/UI-source artifacts.
Mutations execute only when both `--execute 1` and `--commit 1` are provided.

## Selector Overrides

Use environment overrides if your app build/locale differs:

```bash
REDDIT_POST_CELL_PREDICATE="name CONTAINS 'reddit_feed__post__post_cell'" \
REDDIT_POST_OPEN_PREDICATE="name CONTAINS 'reddit_feed__post__title_text'" \
REDDIT_POST_READY_PREDICATE="label CONTAINS 'Comment'" \
REDDIT_LIKE_BUTTON_PREDICATE="label CONTAINS[c] 'upvote'" \
REDDIT_COMMENT_FIELD_PREDICATE="label CONTAINS[c] 'comment'" \
REDDIT_COMMENT_SUBMIT_PREDICATE="label == 'Reply' OR label == 'Post'" \
REDDIT_REPLY_BUTTON_PREDICATE="label CONTAINS[c] 'reply'" \
REDDIT_REPLY_FIELD_PREDICATE="label CONTAINS[c] 'reply'" \
REDDIT_REPLY_SUBMIT_PREDICATE="label == 'Reply' OR label == 'Send'" \
./scripts/rzn_phone.sh reddit-comment-post <udid> "Nice write-up." --execute 1 --commit 1
```

DM flows support additional overrides:

```bash
REDDIT_INBOX_TAB_PREDICATE="label CONTAINS[c] 'Inbox' OR label CONTAINS[c] 'Chat'" \
REDDIT_DM_THREAD_ROW_PREDICATE="type == 'XCUIElementTypeCell'" \
REDDIT_DM_THREAD_READY_PREDICATE="label CONTAINS[c] 'Message' OR value CONTAINS[c] 'Message'" \
REDDIT_DM_MESSAGE_FIELD_PREDICATE="label CONTAINS[c] 'Message' OR value CONTAINS[c] 'Message'" \
REDDIT_DM_SEND_BUTTON_PREDICATE="label == 'Send'" \
./scripts/rzn_phone.sh reddit-send-dm <udid> "hello" --execute 1 --commit 1 --thread-index 0
```

If the app is already inside a post detail view, you can broaden `REDDIT_POST_CELL_PREDICATE` to include title nodes:
`name CONTAINS 'reddit_feed__post__title_text' OR name CONTAINS 'reddit_feed__post__post_cell'`.

## Agentic Pattern

1. Run `reddit-daily-scroll` to produce `digest.json` + `thread.md`.
2. Score/select posts by policy.
3. Dry-run interactions (`--execute 0 --commit 0`).
4. Re-run with `--execute 1 --commit 1` only for approved actions.
5. Keep artifacts for audit (`result.json`, screenshot, XML source).

DM pattern:

1. Run `reddit-open-inbox` or `reddit-open-dm-thread` to verify thread targeting.
2. Dry-run `reddit-send-dm` / `reddit-reply-dm` (`--execute 0 --commit 0`).
3. Re-run with `--execute 1 --commit 1` only after explicit approval.

## Safety Notes

- `reddit.daily_scroll_digest`, `reddit.open_post`, `reddit.open_inbox`, and `reddit.open_dm_thread` are read-only.
- `reddit.like_post` uses `requiresCommit` and only mutates when `execute_like=true`.
- `reddit.comment_post` uses `requiresCommit` and only mutates when `execute_comment=true`.
- `reddit.reply_to_comment` uses `requiresCommit` and only mutates when `execute_reply=true`.
- `reddit.send_dm` uses `requiresCommit` and only mutates when `execute_send=true`.
- `reddit.send_dm_by_username` uses `requiresCommit` and only mutates when `execute_send=true`.
- `reddit.reply_dm_thread` uses `requiresCommit` and only mutates when `execute_reply=true`.
- Keep the phone unlocked during session bootstrap and run execution.
