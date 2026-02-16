# App Store Workflow Notes (Real Device)

This repo now includes read-only App Store workflows for iOS real devices:

- `appstore.typeahead`
- `appstore.search_results`

The implementation is intentionally best-effort and tolerant of minor App Store UI variance.

## Locator Strategy

Primary selectors observed on real devices:

- Search tab: `accessibility id = AppStore.tabBar.search`
- Search field: `accessibility id = AppStore.searchField`
- Typeahead container: `name = AppStore.searchHints` (`XCUIElementTypeCollectionView`)
- Typeahead rows: `XCUIElementTypeCell` text from `label/name` inside `AppStore.searchHints`
- Results container: `name = AppStore.searchResults` (`XCUIElementTypeCollectionView`)
- Result rows: `name BEGINSWITH AppStore.shelfItem.searchResult[`

Fallback policy:

1. `encodedId` targeting from `ios.ui.observe_compact` when available.
2. Accessibility id locators.
3. `-ios predicate string` fallbacks for structural variants.

No XPath is required for the current App Store flows.

## Output Contract

`appstore.typeahead` returns:

- `query`
- `prefixes`: ordered `[{prefix, suggestionCount, suggestions:[{text, position}]}]`
- `suggestions`: ordered `{text, position}` rows
- `screenshot` (base64 PNG)
- `uiSource.source` (full XML)

`appstore.search_results` returns:

- `query`
- `results`: ordered `{position, name, subtitle, developer?}`
- `observed_rank` when `target_app_name` is provided
- `compactSnapshot` of top fold
- `screenshot` (base64 PNG)
- `uiSource.source` (full XML)

## Safety + Prereqs

- Device must be unlocked and trusted.
- App Store should already be logged in.
- Workflows are read-only: no purchase/download/submit actions.
- Popups are dismissed best-effort when possible.
