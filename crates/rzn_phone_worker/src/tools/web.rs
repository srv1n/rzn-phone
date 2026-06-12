use serde_json::{json, Value};

use super::registry::tool;

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &[
    "ios.web.goto",
    "ios.web.wait_css",
    "ios.web.wait_js",
    "ios.web.click_css",
    "ios.web.type_css",
    "ios.web.press_key",
    "ios.web.page_source",
    "ios.web.screenshot",
    "ios.web.eval_js",
];

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
        	            "ios.web.goto",
        	            "Navigate Safari session to a URL.",
        	            json!({
        	                "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" },
                            "url": { "type": "string" }
                        },
                        "required": ["url"],
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.wait_css",
                    "Wait for a CSS selector and return a matching element id.",
                    json!({
                        "type": "object",
        	                "properties": {
        	                    "sessionId": { "type": "string" },
        	                    "selector": { "type": "string" },
        	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
        	                    "requireUnique": { "type": "boolean", "default": false },
        	                    "timeoutMs": { "type": "integer", "default": 10000 }
        	                },
        	                "required": ["selector"],
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.wait_js",
                    "Wait until a JavaScript expression returns a truthy result in the current page context.",
                    json!({
                        "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" },
                            "script": { "type": "string" },
                            "args": { "type": "array", "items": {} },
                            "timeoutMs": { "type": "integer", "default": 10000 },
                            "intervalMs": { "type": "integer", "default": 250 }
                        },
                        "required": ["script"],
                        "additionalProperties": false
                    }),
                ),
        tool(
        	            "ios.web.click_css",
        	            "Click an element matching a CSS selector.",
        	            json!({
        	                "type": "object",
        	                "properties": {
        	                    "sessionId": { "type": "string" },
        	                    "selector": { "type": "string" },
        	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
        	                    "requireUnique": { "type": "boolean", "default": false }
        	                },
        	                "required": ["selector"],
        	                "additionalProperties": false
                    }),
                ),
        tool(
        	            "ios.web.type_css",
        	            "Type text into an element matching a CSS selector.",
        	            json!({
        	                "type": "object",
        	                "properties": {
        	                    "sessionId": { "type": "string" },
        	                    "selector": { "type": "string" },
        	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
        	                    "requireUnique": { "type": "boolean", "default": false },
        	                    "text": { "type": "string" },
        	                    "clearFirst": { "type": "boolean", "default": true }
        	                },
                        "required": ["selector", "text"],
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.press_key",
                    "Send a keyboard key to the active element (supports Enter for MVP).",
                    json!({
                        "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" },
                            "key": { "type": "string", "default": "Enter" }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.page_source",
                    "Get current page source.",
                    json!({
                        "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.screenshot",
                    "Capture a screenshot from the active session.",
                    json!({
                        "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "ios.web.eval_js",
                    "UNSAFE (high-risk): execute raw JavaScript in the current page context.",
                    json!({
                        "type": "object",
                        "properties": {
                            "sessionId": { "type": "string" },
                            "script": { "type": "string" },
                            "args": { "type": "array", "items": {} }
                        },
                        "required": ["script"],
                        "additionalProperties": false
                    }),
                ),
    ]
}
