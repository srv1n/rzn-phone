# RZN Mobile Workflow Spec v1 (Draft)

This spec defines a **portable workflow format** for mobile automation that:

- aligns with the existing RZN browser automation approach (observe → act loops, deterministic workflows)
- works with **Appium** today (iOS real devices now, Android later)
- minimizes LLM tokens via **compact observations + encoded ids**

Capability naming contract:

- standalone operator docs should use `rzn-phone ...`
- umbrella/operator docs should use `rzn phone ...`
- runner internals may still use older implementation labels while repo internals finish converging on `rzn-phone`

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

### 1.3 Two-tier capability model

LLMs should not plan against a flat list of 40-50 low-level verbs. That wastes tokens and produces brittle plans.

This spec uses two tiers:

| Tier | Purpose | Typical size | Examples |
|---|---|---|---|
| Tier 1 | planner-facing capability families | 5-8 stable buckets | `observe`, `navigate`, `extract`, `interact`, `verify`, `session` |
| Tier 2 | runtime-facing execution primitives | dozens of concrete tools/actions | `ios.ui.observe_compact`, `ios.web.goto`, `ios.web.eval_js`, `ios.action.tap` |

Rules:

- Tier 1 is the thing the planner should think with.
- Tier 2 is the thing the runner should execute with.
- Site/app-specific behavior belongs in workflow packs and extraction adapters, not in Tier-1 family names.
- A runner MAY expose more Tier-1 families such as `workflow` or `utility`, but the core planning set should stay small.

### 1.4 Determinism + debuggability

Every step supports:

- `timeout_ms`
- `retries`
- `save_as` (store structured result for later steps)

On failure, the runner should return:

- the failing step index/id
- an error code (stable enum)
- last screenshot + UI source (best effort)

### 1.5 Safety gates

Workflow steps may mark `requires_commit=true` for actions that can cause irreversible side effects
(posting, purchasing, deleting, sending).

Runners MUST refuse to execute these steps unless `commit=true` is supplied at runtime (or a future host-issued approval token).

### 1.6 Completion and cleanup (runner options)

This spec defines the **workflow file format** (what gets stored in a workflow pack). In practice, runners also need
standardized **runtime options** for how to cleanup after a run.

Runners SHOULD support these post-run controls (names shown match the current iOS worker implementation):

- `disconnectOnFinish` (boolean, default `true`): end the automation session after the workflow run.
  - Alias: `closeOnFinish` (same meaning).
  - Set to `false` when you intentionally want to keep a session alive across multiple operations (for example, a
    single-session sequence like open → like → comment).
- `stopAppiumOnFinish` (boolean, default `false`): stop a spawned Appium server on completion (runner-specific).
- `backgroundAppOnFinish` (boolean, default `false`): press the OS Home button before teardown (best-effort).
  - This is the closest “close the app out” behavior that is portable: the app is sent to the background as a user would.
- `lockDeviceOnFinish` (boolean, default `false`): lock the device before teardown (best-effort).

Notes:

- “Closing the app” on iOS is not a first-class automation primitive; the portable approach is (1) background-to-Home, and
  (2) disconnect/teardown the session.
- These are **runtime invocation options**, not part of the workflow JSON file itself. Workflow authors should not hardcode
  teardown steps inside every workflow; prefer runner options so callers can choose per run.

---

## 2) Top-level workflow object

```jsonc
{
  "schema_version": "rzn.mobile.workflow.v1",
  "name": "reddit/comment_first_post",
  "version": "1.0.0",
  "description": "Open Reddit, open first post, draft and submit comment.",
  "required_variables": ["commentText"],
  "platforms": ["ios"],              // optional; default: ["ios","android"]
  "capability": {                    // optional Tier-1 classification for planners/catalogs
    "family": "interact",
    "intent": "comment_post",
    "surface": "native_app",
    "mutating": true
  },
  "inputs": {                        // optional; used for UI + validation
    "commentText": { "type": "string", "required": true }
  },
  "help": {
    "parameters": {
      "commentText": {
        "description": "Comment text to draft or submit.",
        "example": "This breakdown is actually useful."
      }
    },
    "examples": [
      {
        "label": "Draft a comment",
        "args": { "commentText": "This breakdown is actually useful." }
      }
    ]
  },
  "steps": [ /* ... */ ]
}
```

### Fields

