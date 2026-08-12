use serde_json::{json, Value};

use super::registry::tool;

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "util.rank_by_name",
            "Compute a 1-based rank for a target string in a list of items (generic helper).",
            json!({
                "type": "object",
                "properties": {
                    "items": { "type": "array" },
                    "field": { "type": "string", "default": "name" },
                    "target": { "type": "string" }
                },
                "required": ["items", "target"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.list.length",
            "Return the length of an array.",
            json!({
                "type": "object",
                "properties": {
                    "list": { "type": "array" }
                },
                "required": ["list"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.list.first",
            "Return the first item in an array (optionally extract a field).",
            json!({
                "type": "object",
                "properties": {
                    "list": { "type": "array" },
                    "field": { "type": "string" }
                },
                "required": ["list"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.list.nth",
            "Return the Nth (1-based) item in an array (optionally extract a field).",
            json!({
                "type": "object",
                "properties": {
                    "list": { "type": "array" },
                    "index": { "type": "integer", "minimum": 1 },
                    "field": { "type": "string" }
                },
                "required": ["list", "index"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.list.find",
            "Return the first list item whose string field matches generic contains or regex predicates.",
            json!({
                "type": "object",
                "properties": {
                    "list": { "type": "array" },
                    "field": { "type": "string" },
                    "contains": { "type": "string" },
                    "notContains": { "type": "string" },
                    "regex": { "type": "string" },
                    "caseSensitive": { "type": "boolean", "default": false },
                    "startOffset": { "type": "integer", "minimum": 0, "default": 0 }
                },
                "required": ["list"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.rect.relative_point",
            "Compute a tap point inside a rectangle using relative X/Y fractions.",
            json!({
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "width": { "type": "number" },
                    "height": { "type": "number" },
                    "relativeX": { "type": "number", "default": 0.5 },
                    "relativeY": { "type": "number", "default": 0.5 }
                },
                "required": ["x", "y", "width", "height"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.fail",
            "Fail intentionally with a structured workflow/runtime error. Useful for explicit precondition checks.",
            json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" },
                    "details": { "type": "object" }
                },
                "required": ["message"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.sleep",
            "Sleep for a bounded random duration (milliseconds). Useful for human-like dwell windows.",
            json!({
                "type": "object",
                "properties": {
                    "minMs": { "type": "integer", "minimum": 0, "maximum": 600000, "default": 400 },
                    "maxMs": { "type": "integer", "minimum": 0, "maximum": 600000, "default": 900 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "util.date.bucket_counts",
            "Parse date-like strings and compute counts within day windows (generic helper).",
            json!({
                "type": "object",
                "properties": {
                    "items": { "type": "array" },
                    "field": { "type": "string" },
                    "windowsDays": { "type": "array", "items": { "type": "integer", "minimum": 1 } },
                    "nowEpochMs": { "type": "integer" }
                },
                "required": ["items", "windowsDays"],
                "additionalProperties": false
            }),
        ),
    ]
}
