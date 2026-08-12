use serde_json::{json, Value};

use super::registry::tool;

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "ios.workflow.list",
            "List prebuilt iOS workflows grouped by system. Returns canonical ids as system/workflow plus legacy dot-name aliases.",
            json!({
                "type": "object",
                "properties": {
                    "system": { "type": "string", "description": "Optional system namespace filter such as safari or google_maps." },
                    "family": { "type": "string", "description": "Optional Tier-1 capability family filter such as extract or navigate." }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.capability.list",
            "Return the two-tier capability taxonomy: Tier-1 families for planning and Tier-2 primitive groupings for execution.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.workflow.run",
            "Run a named workflow. Accepts system/workflow, system + workflow, or the legacy dotted name.",
            json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Workflow ref. Canonical form is system/workflow." },
                        "workflow": { "type": "string", "description": "Workflow ref, or workflow name when paired with system." },
                        "system": { "type": "string", "description": "Workflow system namespace when using separate system + workflow fields." },
                        "session": { "type": "object" },
                        "args": { "type": "object" },
                        "commit": { "type": "boolean", "default": false },
                        "privacyGate": { "type": "string", "description": "Privacy class grant for workflows that read private phone data, such as messages, otp, calls, or notifications." },
                        "privacyGates": { "type": "array", "items": { "type": "string" }, "description": "Privacy class grants for workflows that read private phone data." },
                        "disconnectOnFinish": { "type": "boolean", "default": true, "description": "Alias of closeOnFinish." },
                        "closeOnFinish": { "type": "boolean", "default": true },
                        "stopAppiumOnFinish": { "type": "boolean", "default": false },
                        "backgroundAppOnFinish": { "type": "boolean", "default": false },
                        "lockDeviceOnFinish": { "type": "boolean", "default": false }
                    },
                    "additionalProperties": false
            }),
        ),
        tool(
            "rzn.workflow_failure_report.review",
            "Build a sanitized phone automation failure draft for host review/submission. Sends nothing.",
            json!({
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "object",
                        "description": "Sanitized failure draft or legacy safe summary returned by ios.workflow.run.",
                        "properties": {
                            "surface": { "type": "string" },
                            "flow": { "type": "string" },
                            "flow_version": { "type": "string" },
                            "failed_stage": { "type": "string" },
                            "error": { "type": "string" },
                            "app_version": { "type": "string" },
                            "platform": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "draft": { "type": "object", "description": "FlowFailureReportDraft previously emitted by the worker." },
                    "note": { "type": "string", "description": "Optional user-authored note, max 2000 characters." }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "rzn.workflow_failure_report.submit",
            "Deprecated compatibility shim. Builds a sanitized draft and manual host event; it does not submit to the backend.",
            json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "object", "description": "Sanitized failure draft or legacy safe summary returned by ios.workflow.run." },
                    "payload": { "type": "object", "description": "FlowFailureReportDraft previously shown to the user." },
                    "draft": { "type": "object", "description": "FlowFailureReportDraft previously shown to the user." },
                    "note": { "type": "string", "description": "Optional user-authored note, max 2000 characters." },
                    "dryRun": { "type": "boolean", "default": true }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "rzn.workflow_failure_report.queue",
            "List or clear explicitly queued broken workflow reports.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "clear"], "default": "list" }
                },
                "additionalProperties": false
            }),
        ),
    ]
}
