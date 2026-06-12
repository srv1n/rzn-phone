use serde_json::{json, Value};

use super::registry::tool;

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &["ios.script.run"];

pub(crate) fn definitions() -> Vec<Value> {
    vec![tool(
        "ios.script.run",
        "Execute a deterministic step list (each step calls an existing tool).",
        json!({
            "type": "object",
            "properties": {
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string" },
                        "arguments": { "type": "object" },
                        "when": {},
                        "timeoutMs": { "type": "integer" },
                        "retries": { "type": "integer", "default": 0 },
                        "requiresCommit": { "type": "boolean", "default": false },
                        "saveAs": { "type": "string" },
                        "save_as": { "type": "string" }
                    },
                    "required": ["tool"],
                    "additionalProperties": false
                }
            },
                "vars": { "type": "object", "default": {} },
                "commit": { "type": "boolean", "default": false },
                "privacyGate": { "type": "string", "description": "Privacy class grant for private phone-data steps." },
                "privacyGates": { "type": "array", "items": { "type": "string" }, "description": "Privacy class grants for private phone-data steps." },
                "disconnectOnFinish": { "type": "boolean", "default": true, "description": "Alias of closeOnFinish." },
                "closeOnFinish": { "type": "boolean", "default": true },
                "stopAppiumOnFinish": { "type": "boolean", "default": false },
                "backgroundAppOnFinish": { "type": "boolean", "default": false },
                "lockDeviceOnFinish": { "type": "boolean", "default": false }
            },
            "required": ["steps"],
            "additionalProperties": false
        }),
    )]
}
