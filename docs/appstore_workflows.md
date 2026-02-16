# App Store Workflow Notes (Real Device)

This repo now includes read-only, data-only App Store workflows for iOS real devices:

- `appstore.typeahead`
- `appstore.search_results`
- `appstore.app_details`
- `appstore.reviews`
- `appstore.version_history`
- `appstore.screenshots`

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

## Safety + Prereqs

- Device must be unlocked and trusted.
- App Store should already be logged in.
- Workflows are read-only: no purchase/download/submit actions.
- Popups/story cards are dismissed best-effort when possible.
