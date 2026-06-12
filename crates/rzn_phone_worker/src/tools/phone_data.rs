use serde_json::{json, Value};

use super::registry::tool;

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &[
    "phone_messages.list_recent_threads",
    "phone_messages.read_latest_messages",
    "phone_messages.find_recent_otp",
    "phone_calls.list_recent_calls",
    "phone_notifications.list_recent_notifications",
    "phone_notifications.filter_notifications_by_app",
];

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
                    "phone_messages.list_recent_threads",
                    "List recent conversation threads from the Messages app on a paired iPhone.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "maxThreads": { "type": "integer", "minimum": 1, "maximum": 50, "default": 25 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": true },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "phone_messages.read_latest_messages",
                    "Open a recent Messages thread and read the latest visible messages without sending anything.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "threadId": { "type": "string", "description": "Thread id returned by phone_messages.list_recent_threads." },
                            "threadIndex": { "type": "integer", "minimum": 0, "default": 0 },
                            "maxMessages": { "type": "integer", "minimum": 1, "maximum": 50, "default": 20 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": true },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "phone_messages.find_recent_otp",
                    "Scan recent Messages threads for likely authentication codes / OTPs without sending anything.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "maxThreads": { "type": "integer", "minimum": 1, "maximum": 20, "default": 5 },
                            "maxMessages": { "type": "integer", "minimum": 1, "maximum": 50, "default": 8 },
                            "threadContains": { "type": "string", "description": "Optional thread title/preview filter (for example service name)." },
                            "senderContains": { "type": "string", "description": "Optional sender/thread filter to bias toward a specific service." },
                            "messageContains": { "type": "string", "description": "Optional message body substring to require." },
                            "codeLength": { "type": "integer", "minimum": 4, "maximum": 8, "description": "Exact OTP length to require." },
                            "minCodeLength": { "type": "integer", "minimum": 4, "maximum": 8, "default": 4 },
                            "maxCodeLength": { "type": "integer", "minimum": 4, "maximum": 8, "default": 8 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": true },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "phone_calls.list_recent_calls",
                    "List recent call history from the Phone app on a paired iPhone.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "maxCalls": { "type": "integer", "minimum": 1, "maximum": 50, "default": 25 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": true },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "phone_notifications.list_recent_notifications",
                    "Open Notification Center and list recent visible notifications from a paired iPhone.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "maxNotifications": { "type": "integer", "minimum": 1, "maximum": 50, "default": 25 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": false },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
        tool(
                    "phone_notifications.filter_notifications_by_app",
                    "List recent notifications, then filter them by visible app label.",
                    json!({
                        "type": "object",
                        "properties": {
                            "deviceId": { "type": "string", "description": "Paired iPhone UDID." },
                            "udid": { "type": "string", "description": "Alias of deviceId." },
                            "appLabel": { "type": "string", "description": "Visible app label to match against notification rows." },
                            "appPackage": { "type": "string", "description": "Alias of appLabel for compatibility; this worker filters by visible UI label, not bundle id." },
                            "maxNotifications": { "type": "integer", "minimum": 1, "maximum": 50, "default": 25 },
                            "backgroundAppOnFinish": { "type": "boolean", "default": false },
                            "lockDeviceOnFinish": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    }),
                ),
    ]
}