- `schema_version` (required): fixed string identifier for this schema
- `name` (required): stable workflow id. Canonical form is `system/workflow`; legacy dotted ids are still accepted when loading older packs.
- `version` (required): semver
- `description` (optional but strongly recommended): human-readable summary shown by CLI help and catalogs
- `required_variables` (optional but recommended): explicit runnable inputs required at invocation time
- `platforms` (optional): `["ios"]`, `["android"]`, or both
- `inputs` (optional): schema-like declarations for runtime args
- `capability` (optional): Tier-1 classification metadata for catalogs, planners, and workflow-family filtering
- `help` (optional but recommended): workflow help metadata including parameter guidance and runnable examples
- `steps` (optional): executable steps
  - **Runnable workflows should include `steps`.** Metadata-only workflows may omit steps but are not executable by the runner.
  - Code-implemented workflows are deprecated; keep app-specific logic inside JSON workflow packs.
- `output` (optional): output template object rendered after steps (see §5.1)
- `presentation` (optional): runner-facing presentation hints derived from workflow data (see §5.2)

### 2.1 Help metadata contract

CLI help should never dead-end. If the user is one step away from success, the workflow metadata should let the runner point at the next successful command instead of just printing “missing input”.

Recommended minimum authoring contract:

- `name`
- `description`
- `required_variables`
- `help.parameters`
- `help.examples`

`help.parameters` is a map keyed by input name. It is documentation metadata, not execution logic.

```jsonc
{
  "help": {
    "parameters": {
      "query": {
        "description": "Search text to submit.",
        "example": "best headphones 2026",
        "group": "core",
        "structure": "Plain search string."
      }
    },
    "examples": [
      {
        "label": "Basic search",
        "args": {
          "query": "best headphones 2026"
        }
      }
    ]
  }
}
```

Rules:

- `required_variables` should stay in sync with runnable required inputs.
- `help.parameters` should provide descriptions/examples when the bare input schema is not enough.
- Runners may infer help from `inputs` and placeholders as a fallback, but inference is the backup path, not the standard authoring path.

### 2.2 Capability metadata

Workflows MAY declare a top-level `capability` object so planners can reason at Tier 1 without reading every step.

```jsonc
{
  "capability": {
    "family": "extract",
    "intent": "search_results",
    "surface": "web",
    "mutating": false
  }
}
```

Fields:

- `family` (required): Tier-1 capability family such as `observe`, `navigate`, `extract`, `interact`, `verify`, or `session`
- `intent` (optional): narrower planner-facing intent within the family such as `search_results` or `open_place`
  - Prefer shared `snake_case` intents across apps when semantics match: use `open_post`, `send_dm`, `like_post`, not brand-specific variants.
- `surface` (optional): broad execution surface such as `web`, `native_app`, or `messages`
- `mutating` (optional): whether the workflow is expected to mutate remote/app state

This field is metadata, not execution logic. The runner must not infer behavior from it beyond filtering, ranking, and display.

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

#### Runner support for `when` (implemented in this repo)

The runner supports a minimal `when` shape:

- `boolean` — run when true
- `string` — treat as a variable path and run if truthy
- object examples:
  - `{ "var": "submit_mode", "equals": "suggestion" }`
  - `{ "var": "flag", "truthy": true }`
  - `{ "var": "value", "notEquals": "x" }`
  - `{ "var": "value", "exists": true }`

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

### 5.2 Workflow presentation hints (runner)

Runners MAY support an optional top-level `presentation` object for human-friendly rendering.
This is for presentation only; it must not replace the structured `output`.

Conventions:

- `presentation` is rendered with the same `{{var}}` substitution rules as `output`.
- Runners that do not understand `presentation` should ignore it.
- CLI renderers should rely on this metadata instead of hardcoding workflow ids.

Recommended initial CLI contract:

```jsonc
{
  "presentation": {
    "cli": {
      "type": "result_list",
      "title": "Google results for \"{{query}}\"",
      "items": "{{steps.results.result}}",
      "titleField": "title",
      "urlField": "url",
      "snippetField": "snippet",
      "footer": "Found {{steps.count.count}} result(s)."
    }
  }
}
```

Notes:

- `items` should resolve to an array of objects.
- `titleField`, `urlField`, and `snippetField` name the keys to display from each item.
- Additional renderer types can be added later without changing the workflow execution model.

---

## 6) Runner invocation example (non-normative)

Example call shape for a worker tool like `ios.workflow.run`:

```jsonc
{
  "name": "ios.workflow.run",
  "arguments": {
    "name": "linkedin.daily_scroll_digest",
    "session": { "udid": "..." },
    "args": { "max_posts": 20 },

    "commit": false,

    "disconnectOnFinish": true,
    "backgroundAppOnFinish": true,
    "lockDeviceOnFinish": false
  }
}
```

---

## 6) Safety and approvals

### 6.1 Commit gate (v1)

- Steps with `requires_commit=true` MUST NOT execute unless `commit=true` is provided at run time.
- Runners SHOULD surface “blocked by safety gate” as a distinct error code.
- Current packaged mutating flows include Reddit, LinkedIn, Instagram, X, and App Store `appstore/post_review`; each needs both its workflow execute arg and `commit=true`.

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
