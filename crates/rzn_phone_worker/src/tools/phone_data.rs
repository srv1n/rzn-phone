use serde_json::{json, Value};

use super::registry::tool;

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
                    "appPackage": { "type": "string", "description": "Compatibility alias accepted at runtime; appLabel is the canonical required schema field. This worker filters by visible UI label, not bundle id." },
                    "maxNotifications": { "type": "integer", "minimum": 1, "maximum": 50, "default": 25 },
                    "backgroundAppOnFinish": { "type": "boolean", "default": false },
                    "lockDeviceOnFinish": { "type": "boolean", "default": false }
                },
                "required": ["appLabel"],
                "additionalProperties": false
            }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_notifications_by_app_schema_requires_app_label() {
        let definitions = definitions();
        let tool = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str)
                    == Some("phone_notifications.filter_notifications_by_app")
            })
            .expect("filter notifications tool");
        let schema = tool.get("inputSchema").expect("input schema");

        assert_eq!(
            schema.get("required").and_then(Value::as_array),
            Some(&vec![json!("appLabel")])
        );
        assert!(schema
            .get("properties")
            .and_then(|properties| properties.get("appLabel"))
            .is_some());
        assert!(schema
            .get("properties")
            .and_then(|properties| properties.get("appPackage"))
            .and_then(|property| property.get("description"))
            .and_then(Value::as_str)
            .is_some_and(|description| description.contains("Compatibility alias")));
    }
}
