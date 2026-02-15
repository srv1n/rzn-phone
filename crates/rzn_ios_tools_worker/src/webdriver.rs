use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const W3C_ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CREATE_SESSION_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct WebDriverClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateRequest {
    pub udid: String,
    pub no_reset: bool,
    pub new_command_timeout_sec: u64,
    pub session_create_timeout_ms: Option<u64>,
    pub wda_local_port: Option<u16>,
    pub wda_launch_timeout_ms: Option<u64>,
    pub wda_connection_timeout_ms: Option<u64>,
    pub language: Option<String>,
    pub locale: Option<String>,
    pub show_xcode_log: Option<bool>,
    pub allow_provisioning_updates: Option<bool>,
    pub allow_provisioning_device_registration: Option<bool>,
    pub xcode_org_id: Option<String>,
    pub xcode_signing_id: Option<String>,
    pub updated_wda_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateResult {
    pub session_id: String,
    pub capabilities: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ElementRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WebDriverClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let trimmed = base_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            bail!("base URL is empty");
        }

        Ok(Self {
            base_url: trimmed.to_string(),
            client: Client::builder()
                .timeout(DEFAULT_REQUEST_TIMEOUT)
                .build()
                .context("build reqwest client")?,
        })
    }

    #[allow(dead_code)]
    pub async fn status(&self) -> Result<Value> {
        self.get_json("/status").await
    }

    pub async fn create_session_safari(
        &self,
        request: SessionCreateRequest,
    ) -> Result<SessionCreateResult> {
        let mut caps = json!({
            "platformName": "iOS",
            "browserName": "Safari",
            "pageLoadStrategy": "eager",
            "appium:automationName": "XCUITest",
            "appium:udid": request.udid,
            "appium:noReset": request.no_reset,
            "appium:newCommandTimeout": request.new_command_timeout_sec,
        });

        if let Some(value) = request.wda_local_port {
            caps["appium:wdaLocalPort"] = json!(value);
        }
        if let Some(value) = request.wda_launch_timeout_ms {
            caps["appium:wdaLaunchTimeout"] = json!(value);
        }
        if let Some(value) = request.wda_connection_timeout_ms {
            caps["appium:wdaConnectionTimeout"] = json!(value);
        }
        if let Some(language) = request.language {
            caps["appium:language"] = json!(language);
        }
        if let Some(locale) = request.locale {
            caps["appium:locale"] = json!(locale);
        }
        if let Some(show_xcode_log) = request.show_xcode_log {
            caps["appium:showXcodeLog"] = json!(show_xcode_log);
        }
        if let Some(value) = request.allow_provisioning_updates {
            caps["appium:allowProvisioningUpdates"] = json!(value);
        }
        if let Some(value) = request.allow_provisioning_device_registration {
            caps["appium:allowProvisioningDeviceRegistration"] = json!(value);
        }
        if let Some(value) = request.xcode_org_id {
            caps["appium:xcodeOrgId"] = json!(value);
        }
        if let Some(value) = request.xcode_signing_id {
            caps["appium:xcodeSigningId"] = json!(value);
        }
        if let Some(value) = request.updated_wda_bundle_id {
            caps["appium:updatedWDABundleId"] = json!(value);
        }

        let payload = json!({
            "capabilities": {
                "alwaysMatch": caps,
                "firstMatch": [{}]
            },
            "desiredCapabilities": caps
        });

        let create_timeout = request
            .session_create_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_CREATE_SESSION_TIMEOUT);
        let response = self
            .post_json_with_timeout("/session", payload, create_timeout)
            .await?;
        let session_id = parse_session_id(&response)?;
        let capabilities = response
            .get("value")
            .and_then(Value::as_object)
            .and_then(|value| value.get("capabilities"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        Ok(SessionCreateResult {
            session_id,
            capabilities,
        })
    }

    pub async fn create_session_native_app(
        &self,
        request: SessionCreateRequest,
        bundle_id: String,
    ) -> Result<SessionCreateResult> {
        if bundle_id.trim().is_empty() {
            bail!("bundleId is empty");
        }

        let mut caps = json!({
            "platformName": "iOS",
            "appium:automationName": "XCUITest",
            "appium:udid": request.udid,
            "appium:bundleId": bundle_id.trim(),
            "appium:noReset": request.no_reset,
            "appium:newCommandTimeout": request.new_command_timeout_sec,
        });

        if let Some(value) = request.wda_local_port {
            caps["appium:wdaLocalPort"] = json!(value);
        }
        if let Some(value) = request.wda_launch_timeout_ms {
            caps["appium:wdaLaunchTimeout"] = json!(value);
        }
        if let Some(value) = request.wda_connection_timeout_ms {
            caps["appium:wdaConnectionTimeout"] = json!(value);
        }
        if let Some(language) = request.language {
            caps["appium:language"] = json!(language);
        }
        if let Some(locale) = request.locale {
            caps["appium:locale"] = json!(locale);
        }
        if let Some(show_xcode_log) = request.show_xcode_log {
            caps["appium:showXcodeLog"] = json!(show_xcode_log);
        }
        if let Some(value) = request.allow_provisioning_updates {
            caps["appium:allowProvisioningUpdates"] = json!(value);
        }
        if let Some(value) = request.allow_provisioning_device_registration {
            caps["appium:allowProvisioningDeviceRegistration"] = json!(value);
        }
        if let Some(value) = request.xcode_org_id {
            caps["appium:xcodeOrgId"] = json!(value);
        }
        if let Some(value) = request.xcode_signing_id {
            caps["appium:xcodeSigningId"] = json!(value);
        }
        if let Some(value) = request.updated_wda_bundle_id {
            caps["appium:updatedWDABundleId"] = json!(value);
        }

        let payload = json!({
            "capabilities": {
                "alwaysMatch": caps,
                "firstMatch": [{}]
            },
            "desiredCapabilities": caps
        });

        let create_timeout = request
            .session_create_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_CREATE_SESSION_TIMEOUT);
        let response = self
            .post_json_with_timeout("/session", payload, create_timeout)
            .await?;
        let session_id = parse_session_id(&response)?;
        let capabilities = response
            .get("value")
            .and_then(Value::as_object)
            .and_then(|value| value.get("capabilities"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        Ok(SessionCreateResult {
            session_id,
            capabilities,
        })
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}");
        let _ = self.delete_json(&path).await?;
        Ok(())
    }

    pub async fn goto_url(&self, session_id: &str, url: &str) -> Result<()> {
        let path = format!("/session/{session_id}/url");
        let _ = self.post_json(&path, json!({"url": url})).await?;
        Ok(())
    }

    pub async fn get_current_url(&self, session_id: &str) -> Result<String> {
        let path = format!("/session/{session_id}/url");
        let value = self.get_json(&path).await?;
        extract_value_as_string(&value).ok_or_else(|| anyhow!("missing URL in WebDriver response"))
    }

    pub async fn get_title(&self, session_id: &str) -> Result<String> {
        let path = format!("/session/{session_id}/title");
        let value = self.get_json(&path).await?;
        extract_value_as_string(&value)
            .ok_or_else(|| anyhow!("missing title in WebDriver response"))
    }

    pub async fn find_elements_css(&self, session_id: &str, selector: &str) -> Result<Vec<String>> {
        self.find_elements(session_id, "css selector", selector).await
    }

    pub async fn find_elements(&self, session_id: &str, using: &str, value: &str) -> Result<Vec<String>> {
        let using = using.trim();
        let value = value.trim();
        if using.is_empty() {
            bail!("locator 'using' is empty");
        }
        if value.is_empty() {
            bail!("locator 'value' is empty");
        }

        let path = format!("/session/{session_id}/elements");
        let payload = json!({
            "using": using,
            "value": value,
        });
        let response = self.post_json(&path, payload).await?;
        let list = response
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("missing value array for find elements"))?;

        let mut ids = Vec::with_capacity(list.len());
        for item in list {
            if let Some(id) = parse_element_id(item) {
                ids.push(id);
            }
        }

        Ok(ids)
    }

    pub async fn find_elements_from_element(
        &self,
        session_id: &str,
        element_id: &str,
        using: &str,
        value: &str,
    ) -> Result<Vec<String>> {
        let element_id = element_id.trim();
        let using = using.trim();
        let value = value.trim();
        if element_id.is_empty() {
            bail!("element_id is empty");
        }
        if using.is_empty() {
            bail!("locator 'using' is empty");
        }
        if value.is_empty() {
            bail!("locator 'value' is empty");
        }

        let path = format!("/session/{session_id}/element/{element_id}/elements");
        let payload = json!({
            "using": using,
            "value": value,
        });
        let response = self.post_json(&path, payload).await?;
        let list = response
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("missing value array for find elements"))?;

        let mut ids = Vec::with_capacity(list.len());
        for item in list {
            if let Some(id) = parse_element_id(item) {
                ids.push(id);
            }
        }

        Ok(ids)
    }

    pub async fn element_text(&self, session_id: &str, element_id: &str) -> Result<String> {
        let element_id = element_id.trim();
        if element_id.is_empty() {
            bail!("element_id is empty");
        }

        let path = format!("/session/{session_id}/element/{element_id}/text");
        let response = self.get_json(&path).await?;
        extract_value_as_string(&response)
            .ok_or_else(|| anyhow!("missing element text from response"))
    }

    pub async fn element_attribute(
        &self,
        session_id: &str,
        element_id: &str,
        name: &str,
    ) -> Result<Option<String>> {
        let element_id = element_id.trim();
        let name = name.trim();
        if element_id.is_empty() {
            bail!("element_id is empty");
        }
        if name.is_empty() {
            bail!("attribute name is empty");
        }

        let path = format!("/session/{session_id}/element/{element_id}/attribute/{name}");
        let response = self.get_json(&path).await?;
        let Some(value) = response.get("value") else {
            return Ok(None);
        };

        if value.is_null() {
            return Ok(None);
        }
        if let Some(s) = value.as_str() {
            return Ok(Some(s.to_string()));
        }
        if let Some(n) = value.as_i64() {
            return Ok(Some(n.to_string()));
        }
        if let Some(n) = value.as_u64() {
            return Ok(Some(n.to_string()));
        }
        if let Some(n) = value.as_f64() {
            return Ok(Some(n.to_string()));
        }
        if let Some(b) = value.as_bool() {
            return Ok(Some(b.to_string()));
        }

        Ok(Some(value.to_string()))
    }

    pub async fn element_rect(&self, session_id: &str, element_id: &str) -> Result<ElementRect> {
        let element_id = element_id.trim();
        if element_id.is_empty() {
            bail!("element_id is empty");
        }

        let path = format!("/session/{session_id}/element/{element_id}/rect");
        let response = self.get_json(&path).await?;
        let value = response
            .get("value")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("missing rect value"))?;

        Ok(ElementRect {
            x: value.get("x").and_then(Value::as_f64).unwrap_or(0.0),
            y: value.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            width: value.get("width").and_then(Value::as_f64).unwrap_or(0.0),
            height: value.get("height").and_then(Value::as_f64).unwrap_or(0.0),
        })
    }

    pub async fn click_element(&self, session_id: &str, element_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}/element/{element_id}/click");
        let _ = self.post_json(&path, json!({})).await?;
        Ok(())
    }

    pub async fn clear_element(&self, session_id: &str, element_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}/element/{element_id}/clear");
        let _ = self.post_json(&path, json!({})).await?;
        Ok(())
    }

    pub async fn type_element(&self, session_id: &str, element_id: &str, text: &str) -> Result<()> {
        let path = format!("/session/{session_id}/element/{element_id}/value");
        let chars: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
        let payload = json!({
            "text": text,
            "value": chars,
        });
        let _ = self.post_json(&path, payload).await?;
        Ok(())
    }

    pub async fn press_enter(&self, session_id: &str) -> Result<()> {
        let actions_path = format!("/session/{session_id}/actions");
        let action_payload = json!({
            "actions": [{
                "type": "key",
                "id": "keyboard",
                "actions": [
                    {"type": "keyDown", "value": "\u{E007}"},
                    {"type": "keyUp", "value": "\u{E007}"}
                ]
            }]
        });

        if self.post_json(&actions_path, action_payload).await.is_ok() {
            return Ok(());
        }

        let active_path = format!("/session/{session_id}/element/active");
        let active_response = self.get_json(&active_path).await?;
        let active_id = active_response
            .get("value")
            .and_then(parse_element_id)
            .ok_or_else(|| anyhow!("no active element to send ENTER key"))?;
        self.type_element(session_id, &active_id, "\n").await
    }

    pub async fn perform_actions(&self, session_id: &str, actions: Value) -> Result<()> {
        let path = format!("/session/{session_id}/actions");
        let _ = self.post_json(&path, actions).await?;
        Ok(())
    }

    pub async fn tap_point(&self, session_id: &str, x: f64, y: f64) -> Result<()> {
        let payload = json!({
            "actions": [{
                "type": "pointer",
                "id": "finger1",
                "parameters": { "pointerType": "touch" },
                "actions": [
                    {"type": "pointerMove", "duration": 0, "x": x, "y": y, "origin": "viewport"},
                    {"type": "pointerDown", "button": 0},
                    {"type": "pause", "duration": 75},
                    {"type": "pointerUp", "button": 0}
                ]
            }]
        });
        self.perform_actions(session_id, payload).await
    }

    pub async fn window_rect(&self, session_id: &str) -> Result<(f64, f64)> {
        let path = format!("/session/{session_id}/window/rect");
        let response = self.get_json(&path).await?;
        let value = response
            .get("value")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("missing window rect value"))?;
        let width = value.get("width").and_then(Value::as_f64).unwrap_or(0.0);
        let height = value.get("height").and_then(Value::as_f64).unwrap_or(0.0);
        if width <= 0.0 || height <= 0.0 {
            bail!("invalid window rect: width={width}, height={height}");
        }
        Ok((width, height))
    }

    pub async fn back(&self, session_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}/back");
        let _ = self.post_json(&path, json!({})).await?;
        Ok(())
    }

    pub async fn alert_text(&self, session_id: &str) -> Result<String> {
        let path = format!("/session/{session_id}/alert/text");
        let response = self.get_json(&path).await?;
        extract_value_as_string(&response).ok_or_else(|| anyhow!("missing alert text from response"))
    }

    pub async fn alert_accept(&self, session_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}/alert/accept");
        let _ = self.post_json(&path, json!({})).await?;
        Ok(())
    }

    pub async fn alert_dismiss(&self, session_id: &str) -> Result<()> {
        let path = format!("/session/{session_id}/alert/dismiss");
        let _ = self.post_json(&path, json!({})).await?;
        Ok(())
    }

    pub async fn execute_script(
        &self,
        session_id: &str,
        script: &str,
        args: Value,
    ) -> Result<Value> {
        let path = format!("/session/{session_id}/execute/sync");
        let payload = json!({
            "script": script,
            "args": args,
        });
        self.post_json(&path, payload).await
    }

    pub async fn page_source(&self, session_id: &str) -> Result<String> {
        let path = format!("/session/{session_id}/source");
        let response = self.get_json(&path).await?;
        extract_value_as_string(&response)
            .ok_or_else(|| anyhow!("missing page source from response"))
    }

    pub async fn screenshot(&self, session_id: &str) -> Result<String> {
        let path = format!("/session/{session_id}/screenshot");
        let response = self.get_json(&path).await?;
        extract_value_as_string(&response)
            .ok_or_else(|| anyhow!("missing screenshot data from response"))
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("webdriver GET failed")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "".to_string());
        if !status.is_success() {
            bail!("webdriver GET failed with status {status}: {body}");
        }
        serde_json::from_str(&body).context("invalid webdriver JSON response")
    }

    async fn post_json(&self, path: &str, payload: Value) -> Result<Value> {
        self.post_json_with_timeout(path, payload, DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    async fn post_json_with_timeout(
        &self,
        path: &str,
        payload: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(url)
            .timeout(timeout)
            .json(&payload)
            .send()
            .await
            .context("webdriver POST failed")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "".to_string());
        if !status.is_success() {
            bail!("webdriver POST failed with status {status}: {body}");
        }
        serde_json::from_str(&body).context("invalid webdriver JSON response")
    }

    async fn delete_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(url)
            .send()
            .await
            .context("webdriver DELETE failed")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "".to_string());
        if !status.is_success() {
            bail!("webdriver DELETE failed with status {status}: {body}");
        }
        serde_json::from_str(&body).context("invalid webdriver JSON response")
    }
}

