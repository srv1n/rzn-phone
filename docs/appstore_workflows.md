# App Store Workflow Notes (Real Device)

This repo now includes App Store workflows for iOS real devices:

Read-only:
- `appstore.typeahead`
- `appstore.search_results`
- `appstore.app_details`
- `appstore.reviews`
- `appstore.version_history`
- `appstore.screenshots`

Commit-gated write flow:

- `appstore.post_review`

The implementation is intentionally best-effort and tolerant of minor App Store UI variance. All App Store-specific selectors live in the workflow JSON, not the core runner.

## Locator Strategy

Primary selectors observed on real devices:

- Search tab: `accessibility id = AppStore.tabBar.search`
- Search field: `accessibility id = AppStore.searchField`
- Typeahead container: `name = AppStore.searchHints` (`XCUIElementTypeCollectionView`)
- Typeahead rows: `XCUIElementTypeCell` text from `label/name` inside `AppStore.searchHints`
- Results container: `name = AppStore.searchResults` (`XCUIElementTypeCollectionView`)
- Result rows: `name BEGINSWITH AppStore.shelfItem.searchResult[`
- Product page top lockup: `name CONTAINS AppStore.shelfItem.productTopLockup`
- Ratings badge: `accessibility id = AppStore.productPage.badge.rating`
- Ratings header (reviews view): `name CONTAINS parentId=productRatings`
- Review rows: `name BEGINSWITH AppStore.shelfItem.productReview`
- What's New header: `name CONTAINS parentId=mostRecentVersion`
- Version rows: `name BEGINSWITH AppStore.shelfItem.titledParagraph`
- Screenshot cells: `name BEGINSWITH AppStore.shelfItem.productMediaItem`

Submission behavior:
- `appstore.search_results` supports `submit_mode` (`suggestion` default, `keyboard` to press Enter/return).
- For workflows that allow `submit_mode=keyboard`, if search hints remain visible after typing, the workflow taps the first hint as a fallback to avoid failed Enter/return submits.

Reviews workflow behavior:
- `appstore.reviews` opens the Ratings & Reviews view by tapping the rating badge.
- Sorting is best-effort: non-default modes attempt to select from the UI if available.

Review-posting workflow behavior:
- `appstore.post_review` first launches the installed target app by bundle id, captures a proof screenshot, then switches to App Store and searches by `app_title`.
- App Store deep-link navigation via `app_url` is not yet a first-class workflow primitive, so the wrapper carries `app_url`/`app_id` for bookkeeping while the on-device path uses native search.
- Review submission is dual-gated: workflow arg `execute_submit=true` plus runner `commit=true`.

Fallback policy:

1. `encodedId` targeting from `ios.ui.observe_compact` when available.
2. Accessibility id locators.
3. `-ios predicate string` fallbacks for structural variants.

Notes:
- XPath is currently used to target the search hint cell and result rows in `appstore.search_results`.

## Output Contract

`appstore.typeahead` returns (from workflow `output` templating):

- `query`
- `prefixes`: ordered `[{prefix, suggestionCount, suggestions:[{text, position}]}]`
- `suggestions`: ordered `{text, position}` rows
- `screenshot` (base64 PNG)
- `uiSource.source` (full XML)

`appstore.search_results` returns (from workflow `output` templating):

- `query`
- `results`: ordered `{position, name, subtitle, developer?}`
- `observed_rank` when `target_app_name` is provided
- `compactSnapshot` of top fold (from `ios.ui.observe_compact`)
- `screenshot` (base64 PNG)
- `uiSource.source` (full XML)

`appstore.app_details` returns:

- `title`, `tagline`, `subtitle`, `developer`, `offer`, `offerSubtitle`
- `badges`: raw badge labels (ratings, age, category, etc.)
- `category`: best-effort category from the Information section (scrolls to fetch)
- `screenshotItems`: raw media cell identifiers + `screenshotCount`
- `screenshot` + `uiSource.source`

`appstore.reviews` returns:

- `ratingSummary`: raw summary label(s)
- `reviewSummary`: best-effort LLM/summary text if present
- `reviews`: extracted review rows (title/body/rating/author/response fields)
- `reviewCount30d`, `reviewCount60d`: counts based on parsed review dates from the author line
- `reviewDateBuckets`, `reviewDatesParsed`, `reviewDatesSkipped` for diagnostics
- `reviewCount` + `screenshot` + `uiSource.source`

`appstore.version_history` returns:

- `versions`: extracted update rows (best-effort split)
- `versionCount` + `screenshot` + `uiSource.source`

`appstore.screenshots` returns:

- `shots`: list of full-screen captures after swiping the media carousel

`appstore.post_review` returns:

- `appLaunchedScreenshot` + `appLaunchedUiSource`
- `draftReviewScreenshot` + `draftReviewUiSource`
- `reviewPostedScreenshot` + `reviewPostedUiSource` when `execute_submit=true`
- `matchedResultRank` / `matchedResultIndex` for search-target diagnostics

## Job Wrapper

Use the host-side wrapper when you need the full phone-team spec flow:

```bash
python3 scripts/appstore_review_job.py <udid> /path/to/job.json
```

Dry-run without submitting the review:

```bash
python3 scripts/appstore_review_job.py <udid> /path/to/job.json --dry-run --skip-upload
```

Required env for R2 upload:

- `OUTREACH_PROOF_R2_ACCESS_KEY_ID`
- `OUTREACH_PROOF_R2_SECRET_ACCESS_KEY`
- `OUTREACH_PROOF_R2_ENDPOINT`
- `OUTREACH_PROOF_R2_PUBLIC_BASE_URL`
- Optional: `OUTREACH_PROOF_R2_BUCKET` (defaults to `outreach-proof`)
- Optional: `OUTREACH_PROOF_R2_REGION` (defaults to `auto`)

Wrapper notes:

- If the job omits `installed_app_bundle_id`, the wrapper resolves bundle id from Apple’s public lookup API using `app_id`.
- Callback delivery is skipped automatically in `--dry-run` mode.
- Daily review caps / spacing / 24h retry policy are not enforced in the worker; those controls still belong to the external scheduler/orchestrator.

## Safety + Prereqs

- Device must be unlocked and trusted.
- App Store should already be logged in.
- Browse workflows are read-only.
- `appstore.post_review` is not read-only; run dry first and only submit with explicit approval (`execute_submit=true`, `commit=true`).
- Popups/story cards are dismissed best-effort when possible.
