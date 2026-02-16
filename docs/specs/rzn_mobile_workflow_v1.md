# RZN Mobile Workflow Spec v1 (Draft)

This spec defines a **portable workflow format** for mobile automation that:

- aligns with the existing RZN browser automation approach (observe → act loops, deterministic workflows)
- works with **Appium** today (iOS real devices now, Android later)
- minimizes LLM tokens via **compact observations + encoded ids**

This is a **workflow format**, not a runner. A runner (host-side or worker-side) is expected to:

- validate workflows (JSON Schema)
- execute steps with retries/timeouts
- return structured trace + artifacts
- enforce safety gates (`commit`)

---

## 1) Design principles

### 1.1 Cross-platform first

We define a small set of primitives that map well to both:

- iOS (XCUITest driver: accessibility id, iOS predicate, iOS class chain, XPath)
- Android (UIAutomator2 driver: accessibility id/content-desc, android uiautomator, resource-id, XPath)

We keep platform-specific locator strategies in **`target_spec`**, not in step names.

### 1.2 Two-tier autonomy (low-token)

The standard encourages **encoded ids** produced by compact observation:

- `observe_compact` returns 2–8KB of actionable nodes.
- Each node has an encoded id like `btn_1`, `fld_2`, `cell_3`.
- Action steps use `target_spec.encoded_id` instead of XPath.

This mirrors the browser system’s “compact snapshot + stable-ish ids” pattern.

### 1.3 Determinism + debuggability

Every step supports:

- `timeout_ms`
- `retries`
- `save_as` (store structured result for later steps)

On failure, the runner should return:

- the failing step index/id
- an error code (stable enum)
- last screenshot + UI source (best effort)

### 1.4 Safety gates

Workflow steps may mark `requires_commit=true` for actions that can cause irreversible side effects
(posting, purchasing, deleting, sending).

Runners MUST refuse to execute these steps unless `commit=true` is supplied at runtime (or a future host-issued approval token).

---

## 2) Top-level workflow object

```jsonc
{
  "schema_version": "rzn.mobile.workflow.v1",
  "name": "reddit.comment_first_post",
  "version": "1.0.0",
  "description": "Open Reddit, open first post, draft and submit comment.",
  "platforms": ["ios"],              // optional; default: ["ios","android"]
  "inputs": {                        // optional; used for UI + validation
    "commentText": { "type": "string", "required": true }
  },
  "steps": [ /* ... */ ]
}
```

### Fields

- `schema_version` (required): fixed string identifier for this schema
- `name` (required): stable workflow id (dot-separated)
- `version` (required): semver
- `platforms` (optional): `["ios"]`, `["android"]`, or both
- `inputs` (optional): schema-like declarations for runtime args
- `steps` (optional): executable steps
  - **Runnable workflows should include `steps`.** Metadata-only workflows may omit steps but are not executable by the runner.
  - Code-implemented workflows are deprecated; keep app-specific logic inside JSON workflow packs.
- `output` (optional): output template object rendered after steps (see §5.1)

---

## 3) Step model

v1 supports **two step shapes**:

1) **Action steps** (`type: ...`) — preferred long-term portable shape  
2) **Tool-call steps** (`tool: ...`) — escape hatch; maps 1:1 to an MCP tool call

Runners MAY support only tool-call steps initially and incrementally add action steps.

### 3.1 Common step fields

All step kinds share:

- `id` (optional): stable id for trace/debug
- `when` (optional): conditional execution
- `timeout_ms` (optional): overrides default timeouts
- `retries` (optional): number of retries on transient failure
- `requires_commit` (optional): safety gate
- `save_as` / `saveAs` (optional): store result into context under a variable name

### 3.2 Tool-call steps (supported today in this repo)

```jsonc
{
  "tool": "ios.action.tap",
  "arguments": { "target": { "using": "accessibility id", "value": "..." } },
  "timeout_ms": 20000,
  "retries": 1,
  "requires_commit": false,
  "save_as": "tap_result"
}
```

This shape is designed to be executed by a worker-level runner without schema drift:
it directly uses the tool contract.

