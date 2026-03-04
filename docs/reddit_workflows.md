# Reddit Workflow Notes (Real Device)

This repo now includes first-pass Reddit workflows for iOS real devices:

- `reddit.read_first_post`
- `reddit.comment_first_post`
- `reddit.open_post`
- `reddit.daily_scroll_digest`
- `reddit.like_post`
- `reddit.comment_post`
- `reddit.reply_to_comment`

These are data-only workflows loaded from `crates/rzn_ios_tools_worker/resources/workflows/`.

## Run Commands

Read-only sweep:

```bash
./scripts/ios_tools.sh reddit-daily-scroll <udid> --max-posts 30 --max-scrolls 8 --min-engagement-score 20 --out /tmp/reddit-daily
```

This command writes:

- `result.json`: raw workflow output (`rows`, screenshot, UI source)
- `digest.json`: structured posts (`author`, `title`, `body`, `engagement`)
- `thread.md`: thread-style summary of high-engagement posts

Interaction targeting (dry-run first):

```bash
./scripts/ios_tools.sh reddit-open-post <udid> --post-index 0 --out /tmp/reddit-open
./scripts/ios_tools.sh reddit-like-post <udid> --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-like-dry
./scripts/ios_tools.sh reddit-comment-post <udid> "Interesting perspective." --execute 0 --commit 0 --post-index 0 --out /tmp/reddit-comment-dry
./scripts/ios_tools.sh reddit-reply-comment <udid> "Good point." --execute 0 --commit 0 --post-index 0 --reply-index 0 --out /tmp/reddit-reply-dry
```

Single-session operation (reduces repeated session bootstrap between actions):

```bash
IOS_TOOLS_SKIP_BUILD=1 \
./scripts/ios_tools.sh reddit-engage-seq <udid> "Test dry-run comment" \
  --execute-like 0 --execute-comment 0 --commit 0 --out /tmp/reddit-engage-seq
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
./scripts/ios_tools.sh reddit-comment-post <udid> "Nice write-up." --execute 1 --commit 1
```

If the app is already inside a post detail view, you can broaden `REDDIT_POST_CELL_PREDICATE` to include title nodes:
`name CONTAINS 'reddit_feed__post__title_text' OR name CONTAINS 'reddit_feed__post__post_cell'`.

## Agentic Pattern

1. Run `reddit-daily-scroll` to produce `digest.json` + `thread.md`.
2. Score/select posts by policy.
3. Dry-run interactions (`--execute 0 --commit 0`).
4. Re-run with `--execute 1 --commit 1` only for approved actions.
5. Keep artifacts for audit (`result.json`, screenshot, XML source).

## Safety Notes

- `reddit.daily_scroll_digest` and `reddit.open_post` are read-only.
- `reddit.like_post` uses `requiresCommit` and only mutates when `execute_like=true`.
- `reddit.comment_post` uses `requiresCommit` and only mutates when `execute_comment=true`.
- `reddit.reply_to_comment` uses `requiresCommit` and only mutates when `execute_reply=true`.
- Keep the phone unlocked during session bootstrap and run execution.
