# rzn-phone deep dive (runtime, workflows, publishing)

This doc is a practical guide for scaling iOS automation in RZN with:

- **low-token autonomy** (compact UI snapshots + encoded ids)
- **reliable primitives** (tap/type/wait/scroll with retries + good errors)
- **versioned prebuilt workflows** (deterministic runner)
- **workflow packs** (data-only distribution separate from the worker binary)

> Scope note: this repo is the **worker/plugin boundary**. We intentionally keep iOS/Xcode/Appium/WDA complexity out of `rznapp`.

---

## 1) Mental model

### 1.1 Three layers

1. **Transport / Tool surface (MCP over stdio)**  
   The host treats this as “just another tool provider”. JSON-RPC `id` is opaque. Notifications are accepted and ignored.

2. **Automation runtime (Appium + XCUITest driver)**  
   Owns WDA lifecycle, device tunnel (iOS 17+), element queries, gestures, screenshots, and web-context commands.

3. **RZN primitives & workflows (this worker)**  
   Converts “LLM intent” into deterministic WebDriver calls, and returns compact observations suitable for fast planning.

### 1.2 Why “compact snapshot + encoded ids” matters

iOS page source can be **huge** (often 50–200KB+ XML). Sending that into an LLM loop is expensive and noisy.

Instead, treat observation as a summarization step:

- `ios.ui.observe_compact` returns **2–8KB** of actionable UI state.
- Every actionable item has an **encoded id** like `btn_3`, `fld_1`, `cell_2`.
- Actions reference encoded ids, not XPath.

This mirrors the browser automation approach: “snapshot → decide → act → verify”.

---

## 2) Primitives (token-efficient autonomy)

### 2.1 Observe tier

**`ios.ui.observe_compact`**

Returns a compact list of nodes:

- `id`: encoded id (`btn_1`, `fld_2`, `txt_7`, …)
- `role`: normalized (`button`, `field`, `cell`, `text`, …)
- `name` / `label` / `value`: short strings (truncated)
- `enabled`, `visible`
- `bounds`: `{x,y,w,h}`
- `hints`: e.g. `["tap"]`, `["type"]`

The worker also stores a **resolver map** (encoded id → locator hints) in memory for the next action calls.

### 2.2 Resolve tier

**`ios.target.resolve`**

Given an encoded id, returns a best-effort `target_spec` suitable for WebDriver:

- prefers `accessibility id` (iOS “name”)
- falls back to `ios predicate string` (e.g. by label)
- avoids XPath unless absolutely necessary

This is useful for debugging, “explainability”, and workflow authoring.

### 2.3 Act tier

**`ios.action.*`** tools accept:

- `target.encodedId` (preferred for low-token loops), or
- a raw `target_spec` (`using` + `value`), or
- `point` for “tap at x/y”.

Target selection also supports:

- `target.index` (choose the Nth match, default `0`)
- `target.requireUnique` (fail fast if multiple matches)

Recommended MVP action set:

- `ios.action.tap`
- `ios.action.type`
- `ios.action.wait` (exists / visible)
- `ios.action.scroll` (direction + amount)
- `ios.action.scroll_until` (composite: find → scroll → retry)

For extraction and verification:

- `ios.element.text`, `ios.element.attribute`, `ios.element.rect`
- `ios.alert.*` helpers for system permission dialogs

Each action should:

- re-find the element at action time (avoid stale element ids)
- include retries for common transient failures
- return structured errors with `errorCode` enums for programmatic handling (`NO_SESSION`, `ELEMENT_NOT_FOUND`, `AMBIGUOUS_MATCH`, `TIMEOUT`, …)

---

## 3) Deterministic workflow runner

### 3.1 Why a runner (even with an LLM)

LLM autonomy is great for long-tail tasks, but for day-to-day reliability you want:

- step-by-step determinism
- consistent retries + timeouts
- artifacts on failure (screenshot + source)

