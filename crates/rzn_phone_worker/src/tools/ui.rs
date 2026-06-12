use serde_json::{json, Value};

use super::registry::tool;

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &[
    "ios.ui.source",
    "ios.ui.screenshot",
    "ios.ui.observe_compact",
    "ios.ui.extract_rows",
    "ios.ui.extract_text",
    "ios.ui.find_row",
    "ios.target.resolve",
    "ios.element.text",
    "ios.element.attribute",
    "ios.element.rect",
];

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "ios.ui.source",
            "Get the current UI hierarchy source XML for the active session (native or web).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.screenshot",
            "Capture a screenshot from the active session (native or web).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.observe_compact",
            "Return a compact, LLM-friendly UI snapshot (native apps only in MVP). Encoded ids can be used with ios.action.* tools.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "nodeFilter": { "type": "string", "enum": ["interactive", "all"], "default": "interactive" },
                    "maxNodes": { "type": "integer", "default": 140 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.extract_rows",
            "Extract ordered rows from a UI source XML using generic selectors and splitting rules.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "source": { "type": "string" },
                    "row": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeCell" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "nameContains": { "type": "string" },
                            "label": { "type": "string" },
                            "labelContains": { "type": "string" },
                            "visibleOnly": { "type": "boolean", "default": false },
                            "ancestorName": { "type": "string" },
                            "ancestorNameContains": { "type": "string" },
                            "ancestorType": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "primary": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeButton" },
                            "attr": { "type": "string", "enum": ["label", "name", "value"], "default": "label" },
                            "pick": { "type": "string", "enum": ["first", "longest"], "default": "longest" }
                        },
                        "additionalProperties": false
                    },
                    "tag": {
                        "type": "object",
                        "properties": {
                            "namePrefix": { "type": "string" },
                            "pick": { "type": "string", "enum": ["first", "last"], "default": "last" },
                            "stripPrefix": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "attr": { "type": "string", "enum": ["label", "name", "value"], "default": "label" },
                                "pick": { "type": "string", "enum": ["first", "last", "longest", "all"], "default": "first" },
                                "joinDelimiter": { "type": "string" },
                                "query": {
                                    "type": "object",
                                    "properties": {
                                        "type": { "type": "string", "default": "XCUIElementTypeStaticText" },
                                        "name": { "type": "string" },
                                        "namePrefix": { "type": "string" },
                                        "nameContains": { "type": "string" },
                                        "label": { "type": "string" },
                                        "labelContains": { "type": "string" },
                                        "visibleOnly": { "type": "boolean", "default": false },
                                        "ancestorName": { "type": "string" },
                                        "ancestorType": { "type": "string" },
                                        "max": { "type": "integer", "minimum": 1, "maximum": 100 }
                                    },
                                    "additionalProperties": false
                                }
                            },
                            "required": ["name", "query"],
                            "additionalProperties": false
                        }
                    },
                    "split": {
                        "type": "object",
                        "properties": {
                            "delimiter": { "type": "string", "default": "," },
                            "ignorePrefixes": { "type": "array", "items": { "type": "string" } },
                            "fields": { "type": "array", "items": { "type": "string" } },
                            "skipMetricLike": { "type": "boolean", "default": true }
                        },
                        "additionalProperties": false
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "maxScrolls": { "type": "integer", "minimum": 0, "maximum": 50, "default": 0 },
                    "scroll": {
                        "type": "object",
                        "properties": {
                            "direction": { "type": "string", "enum": ["down", "up", "left", "right"], "default": "down" },
                            "distance": { "type": "number", "minimum": 0.1, "maximum": 0.95, "default": 0.6 },
                            "settleMs": { "type": "integer", "minimum": 0, "maximum": 10000, "default": 350 }
                        },
                        "additionalProperties": false
                    },
                    "order": { "type": "string", "enum": ["y", "x"], "default": "y" }
                },
                "required": ["row", "primary"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.extract_text",
            "Extract ordered text nodes from a UI source XML using generic selectors.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "source": { "type": "string" },
                    "query": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeStaticText" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "nameContains": { "type": "string" },
                            "label": { "type": "string" },
                            "labelContains": { "type": "string" },
                            "visibleOnly": { "type": "boolean", "default": false },
                            "ancestorName": { "type": "string" },
                            "ancestorType": { "type": "string" },
                            "max": { "type": "integer", "minimum": 1, "maximum": 200 }
                        },
                        "additionalProperties": false
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                    "unique": { "type": "boolean", "default": true },
                    "order": { "type": "string", "enum": ["y", "x"], "default": "y" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.find_row",
            "Search rows pass-by-pass, optionally scrolling, and return the Nth matching row from the current viewport.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "source": { "type": "string" },
                    "row": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeCell" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "nameContains": { "type": "string" },
                            "label": { "type": "string" },
                            "labelContains": { "type": "string" },
                            "visibleOnly": { "type": "boolean", "default": false },
                            "ancestorName": { "type": "string" },
                            "ancestorNameContains": { "type": "string" },
                            "ancestorType": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "primary": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeButton" },
                            "attr": { "type": "string", "enum": ["label", "name", "value"], "default": "label" },
                            "pick": { "type": "string", "enum": ["first", "longest"], "default": "longest" }
                        },
                        "additionalProperties": false
                    },
                    "tag": {
                        "type": "object",
                        "properties": {
                            "namePrefix": { "type": "string" },
                            "pick": { "type": "string", "enum": ["first", "last"], "default": "last" },
                            "stripPrefix": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "attr": { "type": "string", "enum": ["label", "name", "value"], "default": "label" },
                                "pick": { "type": "string", "enum": ["first", "last", "longest", "all"], "default": "first" },
                                "joinDelimiter": { "type": "string" },
                                "query": {
                                    "type": "object",
                                    "properties": {
                                        "type": { "type": "string", "default": "XCUIElementTypeStaticText" },
                                        "name": { "type": "string" },
                                        "namePrefix": { "type": "string" },
                                        "nameContains": { "type": "string" },
                                        "label": { "type": "string" },
                                        "labelContains": { "type": "string" },
                                        "visibleOnly": { "type": "boolean", "default": false },
                                        "ancestorName": { "type": "string" },
                                        "ancestorType": { "type": "string" },
                                        "max": { "type": "integer", "minimum": 1, "maximum": 100 }
                                    },
                                    "additionalProperties": false
                                }
                            },
                            "required": ["name", "query"],
                            "additionalProperties": false
                        }
                    },
                    "split": {
                        "type": "object",
                        "properties": {
                            "delimiter": { "type": "string", "default": "," },
                            "ignorePrefixes": { "type": "array", "items": { "type": "string" } },
                            "fields": { "type": "array", "items": { "type": "string" } },
                            "skipMetricLike": { "type": "boolean", "default": true }
                        },
                        "additionalProperties": false
                    },
                    "match": {
                        "type": "object",
                        "properties": {
                            "field": { "type": "string", "default": "rawLabel" },
                            "contains": { "type": "string" },
                            "notContains": { "type": "string" },
                            "regex": { "type": "string" },
                            "caseSensitive": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    },
                    "matchIndex": { "type": "integer", "minimum": 0, "default": 0 },
                    "dedupeMatches": { "type": "boolean", "default": true },
                    "maxScrolls": { "type": "integer", "minimum": 0, "maximum": 50, "default": 0 },
                    "scroll": {
                        "type": "object",
                        "properties": {
                            "direction": { "type": "string", "enum": ["down", "up", "left", "right"], "default": "down" },
                            "distance": { "type": "number", "minimum": 0.1, "maximum": 0.95, "default": 0.6 },
                            "settleMs": { "type": "integer", "minimum": 0, "maximum": 10000, "default": 350 }
                        },
                        "additionalProperties": false
                    },
                    "order": { "type": "string", "enum": ["y", "x"], "default": "y" },
                    "includeSource": { "type": "boolean", "default": false }
                },
                "required": ["row", "primary", "match"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.target.resolve",
            "Resolve an encoded id from the latest compact snapshot into a WebDriver locator.",
            json!({
                "type": "object",
                "properties": {
                    "encodedId": { "type": "string" },
                    "snapshotId": { "type": "string" }
                },
                "required": ["encodedId"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.element.text",
            "Read element text by locator or encoded id (read-only).",
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
                    }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.element.attribute",
            "Read an element attribute by locator or encoded id (read-only).",
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
                    "name": { "type": "string" }
                },
                "required": ["target", "name"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.element.rect",
            "Read an element rect (x/y/width/height) by locator or encoded id (read-only).",
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
                    }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        ),
    ]
}