pub fn parse_session_id(response: &Value) -> Result<String> {
    if let Some(session_id) = response.get("sessionId").and_then(Value::as_str) {
        if !session_id.is_empty() {
            return Ok(session_id.to_string());
        }
    }

    if let Some(session_id) = response
        .get("value")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("sessionId"))
        .and_then(Value::as_str)
    {
        if !session_id.is_empty() {
            return Ok(session_id.to_string());
        }
    }

    bail!("missing sessionId in create session response")
}

pub fn parse_element_id(value: &Value) -> Option<String> {
    value
        .get(W3C_ELEMENT_KEY)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("ELEMENT")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn extract_value_as_string(response: &Value) -> Option<String> {
    response
        .get("value")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::{parse_element_id, parse_session_id, WebDriverClient};
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use serde_json::json;

    #[test]
    fn parses_w3c_session_id() {
        let id = parse_session_id(&json!({"value": {"sessionId": "abc"}})).expect("id");
        assert_eq!(id, "abc");
    }

    #[test]
    fn parses_legacy_session_id() {
        let id = parse_session_id(&json!({"sessionId": "legacy"})).expect("id");
        assert_eq!(id, "legacy");
    }

    #[test]
    fn parses_element_id_w3c_and_legacy() {
        let w3c = parse_element_id(&json!({"element-6066-11e4-a52e-4f735466cecf": "one"}));
        assert_eq!(w3c.as_deref(), Some("one"));

        let legacy = parse_element_id(&json!({"ELEMENT": "two"}));
        assert_eq!(legacy.as_deref(), Some("two"));
    }

    #[tokio::test]
    async fn status_hits_expected_endpoint() {
        let server = MockServer::start_async().await;
        let status_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/status");
                then.status(200)
                    .json_body(json!({"value": {"ready": true}}));
            })
            .await;

        let client = WebDriverClient::new(&server.url("")).expect("client");
        let value = client.status().await.expect("status");

        status_mock.assert_async().await;
        assert!(value.get("value").is_some());
    }

    #[tokio::test]
    async fn create_session_posts_session_endpoint() {
        let server = MockServer::start_async().await;
        let create_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/session");
                then.status(200)
                    .json_body(json!({"value": {"sessionId": "sess-1", "capabilities": {"browserName": "Safari"}}}));
            })
            .await;

        let client = WebDriverClient::new(&server.url("")).expect("client");
        let output = client
            .create_session_safari(super::SessionCreateRequest {
                udid: "123".to_string(),
                no_reset: true,
                new_command_timeout_sec: 60,
                session_create_timeout_ms: None,
                wda_local_port: None,
                wda_launch_timeout_ms: None,
                wda_connection_timeout_ms: None,
                language: None,
                locale: None,
                show_xcode_log: None,
                allow_provisioning_updates: None,
                allow_provisioning_device_registration: None,
                xcode_org_id: None,
                xcode_signing_id: None,
                updated_wda_bundle_id: None,
            })
            .await
            .expect("session");

        create_mock.assert_async().await;
        assert_eq!(output.session_id, "sess-1");
    }

    #[tokio::test]
    async fn find_elements_from_element_posts_expected_endpoint() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/session/sess-1/element/el-1/elements");
                then.status(200).json_body(json!({
                    "value": [
                        {"element-6066-11e4-a52e-4f735466cecf": "child-1"},
                        {"ELEMENT": "child-2"}
                    ]
                }));
            })
            .await;

        let client = WebDriverClient::new(&server.url("")).expect("client");
        let ids = client
            .find_elements_from_element("sess-1", "el-1", "css selector", ".child")
            .await
            .expect("ids");
        mock.assert_async().await;
        assert_eq!(ids, vec!["child-1".to_string(), "child-2".to_string()]);
    }

    #[tokio::test]
    async fn element_text_hits_expected_endpoint() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/session/sess-1/element/el-1/text");
                then.status(200).json_body(json!({"value": "hello"}));
            })
            .await;

        let client = WebDriverClient::new(&server.url("")).expect("client");
        let text = client.element_text("sess-1", "el-1").await.expect("text");
        mock.assert_async().await;
        assert_eq!(text, "hello");
    }

    #[tokio::test]
    async fn alert_accept_posts_expected_endpoint() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/session/sess-1/alert/accept");
                then.status(200).json_body(json!({"value": null}));
            })
            .await;

        let client = WebDriverClient::new(&server.url("")).expect("client");
        client.alert_accept("sess-1").await.expect("accept");
        mock.assert_async().await;
    }
}
