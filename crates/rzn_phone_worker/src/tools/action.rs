use serde_json::{json, Value};

use super::registry::tool;

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "ios.app.activate",
            "Activate a native iOS app by bundle id, optionally terminating it first for a clean relaunch.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "bundleId": { "type": "string" },
                    "terminateFirst": { "type": "boolean", "default": false }
                },
                "required": ["bundleId"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.tap",
            "Tap a UI element by encoded id (preferred), locator (using/value), or point.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                        "target": {
                            "type": "object",
                            "properties": {
                                "encodedId": { "type": "string" },
                                "snapshotId": { "type": "string" },
                                "using": { "type": "string" },
                                "value": { "type": "string" },
                                "index": { "type": "integer", "minimum": 0, "default": 0 },
                                "requireUnique": { "type": "boolean", "default": false }
                            },
                            "additionalProperties": false
                        },
                    "point": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number" },
                            "y": { "type": "number" }
                        },
                        "required": ["x", "y"],
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.type",
            "Type text into a UI field by encoded id (preferred) or locator (using/value).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                        "target": {
                            "type": "object",
                            "properties": {
                                "encodedId": { "type": "string" },
                                "snapshotId": { "type": "string" },
                                "using": { "type": "string" },
                                "value": { "type": "string" },
                                "index": { "type": "integer", "minimum": 0, "default": 0 },
                                "requireUnique": { "type": "boolean", "default": false }
                            },
                            "additionalProperties": false
                        },
                    "text": { "type": "string" },
                    "clearFirst": { "type": "boolean", "default": true },
                    "pressEnter": { "type": "boolean", "default": false }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.typeahead",
            "Type a query or prefixes into a field and capture ordered typeahead suggestions (generic).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "field": {
                        "type": "object",
                        "properties": {
                            "encodedId": { "type": "string" },
                            "snapshotId": { "type": "string" },
                            "using": { "type": "string" },
                            "value": { "type": "string" },
                            "index": { "type": "integer", "minimum": 0, "default": 0 },
                            "requireUnique": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    },
                    "query": { "type": "string" },
                    "prefixes": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "default": 10, "minimum": 1, "maximum": 20 },
                    "typingMode": { "type": "string", "default": "full" },
                    "suggestionQuery": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeCell" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "nameContains": { "type": "string" },
                            "label": { "type": "string" },
                            "labelContains": { "type": "string" },
                            "ancestorName": { "type": "string" },
                            "ancestorType": { "type": "string" },
                            "max": { "type": "integer", "minimum": 1, "maximum": 50 }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["field"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.wait",
            "Wait for an element to exist by encoded id (preferred) or locator (using/value).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                        "target": {
                            "type": "object",
                            "properties": {
                                "encodedId": { "type": "string" },
                                "snapshotId": { "type": "string" },
                                "using": { "type": "string" },
                                "value": { "type": "string" },
                                "index": { "type": "integer", "minimum": 0, "default": 0 },
                                "requireUnique": { "type": "boolean", "default": false }
                            },
                            "additionalProperties": false
                        },
                    "timeoutMs": { "type": "integer", "default": 10000 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.scroll",
            "Scroll the screen in a direction (uses touch pointer actions).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
                    "distance": { "type": "number", "default": 0.6 }
                },
                "required": ["direction"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.back",
            "Navigate back (best-effort on native apps).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.scroll_until",
            "Scroll until a target element exists (composite: find -> scroll -> retry).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "target": {
                        "type": "object",
                        "properties": {
                            "encodedId": { "type": "string" },
                            "snapshotId": { "type": "string" },
                            "using": { "type": "string" },
                            "value": { "type": "string" },
                            "index": { "type": "integer", "minimum": 0, "default": 0 },
                            "requireUnique": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    },
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "default": "down" },
                    "distance": { "type": "number", "default": 0.6 },
                    "maxScrolls": { "type": "integer", "default": 12 },
                    "timeoutMs": { "type": "integer", "default": 60000 },
                    "settleMs": { "type": "integer", "default": 350 }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.alert.text",
            "Read the currently displayed system alert text, if any (read-only).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.alert.accept",
            "Accept the currently displayed system alert, if any.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.alert.dismiss",
            "Dismiss the currently displayed system alert, if any.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.alert.wait",
            "Wait until a system alert is present and return its text (read-only).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "timeoutMs": { "type": "integer", "default": 10000 }
                },
                "additionalProperties": false
            }),
        ),
    ]
}
