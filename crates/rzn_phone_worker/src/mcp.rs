use serde_json::{json, Value};

use crate::state::AppState;
use crate::tools;

#[derive(Debug, Clone)]
pub struct McpMethodError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl McpMethodError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }
}

pub async fn handle_request(
    state: &AppState,
    method: &str,
    params: Value,
) -> Result<Value, McpMethodError> {
    match method {
        "initialize" => Ok(initialize_result()),
        "tools/list" => Ok(json!({ "tools": tools::list_tool_definitions() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| McpMethodError::invalid_params("tools/call requires params.name"))?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            match tools::handle_tool_call(state, name, arguments).await {
                Ok(result) => Ok(result),
                Err(err) => Ok(tools::tool_error_from_anyhow(&err, name)),
            }
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| {
                McpMethodError::invalid_params("resources/read requires params.uri")
            })?;
            Ok(json!({
                "contents": [{
                    "type": "text",
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": "resource support is not implemented in this MVP"
                }]
            }))
        }
        "prompts/list" => Ok(json!({
            "prompts": [
                {
                    "name": "ios.autonomy.loop",
                    "description": "Use snapshot -> decide -> act loops for iOS web automation.",
                    "arguments": [
                        {"name": "task", "description": "Task objective", "required": true}
                    ]
                }
            ]
        })),
        "prompts/get" => {
            let name = params.get("name").and_then(Value::as_str).ok_or_else(|| {
                McpMethodError::invalid_params("prompts/get requires params.name")
            })?;

            if name != "ios.autonomy.loop" {
                return Err(McpMethodError::invalid_params(format!(
                    "unknown prompt '{name}'"
                )));
            }

            let task = params
                .get("arguments")
                .and_then(|value| value.get("task"))
                .and_then(Value::as_str)
                .unwrap_or("unspecified task");

            Ok(json!({
                "messages": [
                    {
                        "role": "system",
                        "content": format!(
                            "Task: {task}. Use ios.appium.ensure, ios.session.create, and ios.web.* tools in short observe-act loops. Validate each action with ios.web.wait_css or page checks before continuing."
                        )
                    }
                ]
            }))
        }
        _ => Err(McpMethodError::method_not_found(method)),
    }
}

fn initialize_result() -> Value {
    json!({
        "name": "rzn-phone-worker",
        "version": "0.1.0",
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false },
            "prompts": { "listChanged": false },
            "experimental": {}
        },
        "serverInfo": {
            "name": "rzn-phone-worker",
            "version": "0.1.0"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::handle_request;
    use crate::state::AppState;
    use serde_json::json;

    #[tokio::test]
    async fn initialize_includes_server_info_and_top_level_fields() {
        let state = AppState::new();
        let result = handle_request(&state, "initialize", json!({}))
            .await
            .expect("initialize");

        assert_eq!(
            result.get("name").and_then(|value| value.as_str()),
            Some("rzn-phone-worker")
        );
        assert!(result.get("serverInfo").is_some());
        assert_eq!(
            result
                .get("protocolVersion")
                .and_then(|value| value.as_str()),
            Some("2025-06-18")
        );
    }
}