So we treat workflows as **data** (JSON) and execute them with a strict runner:

- `ios.script.run`: execute an inline step array
- `ios.workflow.run`: execute a named workflow loaded from disk/packs
- App-specific logic lives in workflow JSONs; compiled app handlers are deprecated.

### 3.2 Workflow schema v1 (practical)

Workflows should be:

- declarative, small steps
- parameterized (`{{query}}`, etc.)
- explicitly guarded for destructive steps (`commit` gating)
- able to compose an `output` object from saved step results (`save_as` / `saveAs`)

Keep the schema close to the browser-native actions taxonomy to reduce team cognitive load.

Example step types (subset):

- `ensure_appium`
- `session.create` (kind: `safari_web` or `native_app`)
- `ui.observe_compact`
- `action.tap`, `action.type`, `action.wait`, `action.scroll`
- `web.goto`, `web.eval_js` (explicitly unsafe)
- `assert.contains_text` (optional)
- `artifact.screenshot` (optional)

---

## 4) Workflow packs (publishing + versioning)

### 4.1 Goal

Ship new workflows (and updates) **without rebuilding the worker binary**.

Treat workflows as “content packs”:

- versioned
- optionally signed
- discoverable by the worker at runtime

### 4.2 Suggested layout

```
pack.json
workflows/
  safari.google_search.json
  reddit.read_first_post.json
  reddit.post_comment.json
```

`pack.json` should include:

- `packId` (stable)
- `version` (semver)
- `minWorkerVersion`
- list of workflows included (name → file)
- optional `signature` metadata (publisher, key id, hash)

### 4.3 Installation / discovery

The worker should load packs from:

1. built-ins under `crates/rzn_phone_worker/resources/workflows/` (always available)
2. user-provided directories (env-configured), e.g.:
   - `RZN_IOS_WORKFLOW_DIRS=/path/one:/path/two`

### 4.4 Signing

You already have a signed plugin distribution story for worker binaries.

For workflow packs you can pick one of two strategies:

- **MVP**: unsigned packs (local dev) + host-controlled download path
- **Productized**: ed25519 signatures per pack (verify before loading)

The key point: keep pack verification inside the worker (so “trust policy” is centralized).

---

## 5) Authoring at scale

### 5.1 Element discovery

Use a combination of:

- `ios.ui.observe_compact` for day-to-day autonomy
- `ios.ui.source` (raw XML) when you need to discover stable accessibility ids
- Appium Inspector when you want a visual UI tree and generated selectors

### 5.2 Recommended workflow build loop

1. Implement a workflow in JSON (or inline via `ios.script.run`)
2. Run it via `scripts/rzn_phone.sh workflow-smoke ...` or host dev-mount
3. Tighten selectors (prefer accessibility id)
4. Add commit gates for destructive steps
5. Publish as workflow pack (or bundle into plugin for now)

### 5.3 Migrating compiled workflows (legacy → data-only)

If a workflow is still implemented in Rust, migrate it into JSON so app-specific logic lives in workflow packs:

1. **Inventory selectors**: extract app-specific locators (accessibility ids, predicates, CSS) from the code path.
2. **Map to primitives**: replace each action with the closest `ios.action.*` / `ios.web.*` tool call.
3. **Add `saveAs` + `output`**: capture intermediate results and compose the final output in the workflow JSON.
4. **Delete the handler**: remove the compiled workflow function and keep only generic primitives in the worker.
5. **Validate**: run `scripts/rzn_phone.sh workflow-smoke` or a device-specific smoke, then publish the JSON pack.

---

## 6) Safety model (today vs later)

**Today (MVP):**

- `commit: boolean` gates irreversible workflow steps
- `ios.web.eval_js` is labeled unsafe/high-risk in tool docs + structured output

**Later (recommended):**

- move to a host-issued approval token (plan/approve/commit) so enforcement is centralized
- keep worker-side convention anyway (defense in depth)
