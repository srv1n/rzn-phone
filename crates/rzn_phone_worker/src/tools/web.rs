use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::time::Duration;

use super::registry::tool;
use super::{
    driver_from_state, is_retryable_webdriver_error, required_str, resolve_session_id,
    screenshot_tool_result, tool_success, wait_for_selector,
};
use crate::errors::{ToolCallError, ToolErrorCode};
use crate::state::AppState;

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
            "UNSAFE (high-risk): poll caller-provided JavaScript in the current page context until it returns truthy; the script is executed repeatedly and can have side effects.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "script": { "type": "string", "description": "Caller-provided JavaScript executed repeatedly while waiting; it can have side effects." },
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

pub(crate) async fn handle(
    state: &AppState,
    tool_name: &str,
    arguments: &Value,
) -> Option<Result<Value>> {
    Some(match tool_name {
        "ios.web.goto" => web_goto(state, arguments).await,
        "ios.web.wait_css" => web_wait_css(state, arguments).await,
        "ios.web.wait_js" => web_wait_js(state, arguments).await,
        "ios.web.click_css" => web_click_css(state, arguments).await,
        "ios.web.type_css" => web_type_css(state, arguments).await,
        "ios.web.press_key" => web_press_key(state, arguments).await,
        "ios.web.page_source" => web_page_source(state, arguments).await,
        "ios.web.screenshot" => web_screenshot(state, arguments).await,
        "ios.web.eval_js" => web_eval_js(state, arguments).await,
        _ => return None,
    })
}

async fn web_goto(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let url = required_str(arguments, "url")?;
    let driver = driver_from_state(state).await?;
    // Page/web-context selection is handled at session create via the
    // `safariInitialUrl` capability (see config::resolve_safari_web): pointing
    // Safari at a known page makes appium attach to the real tab rather than a
    // phantom `safari-web-extension://` page. Switching windows here instead
    // *fights* that selection (it can land eval on a stale/blank handle), so we
    // deliberately do not touch the active window — just navigate it.
    driver.goto_url(&session_id, url).await?;

    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "url": url}),
        "navigation complete",
    ))
}

async fn web_wait_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(500, 120_000);

    let driver = driver_from_state(state).await?;
    let element_id = wait_for_selector(
        &driver,
        &session_id,
        selector,
        index,
        require_unique,
        Duration::from_millis(timeout_ms),
    )
    .await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id
        }),
        "selector found",
    ))
}

fn js_value_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value
            .as_f64()
            .map(|number| number.is_finite() && number != 0.0)
            .unwrap_or(false),
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(_) => true,
    }
}

async fn web_wait_js(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let script = required_str(arguments, "script")?;
    let args = arguments.get("args").cloned().unwrap_or_else(|| json!([]));
    if !args.is_array() {
        bail!("args must be an array for ios.web.wait_js");
    }

    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(500, 120_000);
    let interval_ms = arguments
        .get("intervalMs")
        .and_then(Value::as_u64)
        .unwrap_or(250)
        .clamp(50, 5_000);

    let driver = driver_from_state(state).await?;
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_retryable_error: Option<String> = None;
    loop {
        let response = match driver
            .execute_script(&session_id, script, args.clone())
            .await
        {
            Ok(response) => Some(response),
            Err(err) if is_retryable_webdriver_error(&err) => {
                last_retryable_error = Some(format!("{err:#}"));
                None
            }
            Err(err) => return Err(err),
        };
        if let Some(response) = response {
            let result = response.get("value").cloned().unwrap_or(Value::Null);
            if js_value_is_truthy(&result) {
                return Ok(tool_success(
                    json!({
                        "ok": true,
                        "sessionId": session_id,
                        "result": result
                    }),
                    "script condition satisfied",
                ));
            }
        }

        if std::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                "timeout waiting for JavaScript condition",
                json!({
                    "tool": "ios.web.wait_js",
                    "timeoutMs": timeout_ms,
                    "lastRetryableError": last_retryable_error
                }),
            )
            .into());
        }

        tokio::time::sleep(Duration::from_millis(interval_ms)).await;
    }
}

async fn web_click_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let driver = driver_from_state(state).await?;

    let ids = driver.find_elements_css(&session_id, selector).await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no elements found for selector '{selector}'"),
            json!({"selector": selector}),
        )
        .into());
    }
    if require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for selector '{selector}', got {}",
                ids.len()
            ),
            json!({"selector": selector, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {index} for selector '{selector}' (found {})",
                ids.len()
            ),
            json!({"selector": selector, "index": index, "matchCount": ids.len()}),
        )
    })?;
    driver.click_element(&session_id, element_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id
        }),
        "click complete",
    ))
}

async fn web_type_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = required_str(arguments, "text")?;
    let clear_first = arguments
        .get("clearFirst")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let driver = driver_from_state(state).await?;
    let ids = driver.find_elements_css(&session_id, selector).await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no elements found for selector '{selector}'"),
            json!({"selector": selector}),
        )
        .into());
    }
    if require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for selector '{selector}', got {}",
                ids.len()
            ),
            json!({"selector": selector, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {index} for selector '{selector}' (found {})",
                ids.len()
            ),
            json!({"selector": selector, "index": index, "matchCount": ids.len()}),
        )
    })?;

    if clear_first {
        let _ = driver.clear_element(&session_id, element_id).await;
    }
    driver.type_element(&session_id, element_id, text).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id,
            "typedLength": text.chars().count()
        }),
        "type complete",
    ))
}

async fn web_press_key(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let key = arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("Enter")
        .to_lowercase();

    let driver = driver_from_state(state).await?;
    match key.as_str() {
        "enter" | "return" | "search" => driver.press_enter(&session_id).await?,
        _ => bail!("unsupported key '{key}', supported: Enter|Return|Search"),
    }

    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "key": key}),
        "key press complete",
    ))
}

async fn web_page_source(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let source = driver.page_source(&session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "length": source.len(),
            "source": source
        }),
        "page source captured",
    ))
}

async fn web_screenshot(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let data = driver.screenshot(&session_id).await?;

    Ok(screenshot_tool_result(&session_id, data))
}

async fn web_eval_js(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let script = required_str(arguments, "script")?;
    let args = arguments.get("args").cloned().unwrap_or_else(|| json!([]));

    if !args.is_array() {
        bail!("args must be an array for ios.web.eval_js");
    }

    let driver = driver_from_state(state).await?;
    let response = driver.execute_script(&session_id, script, args).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "risk": "high",
            "sessionId": session_id,
            "result": response.get("value").cloned().unwrap_or(Value::Null)
        }),
        "script executed",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wait_js_definition_warns_about_side_effecting_caller_javascript() {
        let definitions = definitions();
        let definition = definitions
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("ios.web.wait_js"))
            .expect("ios.web.wait_js definition");

        let description = definition
            .get("description")
            .and_then(Value::as_str)
            .expect("description")
            .to_ascii_lowercase();
        assert!(description.contains("high-risk"));
        assert!(description.contains("caller-provided javascript"));
        assert!(description.contains("side effects"));

        assert_eq!(definition.get("risk"), Some(&json!("high")));
        assert_eq!(definition.get("allowedDirect"), Some(&json!(false)));
        assert_eq!(definition.get("allowedInWorkflow"), Some(&json!(true)));

        let script_description = definition
            .get("inputSchema")
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.get("script"))
            .and_then(|script| script.get("description"))
            .and_then(Value::as_str)
            .expect("script description")
            .to_ascii_lowercase();
        assert!(script_description.contains("side effects"));
        assert!(script_description.contains("repeatedly"));
    }
}
