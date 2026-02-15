use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;
use std::{collections::HashMap, env, fs, path::PathBuf};

use crate::appium::{ensure_appium, EnsureOptions};
use crate::state::AppState;
use crate::webdriver::{SessionCreateRequest, WebDriverClient};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

pub fn list_workflows() -> Vec<WorkflowInfo> {
    let mut by_name: HashMap<String, WorkflowInfo> = HashMap::new();

    for wf in list_file_workflows() {
        by_name.insert(wf.name.clone(), wf);
    }

    by_name
        .entry("safari.google_search".to_string())
        .or_insert_with(|| WorkflowInfo {
            name: "safari.google_search".to_string(),
            version: "1.0.0".to_string(),
            description: "Open Safari on iPhone, search Google, and return top results.".to_string(),
        });

    let mut out: Vec<WorkflowInfo> = by_name.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn run_workflow(state: &AppState, name: &str, args: &Value) -> Result<Value> {
    match name {
        "safari.google_search" => run_safari_google_search(state, args).await,
        other => bail!("unknown workflow: {other}"),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileWorkflowDefinition {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub steps: Option<Vec<Value>>,
}

pub fn load_file_workflow(name: &str) -> Option<FileWorkflowDefinition> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }

    for dir in workflow_search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(def) = serde_json::from_str::<FileWorkflowDefinition>(&raw) else {
                continue;
            };
            if def.name == want {
                return Some(def);
            }
        }
    }

    None
}

fn list_file_workflows() -> Vec<WorkflowInfo> {
    let mut out = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for dir in workflow_search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(def) = serde_json::from_str::<FileWorkflowDefinition>(&raw) else {
                continue;
            };
            if def.name.trim().is_empty() {
                continue;
            }
            if seen.contains_key(&def.name) {
                continue;
            }
            seen.insert(def.name.clone(), ());
            out.push(WorkflowInfo {
                name: def.name,
                version: def.version,
                description: if def.description.trim().is_empty() {
                    "Workflow loaded from JSON pack.".to_string()
                } else {
                    def.description
                },
            });
        }
    }

    out
}

fn workflow_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(plugin_dir) = env::var("RZN_PLUGIN_DIR") {
        let root = PathBuf::from(plugin_dir);
        dirs.push(root.join("resources").join("workflows"));
    }
    if let Ok(plugin_root) = env::var("CLAUDE_PLUGIN_ROOT") {
        let root = PathBuf::from(plugin_root);
        dirs.push(root.join("resources").join("workflows"));
    }

    // Dev fallback (repo root as cwd in claude_plugin/.mcp.json).
    dirs.push(PathBuf::from("crates/rzn_ios_tools_worker/resources/workflows"));

    if let Ok(extra) = env::var("RZN_IOS_WORKFLOW_DIRS") {
        for raw in extra.split(':') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            dirs.push(PathBuf::from(trimmed));
        }
    }

    dirs.into_iter()
        .filter(|dir| dir.exists() && dir.is_dir())
        .fold(Vec::new(), |mut dedup, entry| {
            if !dedup.contains(&entry) {
                dedup.push(entry);
            }
            dedup
        })
}

