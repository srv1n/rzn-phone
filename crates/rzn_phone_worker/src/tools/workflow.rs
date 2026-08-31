use serde_json::{json, Value};

use super::registry::tool;

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "ios.workflow.list",
            "List prebuilt iOS workflows grouped by system. Returns workflow ids as system/workflow.",
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
            "Run a named workflow using a system/workflow id or separate system and workflow fields.",
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
                        "disconnectOnFinish": { "type": "boolean", "default": true },
                        "stopAppiumOnFinish": { "type": "boolean", "default": false },
                        "backgroundAppOnFinish": { "type": "boolean", "default": false },
                        "lockDeviceOnFinish": { "type": "boolean", "default": false }
                    },
                    "additionalProperties": false
            }),
        ),
        tool(
            "rzn.workflow_failure_report.review",
            "Build a sanitized phone automation failure draft for host review. Sends nothing.",
            json!({
                "type": "object",
                "properties": {
                    "draft": { "type": "object", "description": "FlowFailureReportDraft previously emitted by the worker." },
                    "note": { "type": "string", "description": "Optional user-authored note, max 2000 characters." }
                },
                "required": ["draft"],
                "additionalProperties": false
            }),
        ),
    ]
}
