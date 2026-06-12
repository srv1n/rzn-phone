use serde_json::{json, Value};

use crate::state::AppState;
use crate::tools;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

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
        "initialize" => Ok(initialize_result(&params)),
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
                    "description": "Use workflow-or-tool observe -> act -> verify loops for iPhone automation.",
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
                            "Task: {task}. Start with ios.capability.list to pick the smallest Tier-1 family that fits the job, then check ios.workflow.list for a clean prebuilt match in that family and run it when one exists. If no workflow fits, expand into Tier-2 tools and drive the phone directly in short observe-act-verify loops: ios.appium.ensure -> ios.session.create -> ios.ui.observe_compact -> ios.action.*. Re-observe after each state change. Use ios.target.resolve only when you need a raw locator, ios.ui.screenshot or ios.ui.source for debugging, and ios.web.* only for Safari/web tasks. Stay read-only unless the task explicitly calls for commit-gated mutation."
                        )
                    }
                ]
            }))
        }
        _ => Err(McpMethodError::method_not_found(method)),
    }
}

fn initialize_result(params: &Value) -> Value {
    let protocol_version = negotiated_protocol_version(params);

    json!({
        "name": "rzn-phone-worker",
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": { "listChanged": false },
            "prompts": { "listChanged": false },
            "experimental": {}
        },
        "serverInfo": {
            "name": "rzn-phone-worker",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn negotiated_protocol_version(params: &Value) -> &str {
    params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(MCP_PROTOCOL_VERSION)
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
        assert!(result
            .get("capabilities")
            .and_then(|value| value.get("resources"))
            .is_none());
    }

    #[tokio::test]
    async fn initialize_reflects_client_protocol_version_when_provided() {
        let state = AppState::new();
        let result = handle_request(
            &state,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "contract-test", "version": "0" }
            }),
        )
        .await
        .expect("initialize");

        assert_eq!(
            result
                .get("protocolVersion")
                .and_then(|value| value.as_str()),
            Some("2024-11-05")
        );
    }
}
