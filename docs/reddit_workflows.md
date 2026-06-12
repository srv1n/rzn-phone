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

Use the public CLI directly for runtime invocation:

```bash
rzn-phone list reddit
rzn-phone show reddit/open_post
```

Read-only sweep:

```bash
rzn-phone run reddit/daily_scroll_digest \
  --udid <udid> \
  --args-json '{"max_posts":30,"max_scrolls":8,"min_dwell_ms":650,"max_dwell_ms":1800}' \
  --json > /tmp/reddit-daily.json
```

The public CLI returns the workflow JSON directly. If you want decoded screenshots, XML source
files, or a scored digest/thread summary, build that downstream from the JSON instead of teaching
the repo-local wrapper.

Interaction targeting (dry-run first):

```bash
rzn-phone run reddit/open_post \
  --udid <udid> \
  --args-json '{"post_index":0}' \
  --json > /tmp/reddit-open.json

rzn-phone run reddit/like_post \
  --udid <udid> \
  --args-json '{"execute_like":false,"post_index":0}' \
  --commit 0 \
  --json > /tmp/reddit-like-dry.json

rzn-phone run reddit/comment_post \
  --udid <udid> \
  --args-json '{"comment_text":"Interesting perspective.","execute_comment":false,"post_index":0}' \
  --commit 0 \
  --json > /tmp/reddit-comment-dry.json

rzn-phone run reddit/reply_to_comment \
  --udid <udid> \
  --args-json '{"reply_text":"Good point.","execute_reply":false,"post_index":0,"reply_index":0,"max_comment_scrolls":6}' \
  --commit 0 \
  --json > /tmp/reddit-reply-dry.json
```

DM targeting + send/reply (dry-run first):

```bash
rzn-phone run reddit/open_inbox \
  --udid <udid> \
  --args-json '{}' \
  --json > /tmp/reddit-open-inbox.json

rzn-phone run reddit/open_dm_thread \
  --udid <udid> \
  --args-json '{"thread_index":0,"max_thread_scrolls":8}' \
  --json > /tmp/reddit-open-dm-thread.json

rzn-phone run reddit/send_dm \
  --udid <udid> \
  --args-json '{"message_text":"Hey there","execute_send":false,"thread_index":0,"max_thread_scrolls":8}' \
  --commit 0 \
  --json > /tmp/reddit-send-dm-dry.json

rzn-phone run reddit/send_dm_by_username \
  --udid <udid> \
  --args-json '{"username":"chorefit","message_text":"Hey there","execute_send":false,"max_thread_scrolls":8}' \
  --commit 0 \
  --json > /tmp/reddit-send-dm-user-dry.json

rzn-phone run reddit/reply_dm_thread \
  --udid <udid> \
  --args-json '{"message_text":"Following up","execute_reply":false,"thread_index":0,"max_thread_scrolls":8}' \
  --commit 0 \
  --json > /tmp/reddit-reply-dm-dry.json
```

Warm-session sequence:

```bash
rzn-phone run reddit/open_post \
  --udid <udid> \
  --args-json '{"post_index":0}'

rzn-phone run reddit/like_post \
  --udid <udid> \
  --args-json '{"execute_like":false,"post_index":0}' \
  --commit 0

rzn-phone run reddit/comment_post \
  --udid <udid> \
  --args-json '{"comment_text":"Test dry-run comment","execute_comment":false,"post_index":0}' \
  --commit 0
```

Optional completion controls (any workflow command):

```bash
rzn-phone run reddit/like_post \
  --udid <udid> \
  --args-json '{"execute_like":true,"post_index":0}' \
  --commit 1 \
  --background-on-exit 1 --lock-device-on-exit 1
```

Leave fast mode enabled and the runtime will reuse a warm session between runs when it can. No
special repo-only `reddit-engage-seq` syntax needs to be taught.

Each command returns workflow JSON. Mutations execute only when both the workflow arg
(`execute_like`, `execute_comment`, `execute_reply`, `execute_send`) is `true` and `--commit 1`
is provided.

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
rzn-phone run reddit/comment_post \
  --udid <udid> \
  --args-json '{"comment_text":"Nice write-up.","execute_comment":true,"post_index":0}' \
  --commit 1
```

DM flows support additional overrides:

```bash
REDDIT_INBOX_TAB_PREDICATE="label CONTAINS[c] 'Inbox' OR label CONTAINS[c] 'Chat'" \
REDDIT_DM_THREAD_ROW_PREDICATE="type == 'XCUIElementTypeCell'" \
REDDIT_DM_THREAD_READY_PREDICATE="label CONTAINS[c] 'Message' OR value CONTAINS[c] 'Message'" \
REDDIT_DM_MESSAGE_FIELD_PREDICATE="label CONTAINS[c] 'Message' OR value CONTAINS[c] 'Message'" \
REDDIT_DM_SEND_BUTTON_PREDICATE="label == 'Send'" \
rzn-phone run reddit/send_dm \
  --udid <udid> \
  --args-json '{"message_text":"hello","execute_send":true,"thread_index":0,"max_thread_scrolls":8}' \
  --commit 1
```

If the app is already inside a post detail view, you can broaden `REDDIT_POST_CELL_PREDICATE` to include title nodes:
`name CONTAINS 'reddit_feed__post__title_text' OR name CONTAINS 'reddit_feed__post__post_cell'`.

## Agentic Pattern

1. Run `rzn-phone run reddit/daily_scroll_digest ... --json`.
2. Score/select posts by policy from the returned workflow JSON.
3. Dry-run interactions with `execute_*: false` and `--commit 0`.
4. Re-run with `execute_*: true` and `--commit 1` only for approved actions.
5. Decode screenshots or XML from the JSON only when you actually need them.

DM pattern:

1. Run `reddit/open_inbox` or `reddit/open_dm_thread` to verify thread targeting.
2. Dry-run `reddit/send_dm` or `reddit/reply_dm_thread` with `execute_*: false` and `--commit 0`.
3. Re-run with `execute_*: true` and `--commit 1` only after explicit approval.

## Safety Notes

- `reddit.daily_scroll_digest`, `reddit.open_post`, `reddit.open_inbox`, and `reddit.open_dm_thread` are read-only.
- `reddit.like_post` uses `requiresCommit` and only mutates when `execute_like=true`.
- `reddit.comment_post` uses `requiresCommit` and only mutates when `execute_comment=true`.
- `reddit.reply_to_comment` uses `requiresCommit` and only mutates when `execute_reply=true`.
- `reddit.send_dm` uses `requiresCommit` and only mutates when `execute_send=true`.
- `reddit.send_dm_by_username` uses `requiresCommit` and only mutates when `execute_send=true`.
- `reddit.reply_dm_thread` uses `requiresCommit` and only mutates when `execute_reply=true`.
- Keep the phone unlocked during session bootstrap and run execution.