### 3.3 Action steps (portable, browser-aligned)

```jsonc
{
  "type": "tap",
  "target_spec": { "encoded_id": "btn_1" },
  "timeout_ms": 10000,
  "retries": 1
}
```

Recommended `type` values (initial set):

- `ensure_appium`
- `session.create` / `session.delete`
- `observe_compact`
- `tap`
- `type_text`
- `wait`
- `scroll`
- `back`
- `screenshot`
- `ui_source`

> Alignment note: `tap` ≈ browser `click_element`, `type_text` ≈ `fill_input_field`, `observe_compact` ≈ browser “snapshot”.

---

## 4) Target spec (cross-platform)

`target_spec` should be a union of “best effort” strategies, tried in order.

Recommended fields:

```jsonc
{
  "encoded_id": "btn_1",            // from observe_compact
  "snapshot_id": "snap_...",        // optional; validate against current snapshot

  "using": "accessibility id",      // Appium locator strategy
  "value": "reddit__comment_composer__reply_button",

  "point": {"x": 120, "y": 320}     // last resort
}
```

Additional optional fields (future):

- `text` / `text_contains`
- `role` (button/field/cell)
- `bounds_hint` (x/y/w/h for disambiguation)
- platform-specific:
  - iOS: `ios_predicate`, `ios_class_chain`
  - Android: `android_uiautomator`

---

## 5) Variable substitution

Workflows should support `{{var}}` substitution:

- If a string is exactly `{{var}}`, substitute the **typed value** (number/bool/object)
- Otherwise do string interpolation

This allows clean parameterization:

```jsonc
{ "udid": "{{udid}}", "timeout_ms": "{{timeouts.session_create_ms}}" }
```

### 5.1 Workflow outputs (runner)

Runners MAY support an optional top-level `output` template. The template is rendered
after all steps execute (or immediately before returning success), using the same
`{{var}}` substitution rules.

Conventions:

- Any step with `save_as` / `saveAs` is available under `steps.<save_as>` in the template.
- The runner should still include trace metadata even when `output` is provided.
- If `output` is omitted, the runner returns a default `{ok, steps, trace}` envelope.

---

## 6) Safety and approvals

### 6.1 Commit gate (v1)

- Steps with `requires_commit=true` MUST NOT execute unless `commit=true` is provided at run time.
- Runners SHOULD surface “blocked by safety gate” as a distinct error code.

### 6.2 Future: host-issued approval token

Replace boolean `commit` with an approval token minted by the host after presenting a plan to the user.
The workflow format does not need to change; only the runner enforcement does.

---

## 7) Packaging: workflow packs (recommended)

Ship workflows as data-only packs, separate from the worker binary:

```
pack.json
workflows/*.json
```

`pack.json` SHOULD include:

- `pack_id`, `version`, `min_worker_version`
- list of workflows
- optional signature metadata

See `docs/DEEP_DIVE.md` for more detail.

---

## 8) Example: Reddit comment workflow (commit-gated)

```jsonc
{
  "schema_version": "rzn.mobile.workflow.v1",
  "name": "reddit.comment_first_post",
  "version": "1.0.0",
  "platforms": ["ios"],
  "inputs": {
    "commentText": { "type": "string", "required": true }
  },
  "steps": [
    { "tool": "ios.appium.ensure", "arguments": {} },
    {
      "tool": "ios.session.create",
      "arguments": { "udid": "{{udid}}", "kind": "native_app", "bundleId": "com.reddit.Reddit" }
    },
    {
      "tool": "ios.action.tap",
      "arguments": { "target": { "using": "accessibility id", "value": "reddit_feed__post__post_cell" } }
    },
    {
      "tool": "ios.action.type",
      "arguments": {
        "target": { "using": "accessibility id", "value": "reddit__comment_composer__comment_text_view" },
        "text": "{{commentText}}"
      }
    },
    {
      "tool": "ios.action.tap",
      "requires_commit": true,
      "arguments": { "target": { "using": "accessibility id", "value": "reddit__comment_composer__reply_button" } }
    }
  ]
}
```