async fn run_safari_google_search(state: &AppState, args: &Value) -> Result<Value> {
    let query = args
        .get("args")
        .and_then(|v| v.get("query"))
        .or_else(|| args.get("query"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("workflow args.query is required"))?
        .to_string();

    let limit = args
        .get("args")
        .and_then(|v| v.get("limit"))
        .or_else(|| args.get("limit"))
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 20) as usize;

    let session = args.get("session").cloned().unwrap_or_else(|| json!({}));
    let udid = session
        .get("udid")
        .or_else(|| args.get("udid"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("session.udid is required"))?
        .to_string();

    let ensure_result = ensure_appium(
        state,
        EnsureOptions {
            port: None,
            log_level: None,
        },
    )
    .await
    .context("ios.appium.ensure failed")?;

    let driver = WebDriverClient::new(&ensure_result.base_url)?;

    if let Some(existing) = state.active_session().await {
        let _ = driver.delete_session(&existing.session_id).await;
        state.clear_session().await;
    }

    let wda_local_port = session
        .get("wdaLocalPort")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v > 0);

    let created = driver
        .create_session_safari(SessionCreateRequest {
            udid: udid.clone(),
            no_reset: true,
            new_command_timeout_sec: 60,
            session_create_timeout_ms: Some(
                session
                    .get("sessionCreateTimeoutMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(600_000),
            ),
            wda_local_port,
            wda_launch_timeout_ms: Some(
                session
                    .get("wdaLaunchTimeoutMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(240_000),
            ),
            wda_connection_timeout_ms: Some(
                session
                    .get("wdaConnectionTimeoutMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(120_000),
            ),
            language: None,
            locale: None,
            show_xcode_log: session.get("showXcodeLog").and_then(Value::as_bool),
            allow_provisioning_updates: session
                .get("allowProvisioningUpdates")
                .and_then(Value::as_bool),
            allow_provisioning_device_registration: session
                .get("allowProvisioningDeviceRegistration")
                .and_then(Value::as_bool),
            xcode_org_id: session
                .get("signing")
                .and_then(|v| v.get("xcodeOrgId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            xcode_signing_id: session
                .get("signing")
                .and_then(|v| v.get("xcodeSigningId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            updated_wda_bundle_id: session
                .get("signing")
                .and_then(|v| v.get("updatedWDABundleId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
        })
        .await
        .context("create Safari session failed")?;

    state
        .set_session(
            created.session_id.clone(),
            "safari_web".to_string(),
            udid.clone(),
            wda_local_port,
        )
        .await;

    let session_id = created.session_id;

    let result = async {
        driver
            .goto_url(&session_id, "https://www.google.com")
            .await
            .context("navigate to Google failed")?;

        maybe_dismiss_google_consent(&driver, &session_id).await;

        let search_selector = "textarea[name='q'], input[name='q']";
        let search_element_id = wait_for_first_selector(
            &driver,
            &session_id,
            &[search_selector],
            Duration::from_secs(20),
        )
        .await
        .context("Google search input did not appear")?;

        driver
            .click_element(&session_id, &search_element_id)
            .await
            .context("failed to focus search input")?;
        driver
            .clear_element(&session_id, &search_element_id)
            .await
            .context("failed to clear search input")?;
        driver
            .type_element(&session_id, &search_element_id, &query)
            .await
            .context("failed to type search query")?;
        let encoded_query = encode_query_component(&query);
        let search_url = format!("https://www.google.com/search?q={encoded_query}&hl=en");
        driver
            .goto_url(&session_id, &search_url)
            .await
            .context("failed to navigate to Google search URL")?;
        maybe_dismiss_google_consent(&driver, &session_id).await;

        let result_selectors = [
            "#search",
            "#rso",
            "a h3",
            "a div[role='heading']",
            "main a[href]",
        ];

        wait_for_first_selector(&driver, &session_id, &result_selectors, Duration::from_secs(25))
            .await
            .context("Google search results did not render in time")?;

        let extraction_script = r#"
            const max = Math.min(20, Math.max(1, Number(arguments[0] || 5)));
            const out = [];
            const seen = new Set();
            const candidates = document.querySelectorAll('a h3, a div[role="heading"], a[href] h3, a[href] span');
            for (const node of candidates) {
              const anchor = node.closest('a');
              if (!anchor) continue;
              const href = anchor.href || anchor.getAttribute('href') || '';
              const title = (node.innerText || node.textContent || '').trim();
              if (!href || !title) continue;
              if (!/^https?:\/\//i.test(href)) continue;
              if (/^https?:\/\/(www\.)?google\./i.test(href)) continue;
              if (seen.has(href)) continue;
              seen.add(href);
              let snippet = '';
              const card = anchor.closest('div.g, div.MjjYud, div.tF2Cxc, div[data-sokoban-container], div.yuRUbf') || anchor.parentElement;
              if (card) {
                const snippetNode = card.querySelector('.VwiC3b, .yXK7lf, [data-content-feature], span');
                if (snippetNode) snippet = (snippetNode.innerText || snippetNode.textContent || '').trim();
              }
              out.push({ title, url: href, snippet });
              if (out.length >= max) break;
            }
            return out;
        "#;

        let extraction_response = driver
            .execute_script(&session_id, extraction_script, json!([limit]))
            .await
            .context("failed to extract search results")?;
        let results = extraction_response
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let screenshot_b64 = driver
            .screenshot(&session_id)
            .await
            .context("failed to capture screenshot")?;

        let current_url = driver.get_current_url(&session_id).await.unwrap_or_default();
        let title = driver.get_title(&session_id).await.unwrap_or_default();

        Ok::<Value, anyhow::Error>(json!({
            "ok": true,
            "query": query,
            "limit": limit,
            "resultCount": results.len(),
            "results": results,
            "url": current_url,
            "title": title,
            "screenshot": {
                "mimeType": "image/png",
                "data": screenshot_b64,
            }
        }))
    }
    .await;

    result
}

fn encode_query_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("%{byte:02X}"));
            }
        }
    }
    out
}

async fn wait_for_first_selector(
    driver: &WebDriverClient,
    session_id: &str,
    selectors: &[&str],
    timeout: Duration,
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error: Option<anyhow::Error> = None;

    loop {
        for selector in selectors {
            match driver.find_elements_css(session_id, selector).await {
                Ok(ids) if !ids.is_empty() => return Ok(ids[0].clone()),
                Ok(_) => {}
                Err(err) => last_error = Some(err),
            }
        }

        if tokio::time::Instant::now() >= deadline {
            let message = last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "no matching elements found".to_string());
            bail!("wait_for_first_selector timed out: {message}");
        }

        tokio::time::sleep(Duration::from_millis(350)).await;
    }
}

async fn maybe_dismiss_google_consent(driver: &WebDriverClient, session_id: &str) {
    let script = r#"
      const labels = ['I agree', 'Accept all', 'Reject all', 'Agree'];
      const buttons = Array.from(document.querySelectorAll('button, input[type="button"], input[type="submit"]'));
      for (const b of buttons) {
        const text = (b.innerText || b.value || '').trim();
        if (labels.some(label => text.includes(label))) {
          b.click();
          return true;
        }
      }
      return false;
    "#;
    let _ = driver.execute_script(session_id, script, json!([])).await;
}
