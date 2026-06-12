use anyhow::{anyhow, bail, Context, Result};
use once_cell::sync::Lazy;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::str;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

use self::policy as tool_policy;
use crate::appium::{ensure_appium, parse_port_value, probe_webdriver_base, EnsureOptions};
use crate::errors::{ToolCallError, ToolErrorCode};
use crate::state::{AppState, AppiumSource};
use crate::ui_compact::{build_compact_snapshot, locator_to_json, NodeFilter};
use crate::webdriver::{SessionCreateRequest, WebDriverClient};
use crate::workflow_failure_report::{
    self, FailureArtifactPolicy, FlowFailureContext, FlowFailureReportDraft,
};
use crate::workflows;
use crate::xctrace;

pub mod action;
pub mod phone_data;
pub mod policy;
pub mod registry;
pub mod script;
pub mod session;
pub mod spec;
pub mod ui;
pub mod utility;
pub mod web;
pub mod workflow;

const DEFAULT_WDA_LOCAL_PORT: u16 = 8100;

pub use spec::list_tool_definitions;

pub async fn handle_tool_call(
    state: &AppState,
    tool_name: &str,
    arguments: Value,
) -> Result<Value> {
    tool_policy::enforce_direct_tool_policy(tool_name, &arguments)?;
    handle_tool_call_unchecked(state, tool_name, arguments).await
}

async fn handle_tool_call_unchecked(
    state: &AppState,
    tool_name: &str,
    arguments: Value,
) -> Result<Value> {
    match tool_name {
        "rzn.worker.health" => worker_health(state).await,
        "rzn.worker.shutdown" => worker_shutdown(state, &arguments).await,
        "ios.env.doctor" => env_doctor().await,
        "ios.device.list" => device_list(&arguments).await,
        "ios.device.status" => device_status(&arguments).await,
        "ios.appium.ensure" => appium_ensure(state, &arguments).await,
        "ios.session.create" => session_create(state, &arguments).await,
        "ios.session.delete" => session_delete(state, &arguments).await,
        "ios.session.info" => session_info(state).await,
        "ios.app.activate" => app_activate(state, &arguments).await,
        "ios.wda.shutdown" => wda_shutdown(state, &arguments).await,
        "ios.ui.source" => ui_source(state, &arguments).await,
        "ios.ui.screenshot" => ui_screenshot(state, &arguments).await,
        "ios.ui.observe_compact" => ui_observe_compact(state, &arguments).await,
        "ios.ui.extract_rows" => ui_extract_rows(state, &arguments).await,
        "ios.ui.extract_text" => ui_extract_text(state, &arguments).await,
        "ios.ui.find_row" => ui_find_row(state, &arguments).await,
        "ios.target.resolve" => target_resolve(state, &arguments).await,
        "ios.action.tap" => action_tap(state, &arguments).await,
        "ios.action.type" => action_type(state, &arguments).await,
        "ios.action.typeahead" => action_typeahead(state, &arguments).await,
        "ios.action.wait" => action_wait(state, &arguments).await,
        "ios.action.scroll" => action_scroll(state, &arguments).await,
        "ios.action.swipe" => action_swipe(state, &arguments).await,
        "ios.action.back" => action_back(state, &arguments).await,
        "ios.action.scroll_until" => action_scroll_until(state, &arguments).await,
        "ios.element.text" => element_text(state, &arguments).await,
        "ios.element.attribute" => element_attribute(state, &arguments).await,
        "ios.element.rect" => element_rect(state, &arguments).await,
        "ios.alert.text" => alert_text(state, &arguments).await,
        "ios.alert.accept" => alert_accept(state, &arguments).await,
        "ios.alert.dismiss" => alert_dismiss(state, &arguments).await,
        "ios.alert.wait" => alert_wait(state, &arguments).await,
        "ios.web.goto" => web_goto(state, &arguments).await,
        "ios.web.wait_css" => web_wait_css(state, &arguments).await,
        "ios.web.wait_js" => web_wait_js(state, &arguments).await,
        "ios.web.click_css" => web_click_css(state, &arguments).await,
        "ios.web.type_css" => web_type_css(state, &arguments).await,
        "ios.web.press_key" => web_press_key(state, &arguments).await,
        "ios.web.page_source" => web_page_source(state, &arguments).await,
        "ios.web.screenshot" => web_screenshot(state, &arguments).await,
        "ios.web.eval_js" => web_eval_js(state, &arguments).await,
        "ios.workflow.list" => workflow_list(&arguments).await,
        "ios.capability.list" => capability_list(&arguments).await,
        "ios.workflow.run" => workflow_run(state, &arguments).await,
        "rzn.workflow_failure_report.review" => workflow_failure_report_review(&arguments).await,
        "rzn.workflow_failure_report.submit" => workflow_failure_report_submit(&arguments).await,
        "rzn.workflow_failure_report.queue" => workflow_failure_report_queue(&arguments).await,
        "ios.script.run" => script_run(state, &arguments).await,
        "phone_messages.list_recent_threads" => {
            phone_messages_list_recent_threads(state, &arguments).await
        }
        "phone_messages.read_latest_messages" => {
            phone_messages_read_latest_messages(state, &arguments).await
        }
        "phone_messages.find_recent_otp" => phone_messages_find_recent_otp(state, &arguments).await,
        "phone_calls.list_recent_calls" => phone_calls_list_recent_calls(state, &arguments).await,
        "phone_notifications.list_recent_notifications" => {
            phone_notifications_list_recent_notifications(state, &arguments).await
        }
        "phone_notifications.filter_notifications_by_app" => {
            phone_notifications_filter_notifications_by_app(state, &arguments).await
        }
        "util.rank_by_name" => util_rank_by_name(&arguments).await,
        "util.list.length" => util_list_length(&arguments).await,
        "util.list.first" => util_list_first(&arguments).await,
        "util.list.nth" => util_list_nth(&arguments).await,
        "util.list.find" => util_list_find(&arguments).await,
        "util.rect.relative_point" => util_rect_relative_point(&arguments).await,
        "util.fail" => util_fail(&arguments).await,
        "util.sleep" => util_sleep(&arguments).await,
        "util.date.bucket_counts" => util_date_bucket_counts(&arguments).await,
        _ => bail!("unknown tool '{tool_name}'"),
    }
}

fn tool_success(structured: Value, message: &str) -> Value {
    json!({
        "content": [
            { "type": "text", "text": message }
        ],
        "structuredContent": structured
    })
}

fn tool_success_with_content(structured: Value, mut content: Vec<Value>) -> Value {
    if content.is_empty() {
        content.push(json!({"type": "text", "text": "ok"}));
    }
    json!({
        "content": content,
        "structuredContent": structured
    })
}

pub fn tool_error_result(message: &str, details: Value) -> Value {
    tool_error_result_with_code(message, None, details)
}

pub fn tool_error_result_with_code(
    message: &str,
    error_code: Option<&str>,
    details: Value,
) -> Value {
    json!({
        "isError": true,
        "content": [
            { "type": "text", "text": message }
        ],
        "structuredContent": {
            "ok": false,
            "error": message,
            "errorCode": error_code,
            "details": details
        }
    })
}

pub fn tool_error_from_anyhow(err: &anyhow::Error, tool: &str) -> Value {
    if let Some(typed) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<ToolCallError>())
    {
        return tool_error_result_with_code(
            &typed.message,
            Some(typed.code.as_str()),
            merge_error_details(tool, &typed.details),
        );
    }

    let message = format!("{err:#}");
    let lowered = message.to_lowercase();
    let code = if lowered.contains("timeout") {
        ToolErrorCode::Timeout
    } else if lowered.contains("device was not, or could not be, unlocked")
        || lowered.contains("could not be unlocked")
        || lowered.contains("bserrorcodedescription=locked")
        || lowered.contains(" for reason: locked")
    {
        ToolErrorCode::DeviceLocked
    } else if lowered.contains("no active session")
        || lowered.contains("sessionid is required")
        || lowered.contains("appium is not initialized")
    {
        ToolErrorCode::NoSession
    } else if lowered.contains("requires commit") {
        ToolErrorCode::CommitRequired
    } else if lowered.contains("policy") || lowered.contains("privacygate") {
        ToolErrorCode::PolicyDenied
    } else if lowered.contains("no elements found") || lowered.contains("no matching elements") {
        ToolErrorCode::ElementNotFound
    } else if lowered.contains("expected exactly one match")
        || lowered.contains("multiple matching elements")
        || lowered.contains("ambiguous")
    {
        ToolErrorCode::AmbiguousMatch
    } else if lowered.contains("required") || lowered.contains("invalid params") {
        ToolErrorCode::InvalidParams
    } else {
        ToolErrorCode::Internal
    };

    tool_error_result_with_code(&message, Some(code.as_str()), json!({ "tool": tool }))
}

fn merge_error_details(tool: &str, details: &Value) -> Value {
    let mut merged = serde_json::Map::new();
    merged.insert("tool".to_string(), json!(tool));

    if let Some(obj) = details.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    } else if !details.is_null() {
        merged.insert("details".to_string(), details.clone());
    }

    Value::Object(merged)
}

async fn worker_health(state: &AppState) -> Result<Value> {
    let _ = state.restore_persisted_runtime().await;
    let snapshot = state.snapshot().await;
    let appium_health = if let Some(base_url) = snapshot.appium_base_url.clone() {
        probe_webdriver_base(&base_url).await.is_ok()
    } else {
        false
    };

    let source = snapshot
        .appium_source
        .map(|src| match src {
            crate::state::AppiumSource::Env => "env",
            crate::state::AppiumSource::Spawned => "spawned",
        })
        .unwrap_or("none");

    Ok(tool_success(
        json!({
            "ok": true,
            "id": "rzn-phone/ios",
            "plugin_version": std::env::var("RZN_PLUGIN_VERSION").unwrap_or_else(|_| "dev".to_string()),
            "mcp_protocol_version": "2025-06-18",
            "ready": true,
            "appium": {
                "running": appium_health,
                "baseUrl": snapshot.appium_base_url,
                "pid": snapshot.appium_pid,
                "source": source
            },
            "active_session": snapshot.session
        }),
        "worker healthy",
    ))
}

async fn perform_post_run_device_actions(
    state: &AppState,
    background_app: bool,
    lock_device: bool,
) -> Value {
    let mut background_ok: Option<bool> = None;
    let mut lock_ok: Option<bool> = None;
    let mut errors: Vec<String> = Vec::new();

    if !background_app && !lock_device {
        return json!({
            "backgroundAppRequested": false,
            "backgroundAppOk": Value::Null,
            "lockDeviceRequested": false,
            "lockDeviceOk": Value::Null,
            "errors": []
        });
    }

    let Some(session) = state.active_session().await else {
        if background_app {
            background_ok = Some(false);
        }
        if lock_device {
            lock_ok = Some(false);
        }
        errors.push("no active session for post-run device actions".to_string());
        return json!({
            "backgroundAppRequested": background_app,
            "backgroundAppOk": background_ok,
            "lockDeviceRequested": lock_device,
            "lockDeviceOk": lock_ok,
            "errors": errors
        });
    };

    let driver = match driver_from_state(state).await {
        Ok(driver) => driver,
        Err(err) => {
            if background_app {
                background_ok = Some(false);
            }
            if lock_device {
                lock_ok = Some(false);
            }
            errors.push(format!(
                "driver unavailable for post-run device actions: {err:#}"
            ));
            return json!({
                "backgroundAppRequested": background_app,
                "backgroundAppOk": background_ok,
                "lockDeviceRequested": lock_device,
                "lockDeviceOk": lock_ok,
                "errors": errors
            });
        }
    };

    if background_app {
        match driver
            .execute_script(
                &session.session_id,
                "mobile: pressButton",
                json!([{ "name": "home" }]),
            )
            .await
        {
            Ok(_) => background_ok = Some(true),
            Err(err) => {
                background_ok = Some(false);
                errors.push(format!("failed to background app via Home button: {err:#}"));
            }
        }
    }

    if lock_device {
        match driver
            .execute_script(&session.session_id, "mobile: lock", json!([]))
            .await
        {
            Ok(_) => lock_ok = Some(true),
            Err(err) => {
                lock_ok = Some(false);
                errors.push(format!("failed to lock device: {err:#}"));
            }
        }
    }

    json!({
        "backgroundAppRequested": background_app,
        "backgroundAppOk": background_ok,
        "lockDeviceRequested": lock_device,
        "lockDeviceOk": lock_ok,
        "errors": errors
    })
}

async fn worker_shutdown(state: &AppState, arguments: &Value) -> Result<Value> {
    let _ = state.restore_persisted_runtime().await;
    let stop_appium = arguments
        .get("stopAppium")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let shutdown_wda = arguments
        .get("shutdownWDA")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let background_app = arguments
        .get("backgroundApp")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lock_device = arguments
        .get("lockDevice")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let snapshot = state.snapshot().await;
    let wda_port = state
        .last_wda_local_port()
        .await
        .unwrap_or(DEFAULT_WDA_LOCAL_PORT);
    let post_run_actions =
        perform_post_run_device_actions(state, background_app, lock_device).await;

    let mut closed_session = false;
    let mut stopped_env_appium = false;
    let mut wda_shutdown_ok = false;

    if let Some(active) = state.active_session().await {
        if let Some(base_url) = snapshot.appium_base_url.clone() {
            let driver = WebDriverClient::new(&base_url)?;
            let _ = driver.delete_session(&active.session_id).await;
        }
        state.clear_session().await;
        closed_session = true;
    }

    if shutdown_wda {
        wda_shutdown_ok = shutdown_wda_on_port(wda_port).await.unwrap_or(false);
    }

    if stop_appium {
        match snapshot.appium_source {
            Some(AppiumSource::Spawned) => {
                state.shutdown_spawned_appium().await;
            }
            Some(AppiumSource::Env) => {
                state.clear_appium_metadata().await;
                stopped_env_appium = true;
            }
            None => {}
        }
    } else {
        state.clear_session().await;
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "stopAppium": stop_appium,
            "shutdownWDA": shutdown_wda,
            "wdaLocalPort": wda_port,
            "wdaShutdownOk": wda_shutdown_ok,
            "closedSession": closed_session,
            "stoppedEnvAppium": stopped_env_appium,
            "postRunActions": post_run_actions
        }),
        "shutdown complete",
    ))
}

async fn kill_wda_build_processes_for_udid(udid: &str) -> bool {
    let trimmed = udid.trim();
    if trimmed.is_empty() {
        return false;
    }

    let patterns = [
        format!("xcodebuild build-for-testing test-without-building .*id={trimmed}"),
        format!("WebDriverAgentRunner.*{trimmed}"),
    ];

    for pattern in &patterns {
        let _ = Command::new("pkill")
            .args(["-TERM", "-f", pattern])
            .status()
            .await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    for pattern in &patterns {
        let _ = Command::new("pkill")
            .args(["-KILL", "-f", pattern])
            .status()
            .await;
    }

    true
}

async fn cleanup_failed_session_create(
    state: &AppState,
    appium_source: AppiumSource,
    _appium_base_url: &str,
    wda_port: u16,
    udid: &str,
) {
    let _ = shutdown_wda_on_port(wda_port).await;
    match appium_source {
        AppiumSource::Spawned => state.shutdown_spawned_appium().await,
        AppiumSource::Env => {
            state.clear_appium_metadata().await;
            state.clear_session().await;
        }
    }
    let _ = kill_wda_build_processes_for_udid(udid).await;
}

async fn shutdown_wda_on_port(port: u16) -> Result<bool> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("build HTTP client")?;

    let urls = [
        format!("http://127.0.0.1:{port}/wda/shutdown"),
        format!("http://localhost:{port}/wda/shutdown"),
        format!("http://[::1]:{port}/wda/shutdown"),
    ];

    for url in urls {
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };

        if response.status().is_success() {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn env_doctor() -> Result<Value> {
    let mut checks = Vec::new();

    checks.push(run_check("xcodebuild", "xcodebuild", &["-version"], None).await);
    checks.push(run_check("xctrace", "xcrun", &["xctrace", "list", "devices"], None).await);
    checks.push(run_check("node", "node", &["--version"], None).await);
    checks.push(run_check("appium", "appium", &["--version"], None).await);
    checks.push(
        run_check(
            "appium_xcuitest_driver",
            "appium",
            &["driver", "list", "--installed"],
            Some("xcuitest"),
        )
        .await,
    );

    let ok = checks
        .iter()
        .all(|entry| entry.get("ok") == Some(&Value::Bool(true)));

    Ok(tool_success(
        json!({
            "ok": ok,
            "checks": checks,
            "remediation": [
                "Install Node.js LTS and ensure it is available to GUI-launched apps.",
                "Install Appium: npm i -g appium",
                "Install XCUITest driver: appium driver install xcuitest",
                "Prefer setting RZN_IOS_APPIUM_URL for desktop runtime stability."
            ]
        }),
        if ok {
            "environment looks good"
        } else {
            "environment has missing prerequisites"
        },
    ))
}

async fn run_check(
    name: &str,
    command: &str,
    args: &[&str],
    output_must_contain: Option<&str>,
) -> Value {
    let output = Command::new(command).args(args).output().await;
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let mut ok = output.status.success();
            if let Some(fragment) = output_must_contain {
                let haystack = format!("{stdout}\n{stderr}").to_lowercase();
                ok = ok && haystack.contains(&fragment.to_lowercase());
            }
            json!({
                "name": name,
                "ok": ok,
                "exitCode": output.status.code(),
                "stdout": stdout,
                "stderr": stderr
            })
        }
        Err(err) => json!({
            "name": name,
            "ok": false,
            "error": err.to_string()
        }),
    }
}

async fn device_list(arguments: &Value) -> Result<Value> {
    let include_simulators = arguments
        .get("includeSimulators")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let devices = xctrace::list_devices(include_simulators).await?;

    Ok(tool_success(
        json!({ "devices": devices }),
        "device list complete",
    ))
}

async fn device_status(arguments: &Value) -> Result<Value> {
    let udid = required_str(arguments, "udid")?;
    let probe = xctrace::probe_device(udid).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "device": probe
        }),
        "device status captured",
    ))
}

async fn appium_ensure(state: &AppState, arguments: &Value) -> Result<Value> {
    let port = parse_port_value(arguments.get("port"), "port")?;
    let log_level = arguments
        .get("logLevel")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let result = ensure_appium(state, EnsureOptions { port, log_level }).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "baseUrl": result.base_url,
            "source": result.source,
            "pid": result.pid
        }),
        "appium ready",
    ))
}

async fn session_create(state: &AppState, arguments: &Value) -> Result<Value> {
    let _ = state.restore_persisted_runtime().await;
    let udid = required_str(arguments, "udid")?.to_string();
    let kind = arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("safari_web");
    if kind != "safari_web" && kind != "native_app" {
        bail!("unsupported session kind '{kind}'");
    }
    let requested_bundle_id = if kind == "native_app" {
        Some(required_str(arguments, "bundleId")?.to_string())
    } else {
        None
    };

    let reuse_active_session = arguments
        .get("reuseActiveSession")
        .and_then(Value::as_bool)
        .or_else(|| {
            std::env::var("RZN_IOS_REUSE_ACTIVE_SESSION")
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        })
        .unwrap_or(false);

    let replace_existing = arguments
        .get("replaceExisting")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let device_probe = xctrace::probe_device(&udid).await?;
    if device_probe.state != "available" && device_probe.state != "simulator" {
        return Err(ToolCallError::new(
            ToolErrorCode::ActionFailed,
            format!(
                "device transport unavailable: xctrace reports {} for {}",
                device_probe.state, udid
            ),
            json!({
                "udid": udid,
                "deviceState": device_probe.state,
                "matchedSection": device_probe.matched_section,
                "matchedLine": device_probe.matched_line
            }),
        )
        .into());
    }

    let ensure_result = ensure_appium(
        state,
        EnsureOptions {
            port: None,
            log_level: None,
        },
    )
    .await?;

    let driver = WebDriverClient::new(&ensure_result.base_url)?;

    if reuse_active_session {
        if let Some(existing) = state.active_session().await {
            if session_matches_request(&existing, &udid, kind, requested_bundle_id.as_deref()) {
                if session_is_alive(&driver, &existing.session_id).await {
                    state.touch_runtime().await;
                    return Ok(tool_success(
                        json!({
                            "ok": true,
                            "sessionId": existing.session_id,
                            "kind": existing.kind,
                            "bundleId": existing.bundle_id,
                            "appiumBaseUrl": ensure_result.base_url,
                            "reused": true
                        }),
                        "session reused",
                    ));
                }

                let _ = driver.delete_session(&existing.session_id).await;
                state.clear_session().await;
            }
        }
    }

    if replace_existing {
        if let Some(existing) = state.active_session().await {
            let _ = driver.delete_session(&existing.session_id).await;
            state.clear_session().await;
        }
    }

    let signing = arguments
        .get("signing")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let wda_local_port = parse_port_value(arguments.get("wdaLocalPort"), "wdaLocalPort")?;

    let session_create_timeout_ms = arguments
        .get("sessionCreateTimeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(600_000);

    let request = SessionCreateRequest {
        udid: udid.clone(),
        no_reset: arguments
            .get("noReset")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        new_command_timeout_sec: arguments
            .get("newCommandTimeoutSec")
            .and_then(Value::as_u64)
            .unwrap_or(60),
        session_create_timeout_ms: Some(session_create_timeout_ms),
        wda_local_port,
        wda_launch_timeout_ms: Some(
            arguments
                .get("wdaLaunchTimeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(240_000),
        ),
        wda_connection_timeout_ms: Some(
            arguments
                .get("wdaConnectionTimeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(120_000),
        ),
        show_xcode_log: arguments.get("showXcodeLog").and_then(Value::as_bool),
        allow_provisioning_updates: arguments
            .get("allowProvisioningUpdates")
            .and_then(Value::as_bool),
        allow_provisioning_device_registration: arguments
            .get("allowProvisioningDeviceRegistration")
            .and_then(Value::as_bool),
        language: arguments
            .get("language")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        locale: arguments
            .get("locale")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        xcode_org_id: signing
            .get("xcodeOrgId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        xcode_signing_id: signing
            .get("xcodeSigningId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        updated_wda_bundle_id: signing
            .get("updatedWDABundleId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    };

    let create_deadline = Duration::from_millis(session_create_timeout_ms.saturating_add(15_000));
    let created = match tokio::time::timeout(create_deadline, async {
        match kind {
            "safari_web" => driver.create_session_safari(request).await,
            "native_app" => {
                driver
                    .create_session_native_app(
                        request,
                        requested_bundle_id
                            .clone()
                            .expect("bundle id required for native_app"),
                    )
                    .await
            }
            _ => unreachable!(),
        }
    })
    .await
    {
        Ok(Ok(created)) => created,
        Ok(Err(err)) => {
            cleanup_failed_session_create(
                state,
                if ensure_result.source == "spawned" {
                    AppiumSource::Spawned
                } else {
                    AppiumSource::Env
                },
                &ensure_result.base_url,
                wda_local_port.unwrap_or(DEFAULT_WDA_LOCAL_PORT),
                &udid,
            )
            .await;
            return Err(err.context(format!("failed to create {} session", kind)));
        }
        Err(_) => {
            cleanup_failed_session_create(
                state,
                if ensure_result.source == "spawned" {
                    AppiumSource::Spawned
                } else {
                    AppiumSource::Env
                },
                &ensure_result.base_url,
                wda_local_port.unwrap_or(DEFAULT_WDA_LOCAL_PORT),
                &udid,
            )
            .await;
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                format!("timed out creating {kind} session"),
                json!({
                    "udid": udid,
                    "kind": kind,
                    "bundleId": requested_bundle_id,
                    "sessionCreateTimeoutMs": session_create_timeout_ms
                }),
            )
            .into());
        }
    };

    state
        .set_session(
            created.session_id.clone(),
            kind.to_string(),
            udid,
            requested_bundle_id.clone(),
            wda_local_port,
        )
        .await;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": created.session_id,
            "kind": kind,
            "bundleId": requested_bundle_id,
            "appiumBaseUrl": ensure_result.base_url,
            "capabilities": created.capabilities
        }),
        "session created",
    ))
}

async fn ui_source(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let source = fetch_native_ui_source(&driver, &session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "length": source.len(),
            "source": source
        }),
        "ui source captured",
    ))
}

async fn ui_screenshot(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let data = driver.screenshot(&session_id).await?;

    Ok(tool_success_with_content(
        json!({
            "ok": true,
            "sessionId": session_id,
            "mimeType": "image/png",
            "bytesBase64": data.len(),
            "data": data
        }),
        vec![
            json!({"type": "text", "text": "screenshot captured"}),
            json!({"type": "image", "mimeType": "image/png", "data": data}),
        ],
    ))
}

fn is_retryable_native_source_error(err: &anyhow::Error) -> bool {
    let message = format!("{err:#}").to_lowercase();
    message.contains("socket hang up")
        || message.contains("operation timed out")
        || message.contains("timed out")
        || message.contains("could not proxy command to the remote server")
        || message.contains("connection refused")
        || message.contains("connection reset by peer")
        || message.contains("remote server")
}

async fn fetch_native_ui_source(driver: &WebDriverClient, session_id: &str) -> Result<String> {
    let delays_ms = [0_u64, 600, 1200];
    let mut last_err: Option<anyhow::Error> = None;

    for (attempt_idx, delay_ms) in delays_ms.iter().enumerate() {
        if *delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
        }

        match driver.page_source(session_id).await {
            Ok(source) => return Ok(source),
            Err(err) => {
                let retryable = is_retryable_native_source_error(&err);
                if !retryable || attempt_idx + 1 == delays_ms.len() {
                    return Err(err);
                }
                last_err = Some(err);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("failed to fetch native UI source")))
}

async fn ui_observe_compact(state: &AppState, arguments: &Value) -> Result<Value> {
    let session = state
        .active_session()
        .await
        .ok_or_else(|| anyhow!("no active session; call ios.session.create first"))?;
    if session.kind != "native_app" {
        bail!(
            "ios.ui.observe_compact requires a native_app session (current kind={})",
            session.kind
        );
    }

    let session_id = resolve_session_id(state, arguments).await?;
    if session_id != session.session_id {
        bail!("unknown sessionId (this worker supports a single active session)");
    }

    let filter = arguments
        .get("nodeFilter")
        .and_then(Value::as_str)
        .map(NodeFilter::from_filter_name)
        .unwrap_or(NodeFilter::Interactive);
    let max_nodes = arguments
        .get("maxNodes")
        .and_then(Value::as_u64)
        .unwrap_or(140)
        .clamp(10, 500) as usize;

    let driver = driver_from_state(state).await?;
    let source = fetch_native_ui_source(&driver, &session_id).await?;
    let snapshot = build_compact_snapshot(&source, filter, max_nodes)
        .context("failed to build compact snapshot (is this native XML source?)")?;

    let snapshot_id = snapshot.snapshot_id.clone();
    state
        .set_compact_observation(snapshot_id.clone(), session_id.clone(), snapshot.targets)
        .await;

    let nodes_json = serde_json::to_value(&snapshot.nodes).unwrap_or_else(|_| json!([]));

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "snapshotId": snapshot_id,
            "nodes": nodes_json,
            "stats": snapshot.stats
        }),
        "compact snapshot captured",
    ))
}

#[derive(Debug, Clone)]
struct StringMatchQuery {
    contains: Option<String>,
    not_contains: Option<String>,
    regex: Option<Regex>,
    case_sensitive: bool,
}

fn parse_string_match_query(value: &Value, context: &str) -> Result<StringMatchQuery> {
    let contains = value
        .get("contains")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let not_contains = value
        .get("notContains")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let regex_raw = value
        .get("regex")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    if contains.is_none() && not_contains.is_none() && regex_raw.is_none() {
        bail!("{context} requires at least one of contains, notContains, or regex");
    }

    let case_sensitive = value
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let regex = if let Some(pattern) = regex_raw {
        let compiled = if case_sensitive {
            pattern
        } else {
            format!("(?i){pattern}")
        };
        Some(Regex::new(&compiled).with_context(|| format!("invalid regex for {context}"))?)
    } else {
        None
    };

    Ok(StringMatchQuery {
        contains: contains.map(|value| {
            if case_sensitive {
                value
            } else {
                value.to_lowercase()
            }
        }),
        not_contains: not_contains.map(|value| {
            if case_sensitive {
                value
            } else {
                value.to_lowercase()
            }
        }),
        regex,
        case_sensitive,
    })
}

fn matches_string_query(candidate: &str, query: &StringMatchQuery) -> bool {
    let haystack = if query.case_sensitive {
        candidate.to_string()
    } else {
        candidate.to_lowercase()
    };
    if let Some(needle) = query.contains.as_ref() {
        if !haystack.contains(needle) {
            return false;
        }
    }
    if let Some(needle) = query.not_contains.as_ref() {
        if haystack.contains(needle) {
            return false;
        }
    }
    if let Some(regex) = query.regex.as_ref() {
        if !regex.is_match(candidate) {
            return false;
        }
    }
    true
}

fn parse_scroll_settings(arguments: &Value) -> (String, f64, u64) {
    if let Some(scroll) = arguments.get("scroll").and_then(Value::as_object) {
        let direction = scroll
            .get("direction")
            .and_then(Value::as_str)
            .unwrap_or("down")
            .to_lowercase();
        let distance = scroll
            .get("distance")
            .and_then(Value::as_f64)
            .unwrap_or(0.6)
            .clamp(0.1, 0.95);
        let settle_ms = scroll
            .get("settleMs")
            .and_then(Value::as_u64)
            .unwrap_or(350)
            .clamp(0, 10_000);
        (direction, distance, settle_ms)
    } else {
        ("down".to_string(), 0.6, 350)
    }
}

fn sort_rows(rows: &mut [RowMatch], order: &str) {
    if order == "x" {
        rows.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        rows.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    }
}

fn row_match_to_value(row: &RowMatch, position: usize) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("position".to_string(), json!(position));
    obj.insert("x".to_string(), json!(row.x));
    obj.insert("y".to_string(), json!(row.y));
    obj.insert("width".to_string(), json!(row.width));
    obj.insert("height".to_string(), json!(row.height));
    obj.insert("centerX".to_string(), json!(row.x + (row.width / 2.0)));
    obj.insert("centerY".to_string(), json!(row.y + (row.height / 2.0)));
    obj.insert(
        "tapX".to_string(),
        json!(preferred_row_tap_x(row.x, row.width)),
    );
    obj.insert(
        "tapY".to_string(),
        json!(preferred_row_tap_y(row.y, row.height)),
    );
    for (k, v) in &row.fields {
        obj.insert(k.clone(), json!(v));
    }
    for (k, v) in &row.extra_fields {
        obj.insert(k.clone(), json!(v));
    }
    if let Some(tag_field) = &row.tag_field {
        obj.insert(
            tag_field.clone(),
            json!(row.tag_value.clone().unwrap_or_default()),
        );
    }
    obj.insert("rawLabel".to_string(), json!(row.raw_label));
    Value::Object(obj)
}

fn row_field_value(row: &RowMatch, field: Option<&str>) -> Option<String> {
    let Some(field) = field.map(str::trim).filter(|value| !value.is_empty()) else {
        return Some(row.raw_label.clone());
    };
    if field.eq_ignore_ascii_case("rawLabel") {
        return Some(row.raw_label.clone());
    }
    if let Some((_, value)) = row
        .fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(field))
    {
        return Some(value.clone());
    }
    if let Some((_, value)) = row
        .extra_fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(field))
    {
        return Some(value.clone());
    }
    if row
        .tag_field
        .as_deref()
        .map(|name| name.eq_ignore_ascii_case(field))
        .unwrap_or(false)
    {
        return row.tag_value.clone();
    }
    None
}

fn normalize_string_dedupe_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn find_matching_row_in_rows(
    rows: &[RowMatch],
    match_field: &str,
    match_query: &StringMatchQuery,
    dedupe_matches: bool,
    seen_matches: &mut HashSet<String>,
    matched_count: &mut usize,
    match_index: usize,
) -> Option<(usize, String, RowMatch)> {
    for (row_idx, row) in rows.iter().enumerate() {
        let Some(candidate) = row_field_value(row, Some(match_field)) else {
            continue;
        };
        if !matches_string_query(&candidate, match_query) {
            continue;
        }
        if dedupe_matches {
            let dedupe_key = normalize_string_dedupe_key(&candidate);
            if dedupe_key.is_empty() || !seen_matches.insert(dedupe_key) {
                continue;
            }
        }
        if *matched_count < match_index {
            *matched_count += 1;
            continue;
        }
        return Some((row_idx, candidate, row.clone()));
    }
    None
}

async fn ui_extract_rows(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    let source_override = arguments
        .get("source")
        .and_then(Value::as_str)
        .map(|raw| raw.to_string());

    let row_query = parse_row_query(arguments.get("row"))?;
    let primary_query = parse_primary_query(arguments.get("primary"))?;
    let tag_query = parse_tag_query(arguments.get("tag"));
    let field_queries = parse_field_queries(arguments.get("fields"))?;
    let split_cfg = parse_split_config(arguments.get("split"));
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0);
    let order = arguments
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("y")
        .to_lowercase();
    let max_scrolls = arguments
        .get("maxScrolls")
        .and_then(Value::as_u64)
        .or_else(|| arguments.get("max_scrolls").and_then(Value::as_u64))
        .unwrap_or(0)
        .clamp(0, 50) as u32;
    if source_override.is_some() && max_scrolls > 0 {
        bail!("source cannot be combined with maxScrolls");
    }

    let (scroll_direction, scroll_distance, settle_ms) = parse_scroll_settings(arguments);

    let mut rows_out: Vec<RowMatch> = Vec::new();
    let mut seen = HashSet::<String>::new();
    let mut scrolls_done: u32 = 0;

    for pass in 0..=max_scrolls {
        let source = if let Some(raw) = source_override.as_ref() {
            raw.clone()
        } else {
            fetch_native_ui_source(&driver, &session_id).await?
        };
        let mut rows = extract_rows_from_source(
            &source,
            &row_query,
            &primary_query,
            tag_query.as_ref(),
            &field_queries,
            &split_cfg,
        );

        sort_rows(&mut rows, &order);

        for row in rows {
            let key = normalize_match_key(&row.raw_label);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            rows_out.push(row);
            if let Some(max) = limit {
                if rows_out.len() >= max {
                    break;
                }
            }
        }

        if let Some(max) = limit {
            if rows_out.len() >= max {
                break;
            }
        }
        if source_override.is_some() {
            break;
        }
        if pass < max_scrolls {
            perform_scroll_gesture(&driver, &session_id, &scroll_direction, scroll_distance)
                .await?;
            scrolls_done += 1;
            if settle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(settle_ms)).await;
            }
        }
    }

    let output_rows: Vec<Value> = rows_out
        .iter()
        .enumerate()
        .map(|(idx, row)| row_match_to_value(row, idx + 1))
        .collect();

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "rowCount": output_rows.len(),
            "rows": output_rows,
            "scrolls": scrolls_done
        }),
        "rows extracted",
    ))
}

async fn ui_find_row(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    let source_override = arguments
        .get("source")
        .and_then(Value::as_str)
        .map(|raw| raw.to_string());

    let row_query = parse_row_query(arguments.get("row"))?;
    let primary_query = parse_primary_query(arguments.get("primary"))?;
    let tag_query = parse_tag_query(arguments.get("tag"));
    let field_queries = parse_field_queries(arguments.get("fields"))?;
    let split_cfg = parse_split_config(arguments.get("split"));
    let match_value = arguments
        .get("match")
        .ok_or_else(|| anyhow!("match is required"))?;
    let match_field = match_value
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("rawLabel")
        .to_string();
    let match_query = parse_string_match_query(match_value, "ios.ui.find_row.match")?;
    let match_index = arguments
        .get("matchIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let dedupe_matches = arguments
        .get("dedupeMatches")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let order = arguments
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("y")
        .to_lowercase();
    let include_source = arguments
        .get("includeSource")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_scrolls = arguments
        .get("maxScrolls")
        .and_then(Value::as_u64)
        .or_else(|| arguments.get("max_scrolls").and_then(Value::as_u64))
        .unwrap_or(0)
        .clamp(0, 50) as u32;
    if source_override.is_some() && max_scrolls > 0 {
        bail!("source cannot be combined with maxScrolls");
    }

    let (scroll_direction, scroll_distance, settle_ms) = parse_scroll_settings(arguments);
    let mut seen_matches = HashSet::<String>::new();
    let mut matched_count: usize = 0;
    let mut scrolls_done: u32 = 0;
    let mut visible_row_count: usize = 0;
    let mut last_source: Option<String> = None;

    for pass in 0..=max_scrolls {
        let source = if let Some(raw) = source_override.as_ref() {
            raw.clone()
        } else {
            fetch_native_ui_source(&driver, &session_id).await?
        };
        if include_source || pass == max_scrolls {
            last_source = Some(source.clone());
        }

        let mut rows = extract_rows_from_source(
            &source,
            &row_query,
            &primary_query,
            tag_query.as_ref(),
            &field_queries,
            &split_cfg,
        );
        sort_rows(&mut rows, &order);
        visible_row_count = rows.len();

        if let Some((row_idx, candidate, row)) = find_matching_row_in_rows(
            &rows,
            &match_field,
            &match_query,
            dedupe_matches,
            &mut seen_matches,
            &mut matched_count,
            match_index,
        ) {
            let mut out = json!({
                "ok": true,
                "sessionId": session_id,
                "found": true,
                "index": matched_count + 1,
                "zeroBasedIndex": matched_count,
                "matchedText": candidate,
                "value": row_match_to_value(&row, row_idx + 1),
                "matchField": match_field,
                "visibleRowCount": visible_row_count,
                "pass": pass,
                "scrolls": scrolls_done
            });
            if include_source {
                if let Some(obj) = out.as_object_mut() {
                    obj.insert(
                        "source".to_string(),
                        json!({
                            "length": source.len(),
                            "source": source
                        }),
                    );
                }
            }
            return Ok(tool_success(out, "matching row selected"));
        }

        if source_override.is_some() {
            break;
        }
        if pass < max_scrolls {
            perform_scroll_gesture(&driver, &session_id, &scroll_direction, scroll_distance)
                .await?;
            scrolls_done += 1;
            if settle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(settle_ms)).await;
            }
        }
    }

    let mut out = json!({
        "ok": true,
        "sessionId": session_id,
        "found": false,
        "index": Value::Null,
        "zeroBasedIndex": Value::Null,
        "matchedText": Value::Null,
        "value": Value::Null,
        "matchField": match_field,
        "visibleRowCount": visible_row_count,
        "matchedCount": matched_count,
        "scrolls": scrolls_done
    });
    if include_source {
        if let Some(source) = last_source {
            if let Some(obj) = out.as_object_mut() {
                obj.insert(
                    "source".to_string(),
                    json!({
                        "length": source.len(),
                        "source": source
                    }),
                );
            }
        }
    }

    Ok(tool_success(out, "no matching row found"))
}

async fn ui_extract_text(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    let source = if let Some(raw) = arguments.get("source").and_then(Value::as_str) {
        raw.to_string()
    } else {
        fetch_native_ui_source(&driver, &session_id).await?
    };

    let query = parse_node_query(arguments.get("query"));
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0)
        .unwrap_or(50);
    let unique = arguments
        .get("unique")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let order = arguments
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("y")
        .to_lowercase();

    let mut nodes = extract_nodes_from_source(&source, &query);
    if order == "x" {
        nodes.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        nodes.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    }

    let mut out = Vec::new();
    let mut seen = HashSet::<String>::new();
    for node in nodes {
        if unique {
            let key = normalize_match_key(&node.text);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
        }
        out.push(json!({
            "position": out.len() + 1,
            "text": node.text,
            "x": node.x,
            "y": node.y
        }));
        if out.len() >= limit {
            break;
        }
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "count": out.len(),
            "texts": out
        }),
        "texts extracted",
    ))
}

async fn target_resolve(state: &AppState, arguments: &Value) -> Result<Value> {
    let encoded_id = required_str(arguments, "encodedId")?;
    let snapshot_id = arguments.get("snapshotId").and_then(Value::as_str);

    let current_snapshot = state
        .compact_snapshot_id()
        .await
        .unwrap_or_else(|| "<none>".to_string());

    let locator = state
        .resolve_compact_target(snapshot_id, encoded_id)
        .await
        .ok_or_else(|| {
            anyhow!(
                "unable to resolve encodedId '{encoded_id}'. Re-run ios.ui.observe_compact (current snapshotId={current_snapshot})."
            )
        })?;

    Ok(tool_success(
        json!({
            "ok": true,
            "encodedId": encoded_id,
            "targetSpec": locator_to_json(&locator)
        }),
        "target resolved",
    ))
}

#[derive(Debug, Clone)]
struct ResolvedTarget {
    using: String,
    value: String,
    index: usize,
    require_unique: bool,
}

async fn resolve_target(state: &AppState, arguments: &Value) -> Result<Option<ResolvedTarget>> {
    if let Some(point) = arguments.get("point") {
        if point.get("x").is_some() || point.get("y").is_some() {
            return Ok(None);
        }
    }

    let Some(target) = arguments.get("target").and_then(Value::as_object) else {
        return Err(ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required (or provide point)",
            json!({}),
        )
        .into());
    };

    let index = match target.get("index") {
        Some(value) => {
            if let Some(u) = value.as_u64() {
                u as usize
            } else if let Some(i) = value.as_i64() {
                if i < 0 {
                    return Err(ToolCallError::new(
                        ToolErrorCode::InvalidParams,
                        "target.index must be >= 0",
                        json!({"index": i}),
                    )
                    .into());
                }
                i as usize
            } else {
                return Err(ToolCallError::new(
                    ToolErrorCode::InvalidParams,
                    "target.index must be an integer",
                    json!({"index": value}),
                )
                .into());
            }
        }
        None => 0,
    };

    let explicit_require_unique = target
        .get("requireUnique")
        .and_then(Value::as_bool)
        .or_else(|| target.get("require_unique").and_then(Value::as_bool));

    if let Some(encoded) = target
        .get("encodedId")
        .and_then(Value::as_str)
        .map(str::trim)
    {
        if !encoded.is_empty() {
            let snapshot_id = target.get("snapshotId").and_then(Value::as_str);
            let locator = state
                .resolve_compact_target(snapshot_id, encoded)
                .await
                .ok_or_else(|| {
                    anyhow!("encodedId '{encoded}' not found. Call ios.ui.observe_compact first.")
                })?;
            return Ok(Some(ResolvedTarget {
                using: locator.using,
                value: locator.value,
                index,
                require_unique: explicit_require_unique.unwrap_or(true),
            }));
        }
    }

    if let (Some(using), Some(value)) = (
        target.get("using").and_then(Value::as_str).map(str::trim),
        target.get("value").and_then(Value::as_str).map(str::trim),
    ) {
        if using.is_empty() || value.is_empty() {
            return Err(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                "target.using and target.value must be non-empty",
                json!({ "using": using, "value": value }),
            )
            .into());
        }
        return Ok(Some(ResolvedTarget {
            using: using.to_string(),
            value: value.to_string(),
            index,
            require_unique: explicit_require_unique.unwrap_or(false),
        }));
    }

    Err(ToolCallError::new(
        ToolErrorCode::InvalidParams,
        "target must include either encodedId or using/value (or provide point)",
        json!({}),
    )
    .into())
}

async fn action_tap(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    if let Some(point) = arguments.get("point") {
        let x = point
            .get("x")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("point.x must be a number"))?;
        let y = point
            .get("y")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("point.y must be a number"))?;
        let allow_out_of_bounds = arguments
            .get("allowOutOfBounds")
            .and_then(Value::as_bool)
            .or_else(|| point.get("allowOutOfBounds").and_then(Value::as_bool))
            .unwrap_or(false);
        driver
            .tap_point_with_options(&session_id, x, y, allow_out_of_bounds)
            .await?;
        return Ok(tool_success(
            json!({"ok": true, "sessionId": session_id, "point": {"x": x, "y": y}}),
            "tap complete",
        ));
    }

    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no elements found for locator using='{}' value='{}'",
                &resolved.using, &resolved.value
            ),
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for using='{}' value='{}', got {}",
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }

    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {} for locator using='{}' value='{}' (found {})",
                resolved.index,
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;
    driver.click_element(&session_id, element_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
        }),
        "tap complete",
    ))
}

async fn action_type(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let text = required_str(arguments, "text")?;
    let clear_first = arguments
        .get("clearFirst")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let press_enter = arguments
        .get("pressEnter")
        .and_then(Value::as_bool)
        .or_else(|| arguments.get("press_enter").and_then(Value::as_bool))
        .unwrap_or(false);

    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;
    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no elements found for locator using='{}' value='{}'",
                &resolved.using, &resolved.value
            ),
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for using='{}' value='{}', got {}",
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }

    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {} for locator using='{}' value='{}' (found {})",
                resolved.index,
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    driver.click_element(&session_id, element_id).await?;
    if clear_first {
        let _ = driver.clear_element(&session_id, element_id).await;
    }
    driver.type_element(&session_id, element_id, text).await?;
    if press_enter {
        driver.press_enter(&session_id).await?;
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "typedLength": text.chars().count(),
            "pressEnter": press_enter,
            "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
        }),
        "type complete",
    ))
}

async fn action_typeahead(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let field = arguments.get("field").cloned().ok_or_else(|| {
        ToolCallError::new(ToolErrorCode::InvalidParams, "field is required", json!({}))
    })?;
    let typing_mode = arguments
        .get("typingMode")
        .and_then(Value::as_str)
        .unwrap_or("full")
        .to_lowercase();
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 20) as usize;

    let prefixes = resolve_prefixes_for_typeahead(arguments)?;
    let suggestion_query = parse_node_query(arguments.get("suggestionQuery"));

    let driver = driver_from_state(state).await?;

    let mut prefixes_out = Vec::new();
    let mut final_suggestions = Vec::new();

    for prefix in &prefixes {
        type_into_field(state, &driver, &session_id, &field, prefix, &typing_mode).await?;
        tokio::time::sleep(Duration::from_millis(900)).await;

        let source = driver.page_source(&session_id).await?;
        let suggestions = extract_suggestion_texts(&source, &suggestion_query, limit);
        final_suggestions = suggestions.clone();
        prefixes_out.push(json!({
            "prefix": prefix,
            "suggestions": suggestions,
            "suggestionCount": suggestions.len()
        }));
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "prefixes": prefixes_out,
            "prefixCount": prefixes.len(),
            "activePrefix": prefixes.last().cloned(),
            "suggestions": final_suggestions,
            "suggestionCount": final_suggestions.len(),
            "limit": limit
        }),
        "typeahead captured",
    ))
}

async fn action_wait(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(250, 180_000);

    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let ids = driver
            .find_elements(&session_id, &resolved.using, &resolved.value)
            .await?;
        if ids.is_empty() {
            // keep waiting
        } else if resolved.require_unique && ids.len() != 1 {
            return Err(ToolCallError::new(
                ToolErrorCode::AmbiguousMatch,
                format!(
                    "expected exactly one match for using='{}' value='{}', got {}",
                    &resolved.using,
                    &resolved.value,
                    ids.len()
                ),
                json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
            )
            .into());
        } else if let Some(element_id) = ids.get(resolved.index) {
            return Ok(tool_success(
                json!({
                    "ok": true,
                    "sessionId": session_id,
                    "elementId": element_id,
                    "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
                }),
                "element found",
            ));
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                format!(
                    "timeout waiting for locator using='{}' value='{}'",
                    &resolved.using, &resolved.value
                ),
                json!({"using": &resolved.using, "value": &resolved.value, "timeoutMs": timeout_ms}),
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn action_scroll(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let direction = required_str(arguments, "direction")?.to_lowercase();
    let distance = arguments
        .get("distance")
        .and_then(Value::as_f64)
        .unwrap_or(0.6)
        .clamp(0.1, 0.95);

    let driver = driver_from_state(state).await?;
    perform_scroll_gesture(&driver, &session_id, &direction, distance).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "direction": direction,
            "distance": distance
        }),
        "scroll complete",
    ))
}

async fn perform_scroll_gesture(
    driver: &WebDriverClient,
    session_id: &str,
    direction: &str,
    distance: f64,
) -> Result<()> {
    let distance = distance.clamp(0.1, 0.95);
    let (width, height) = driver.window_rect(session_id).await?;
    let (start_x, start_y, end_x, end_y) = match direction.trim().to_lowercase().as_str() {
        "down" => (
            width * 0.5,
            height * (0.5 + distance / 2.0),
            width * 0.5,
            height * (0.5 - distance / 2.0),
        ),
        "up" => (
            width * 0.5,
            height * (0.5 - distance / 2.0),
            width * 0.5,
            height * (0.5 + distance / 2.0),
        ),
        "left" => (
            width * (0.5 - distance / 2.0),
            height * 0.5,
            width * (0.5 + distance / 2.0),
            height * 0.5,
        ),
        "right" => (
            width * (0.5 + distance / 2.0),
            height * 0.5,
            width * (0.5 - distance / 2.0),
            height * 0.5,
        ),
        other => bail!("unsupported direction '{other}'"),
    };

    let payload = json!({
        "actions": [{
            "type": "pointer",
            "id": "finger1",
            "parameters": { "pointerType": "touch" },
            "actions": [
                {"type": "pointerMove", "duration": 0, "x": start_x, "y": start_y, "origin": "viewport"},
                {"type": "pointerDown", "button": 0},
                {"type": "pause", "duration": 100},
                {"type": "pointerMove", "duration": 400, "x": end_x, "y": end_y, "origin": "viewport"},
                {"type": "pointerUp", "button": 0}
            ]
        }]
    });
    driver.perform_actions(session_id, payload).await?;
    Ok(())
}

async fn action_swipe(state: &AppState, arguments: &Value) -> Result<Value> {
    action_scroll(state, arguments).await
}

async fn action_back(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.back(&session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id
        }),
        "back complete",
    ))
}

async fn action_scroll_until(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let direction = arguments
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("down")
        .to_lowercase();
    let distance = arguments
        .get("distance")
        .and_then(Value::as_f64)
        .unwrap_or(0.6)
        .clamp(0.1, 0.95);
    let max_scrolls = arguments
        .get("maxScrolls")
        .and_then(Value::as_u64)
        .unwrap_or(12)
        .clamp(0, 200) as u32;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(60_000)
        .clamp(250, 600_000);
    let settle_ms = arguments
        .get("settleMs")
        .and_then(Value::as_u64)
        .unwrap_or(350)
        .clamp(0, 10_000);

    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut scrolls: u32 = 0;

    loop {
        let ids = driver
            .find_elements(&session_id, &resolved.using, &resolved.value)
            .await?;

        if !ids.is_empty() {
            if resolved.require_unique && ids.len() != 1 {
                return Err(ToolCallError::new(
                    ToolErrorCode::AmbiguousMatch,
                    format!(
                        "expected exactly one match for using='{}' value='{}', got {}",
                        &resolved.using,
                        &resolved.value,
                        ids.len()
                    ),
                    json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
                )
                .into());
            }

            if let Some(element_id) = ids.get(resolved.index) {
                return Ok(tool_success(
                    json!({
                        "ok": true,
                        "found": true,
                        "sessionId": session_id,
                        "elementId": element_id,
                        "scrolls": scrolls,
                        "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
                    }),
                    "element found",
                ));
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                "timeout scrolling to target",
                json!({"using": &resolved.using, "value": &resolved.value, "scrolls": scrolls, "timeoutMs": timeout_ms}),
            )
            .into());
        }
        if scrolls >= max_scrolls {
            return Err(ToolCallError::new(
                ToolErrorCode::ElementNotFound,
                "target not found within maxScrolls",
                json!({"using": &resolved.using, "value": &resolved.value, "scrolls": scrolls, "maxScrolls": max_scrolls}),
            )
            .into());
        }

        perform_scroll_gesture(&driver, &session_id, &direction, distance).await?;
        scrolls += 1;
        if settle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(settle_ms)).await;
        }
    }
}

async fn element_text(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let text = driver.element_text(&session_id, element_id).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "text": text
        }),
        "element text read",
    ))
}

async fn element_attribute(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let name = required_str(arguments, "name")?;
    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let value = driver
        .element_attribute(&session_id, element_id, name)
        .await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "name": name,
            "value": value
        }),
        "element attribute read",
    ))
}

async fn element_rect(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let resolved = resolve_target(state, arguments).await?.ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required",
            json!({}),
        )
    })?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let rect = driver.element_rect(&session_id, element_id).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "rect": {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height}
        }),
        "element rect read",
    ))
}

async fn alert_text(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let text = driver.alert_text(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "text": text}),
        "alert text read",
    ))
}

async fn alert_accept(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.alert_accept(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id}),
        "alert accepted",
    ))
}

async fn alert_dismiss(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.alert_dismiss(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id}),
        "alert dismissed",
    ))
}

async fn alert_wait(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(250, 180_000);

    let driver = driver_from_state(state).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match driver.alert_text(&session_id).await {
            Ok(text) => {
                return Ok(tool_success(
                    json!({"ok": true, "sessionId": session_id, "text": text}),
                    "alert present",
                ));
            }
            Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolCallError::new(
                        ToolErrorCode::Timeout,
                        "timeout waiting for alert",
                        json!({"timeoutMs": timeout_ms}),
                    )
                    .into());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

fn normalize_text(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_match_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

#[derive(Debug, Clone)]
struct NodeQuery {
    element_type: Option<String>,
    name: Option<String>,
    name_prefix: Option<String>,
    name_contains: Option<String>,
    label: Option<String>,
    label_contains: Option<String>,
    visible_only: bool,
    ancestor_name: Option<String>,
    ancestor_type: Option<String>,
    max: Option<usize>,
}

fn parse_node_query(value: Option<&Value>) -> NodeQuery {
    let mut query = NodeQuery {
        element_type: Some("XCUIElementTypeCell".to_string()),
        name: None,
        name_prefix: None,
        name_contains: None,
        label: None,
        label_contains: None,
        visible_only: false,
        ancestor_name: None,
        ancestor_type: None,
        max: None,
    };

    let Some(obj) = value.and_then(Value::as_object) else {
        return query;
    };

    if let Some(value) = obj.get("type").and_then(Value::as_str) {
        if !value.trim().is_empty() {
            query.element_type = Some(value.trim().to_string());
        }
    }
    if let Some(value) = obj.get("name").and_then(Value::as_str) {
        query.name = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("namePrefix").and_then(Value::as_str) {
        query.name_prefix = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("nameContains").and_then(Value::as_str) {
        query.name_contains = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("label").and_then(Value::as_str) {
        query.label = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("labelContains").and_then(Value::as_str) {
        query.label_contains = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("visibleOnly").and_then(Value::as_bool) {
        query.visible_only = value;
    }
    if let Some(value) = obj.get("ancestorName").and_then(Value::as_str) {
        query.ancestor_name = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("ancestorType").and_then(Value::as_str) {
        query.ancestor_type = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("max").and_then(Value::as_u64) {
        query.max = Some(value.clamp(1, 100) as usize);
    }

    query
}

#[derive(Debug, Clone)]
struct RowQuery {
    element_type: String,
    name: Option<String>,
    name_prefix: Option<String>,
    name_contains: Option<String>,
    label: Option<String>,
    label_contains: Option<String>,
    visible_only: bool,
    ancestor_name: Option<String>,
    ancestor_name_contains: Option<String>,
    ancestor_type: Option<String>,
}

#[derive(Debug, Clone)]
struct PrimaryQuery {
    element_type: String,
    attr: String,
    pick: String,
}

#[derive(Debug, Clone)]
struct TagQuery {
    name_prefix: String,
    pick: String,
    strip_prefix: Option<String>,
    field: String,
}

#[derive(Debug, Clone)]
struct FieldQuery {
    name: String,
    query: NodeQuery,
    attr: String,
    pick: String,
    join_delimiter: Option<String>,
}

#[derive(Debug, Clone)]
struct SplitConfig {
    delimiter: String,
    ignore_prefixes: Vec<String>,
    fields: Vec<String>,
    skip_metric_like: bool,
}

#[derive(Debug, Clone)]
struct RowMatch {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    raw_label: String,
    fields: Vec<(String, String)>,
    extra_fields: Vec<(String, String)>,
    tag_field: Option<String>,
    tag_value: Option<String>,
}

type RowAccumulator = (
    usize,
    f64,
    f64,
    f64,
    f64,
    Vec<String>,
    Vec<String>,
    Vec<Vec<String>>,
);

fn parse_row_query(value: Option<&Value>) -> Result<RowQuery> {
    let Some(obj) = value.and_then(Value::as_object) else {
        return Err(anyhow!("row query is required"));
    };
    let element_type = obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("XCUIElementTypeCell")
        .to_string();

    Ok(RowQuery {
        element_type,
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        name_prefix: obj
            .get("namePrefix")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        name_contains: obj
            .get("nameContains")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        label: obj
            .get("label")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        label_contains: obj
            .get("labelContains")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        visible_only: obj
            .get("visibleOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ancestor_name: obj
            .get("ancestorName")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        ancestor_name_contains: obj
            .get("ancestorNameContains")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        ancestor_type: obj
            .get("ancestorType")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
    })
}

fn parse_primary_query(value: Option<&Value>) -> Result<PrimaryQuery> {
    let Some(obj) = value.and_then(Value::as_object) else {
        return Err(anyhow!("primary query is required"));
    };
    let element_type = obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("XCUIElementTypeButton")
        .to_string();
    let attr = obj
        .get("attr")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("label")
        .to_string();
    let pick = obj
        .get("pick")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("longest")
        .to_string();

    Ok(PrimaryQuery {
        element_type,
        attr,
        pick,
    })
}

fn parse_tag_query(value: Option<&Value>) -> Option<TagQuery> {
    let obj = value.and_then(Value::as_object)?;
    let name_prefix = obj
        .get("namePrefix")
        .and_then(Value::as_str)
        .and_then(|value| normalize_text(value.to_string()))?;
    let pick = obj
        .get("pick")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("last")
        .to_string();
    let strip_prefix = obj
        .get("stripPrefix")
        .and_then(Value::as_str)
        .and_then(|value| normalize_text(value.to_string()));
    let field = obj
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tag")
        .to_string();
    Some(TagQuery {
        name_prefix,
        pick,
        strip_prefix,
        field,
    })
}

fn parse_field_queries(value: Option<&Value>) -> Result<Vec<FieldQuery>> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for (idx, item) in values.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            bail!("fields[{idx}] must be an object");
        };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("fields[{idx}].name is required"))?;
        let query_value = obj
            .get("query")
            .ok_or_else(|| anyhow!("fields[{idx}].query is required"))?;
        let query = parse_node_query(Some(query_value));
        let attr = obj
            .get("attr")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("label")
            .to_string();
        let pick = obj
            .get("pick")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("first")
            .to_string();
        let join_delimiter = obj
            .get("joinDelimiter")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        out.push(FieldQuery {
            name: name.to_string(),
            query,
            attr,
            pick,
            join_delimiter,
        });
    }

    Ok(out)
}

fn parse_split_config(value: Option<&Value>) -> SplitConfig {
    let mut cfg = SplitConfig {
        delimiter: ",".to_string(),
        ignore_prefixes: Vec::new(),
        fields: vec!["name".to_string(), "subtitle".to_string()],
        skip_metric_like: true,
    };

    let Some(obj) = value.and_then(Value::as_object) else {
        return cfg;
    };

    if let Some(delim) = obj.get("delimiter").and_then(Value::as_str) {
        if !delim.trim().is_empty() {
            cfg.delimiter = delim.to_string();
        }
    }
    if let Some(values) = obj.get("ignorePrefixes").and_then(Value::as_array) {
        cfg.ignore_prefixes = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
    }
    if let Some(values) = obj.get("fields").and_then(Value::as_array) {
        let fields: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
        if !fields.is_empty() {
            cfg.fields = fields;
        }
    }
    if let Some(value) = obj.get("skipMetricLike").and_then(Value::as_bool) {
        cfg.skip_metric_like = value;
    }

    cfg
}

fn resolve_prefixes_for_typeahead(arguments: &Value) -> Result<Vec<String>> {
    if let Some(values) = arguments.get("prefixes").and_then(Value::as_array) {
        let mut out: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
        out.retain(|value| !value.is_empty());
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let Some(query) = arguments.get("query").and_then(Value::as_str) else {
        return Err(anyhow!("query or prefixes[] is required"));
    };
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("query or prefixes[] is required"));
    }

    let mut prefixes = Vec::new();
    let mut cur = String::new();
    for ch in query.chars() {
        cur.push(ch);
        if let Some(normalized) = normalize_text(cur.clone()) {
            prefixes.push(normalized);
        }
    }
    if prefixes.is_empty() {
        prefixes.push(query.to_string());
    }
    prefixes.dedup();
    Ok(prefixes)
}

async fn type_into_field(
    state: &AppState,
    driver: &WebDriverClient,
    session_id: &str,
    field: &Value,
    prefix: &str,
    typing_mode: &str,
) -> Result<()> {
    let resolved = resolve_target(
        state,
        &json!({
            "sessionId": session_id,
            "target": field
        }),
    )
    .await?
    .ok_or_else(|| anyhow!("unable to resolve field target"))?;
    let ids = driver
        .find_elements(session_id, &resolved.using, &resolved.value)
        .await?;
    let element_id = if let Some(id) = ids.get(resolved.index).cloned() {
        id
    } else {
        driver
            .active_element(session_id)
            .await
            .context("no field element found for typeahead")?
    };
    let _ = driver.click_element(session_id, &element_id).await;
    let _ = driver.clear_element(session_id, &element_id).await;
    if let Ok(clear_ids) = driver
        .find_elements(session_id, "accessibility id", "Clear text")
        .await
    {
        if let Some(clear_id) = clear_ids.first() {
            let _ = driver.click_element(session_id, clear_id).await;
        }
    }

    if typing_mode == "char-by-char" {
        for ch in prefix.chars() {
            let text = ch.to_string();
            driver.type_element(session_id, &element_id, &text).await?;
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    } else {
        driver.type_element(session_id, &element_id, prefix).await?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct TextNodeMatch {
    text: String,
    x: f64,
    y: f64,
}

fn extract_suggestion_texts(source: &str, query: &NodeQuery, limit: usize) -> Vec<Value> {
    let mut nodes = extract_nodes_from_source(source, query);
    nodes.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for node in nodes {
        let key = normalize_match_key(&node.text);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(json!({"text": node.text, "position": out.len() + 1}));
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn extract_nodes_from_source(source: &str, query: &NodeQuery) -> Vec<TextNodeMatch> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    let mut out = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = attr_text(&e, "name");
                if node_matches(&e, &elem_type, query, &stack) {
                    if let Some(text) = extract_preferred_text(&e) {
                        let (x, y) = (
                            attr_f64(&e, "x").unwrap_or(0.0),
                            attr_f64(&e, "y").unwrap_or(0.0),
                        );
                        out.push(TextNodeMatch { text, x, y });
                        if let Some(max) = query.max {
                            if out.len() >= max {
                                buf.clear();
                                break;
                            }
                        }
                    }
                }
                stack.push((elem_type, name));
            }
            Ok(Event::Empty(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if node_matches(&e, &elem_type, query, &stack) {
                    if let Some(text) = extract_preferred_text(&e) {
                        let (x, y) = (
                            attr_f64(&e, "x").unwrap_or(0.0),
                            attr_f64(&e, "y").unwrap_or(0.0),
                        );
                        out.push(TextNodeMatch { text, x, y });
                        if let Some(max) = query.max {
                            if out.len() >= max {
                                buf.clear();
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

fn node_matches(
    elem: &quick_xml::events::BytesStart<'_>,
    elem_type: &str,
    query: &NodeQuery,
    ancestors: &[(String, Option<String>)],
) -> bool {
    if let Some(want_type) = &query.element_type {
        if elem_type != want_type {
            return false;
        }
    }

    if let Some(want_name) = &query.name {
        if attr_text(elem, "name").as_deref() != Some(want_name.as_str()) {
            return false;
        }
    }
    if let Some(prefix) = &query.name_prefix {
        let name = attr_text(elem, "name").unwrap_or_default();
        if !name.starts_with(prefix) {
            return false;
        }
    }
    if let Some(contains) = &query.name_contains {
        let name = attr_text(elem, "name").unwrap_or_default().to_lowercase();
        if !name.contains(&contains.to_lowercase()) {
            return false;
        }
    }
    if let Some(label) = &query.label {
        if attr_text(elem, "label").as_deref() != Some(label.as_str()) {
            return false;
        }
    }
    if let Some(contains) = &query.label_contains {
        let label = attr_text(elem, "label").unwrap_or_default().to_lowercase();
        if !label.contains(&contains.to_lowercase()) {
            return false;
        }
    }
    if let Some(contains) = &query.label_contains {
        let label = attr_text(elem, "label").unwrap_or_default().to_lowercase();
        if !label.contains(&contains.to_lowercase()) {
            return false;
        }
    }
    if query.visible_only && attr_text(elem, "visible").as_deref() != Some("true") {
        return false;
    }

    if query.ancestor_name.is_none() && query.ancestor_type.is_none() {
        return true;
    }

    ancestors.iter().any(|(ancestor_type, ancestor_name)| {
        if let Some(want_name) = &query.ancestor_name {
            if ancestor_name.as_deref() != Some(want_name.as_str()) {
                return false;
            }
        }
        if let Some(want_type) = &query.ancestor_type {
            if ancestor_type != want_type {
                return false;
            }
        }
        true
    })
}

fn extract_rows_from_source(
    source: &str,
    row_query: &RowQuery,
    primary_query: &PrimaryQuery,
    tag_query: Option<&TagQuery>,
    field_queries: &[FieldQuery],
    split_cfg: &SplitConfig,
) -> Vec<RowMatch> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    let mut current: Option<RowAccumulator> = None;
    let mut out = Vec::new();

    let mut depth = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = attr_text(&e, "name");

                if current.is_none() && element_matches_row(&e, &elem_type, row_query, &stack) {
                    let mut field_matches = Vec::new();
                    field_matches.resize_with(field_queries.len(), Vec::new);
                    current = Some((
                        depth,
                        attr_f64(&e, "x").unwrap_or(0.0),
                        attr_f64(&e, "y").unwrap_or(0.0),
                        attr_f64(&e, "width").unwrap_or(0.0),
                        attr_f64(&e, "height").unwrap_or(0.0),
                        Vec::new(),
                        Vec::new(),
                        field_matches,
                    ));
                }

                if let Some((_row_depth, _x, _y, _width, _height, labels, tags, field_matches)) =
                    current.as_mut()
                {
                    collect_primary_label(&elem_type, &e, primary_query, labels);
                    collect_tag_value(&e, tag_query, tags);
                    collect_field_matches(&elem_type, &e, field_queries, field_matches, &stack);
                }

                stack.push((elem_type, name));
            }
            Ok(Event::Empty(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if current.is_none() && element_matches_row(&e, &elem_type, row_query, &stack) {
                    let mut labels = Vec::new();
                    let mut tags = Vec::new();
                    let mut field_matches = Vec::new();
                    field_matches.resize_with(field_queries.len(), Vec::new);
                    collect_primary_label(&elem_type, &e, primary_query, &mut labels);
                    collect_tag_value(&e, tag_query, &mut tags);
                    collect_field_matches(
                        &elem_type,
                        &e,
                        field_queries,
                        &mut field_matches,
                        &stack,
                    );
                    let row = finalize_row(
                        attr_f64(&e, "x").unwrap_or(0.0),
                        attr_f64(&e, "y").unwrap_or(0.0),
                        attr_f64(&e, "width").unwrap_or(0.0),
                        attr_f64(&e, "height").unwrap_or(0.0),
                        labels,
                        tags,
                        field_matches,
                        primary_query,
                        tag_query,
                        field_queries,
                        split_cfg,
                    );
                    if let Some(row) = row {
                        out.push(row);
                    }
                } else if let Some((
                    _row_depth,
                    _x,
                    _y,
                    _width,
                    _height,
                    labels,
                    tags,
                    field_matches,
                )) = current.as_mut()
                {
                    collect_primary_label(&elem_type, &e, primary_query, labels);
                    collect_tag_value(&e, tag_query, tags);
                    collect_field_matches(&elem_type, &e, field_queries, field_matches, &stack);
                }
            }
            Ok(Event::End(_)) => {
                if let Some((row_depth, x, y, width, height, labels, tags, field_matches)) =
                    current.take()
                {
                    if row_depth == depth {
                        if let Some(row) = finalize_row(
                            x,
                            y,
                            width,
                            height,
                            labels,
                            tags,
                            field_matches,
                            primary_query,
                            tag_query,
                            field_queries,
                            split_cfg,
                        ) {
                            out.push(row);
                        }
                    } else {
                        current =
                            Some((row_depth, x, y, width, height, labels, tags, field_matches));
                    }
                }
                stack.pop();
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

fn element_matches_row(
    elem: &quick_xml::events::BytesStart<'_>,
    elem_type: &str,
    query: &RowQuery,
    ancestors: &[(String, Option<String>)],
) -> bool {
    if elem_type != query.element_type {
        return false;
    }
    if let Some(want) = &query.name {
        if attr_text(elem, "name").as_deref() != Some(want.as_str()) {
            return false;
        }
    }
    if let Some(prefix) = &query.name_prefix {
        let name = attr_text(elem, "name").unwrap_or_default();
        if !name.starts_with(prefix) {
            return false;
        }
    }
    if let Some(contains) = &query.name_contains {
        let name = attr_text(elem, "name").unwrap_or_default().to_lowercase();
        if !name.contains(&contains.to_lowercase()) {
            return false;
        }
    }
    if let Some(label) = &query.label {
        if attr_text(elem, "label").as_deref() != Some(label.as_str()) {
            return false;
        }
    }
    if let Some(contains) = &query.label_contains {
        let label = attr_text(elem, "label").unwrap_or_default().to_lowercase();
        if !label.contains(&contains.to_lowercase()) {
            return false;
        }
    }
    if query.visible_only && attr_text(elem, "visible").as_deref() != Some("true") {
        return false;
    }
    if query.ancestor_name.is_none() && query.ancestor_type.is_none() {
        return true;
    }
    ancestors.iter().any(|(ancestor_type, ancestor_name)| {
        if let Some(want_name) = &query.ancestor_name {
            if ancestor_name.as_deref() != Some(want_name.as_str()) {
                return false;
            }
        }
        if let Some(want_contains) = &query.ancestor_name_contains {
            let name = ancestor_name.clone().unwrap_or_default().to_lowercase();
            if !name.contains(&want_contains.to_lowercase()) {
                return false;
            }
        }
        if let Some(want_type) = &query.ancestor_type {
            if ancestor_type != want_type {
                return false;
            }
        }
        true
    })
}

fn collect_primary_label(
    elem_type: &str,
    elem: &quick_xml::events::BytesStart<'_>,
    query: &PrimaryQuery,
    labels: &mut Vec<String>,
) {
    if elem_type != query.element_type {
        return;
    }
    if let Some(value) = attr_text(elem, &query.attr) {
        labels.push(value);
    }
}

fn collect_tag_value(
    elem: &quick_xml::events::BytesStart<'_>,
    query: Option<&TagQuery>,
    tags: &mut Vec<String>,
) {
    let Some(query) = query else {
        return;
    };
    let Some(name) = attr_text(elem, "name") else {
        return;
    };
    if let Some(stripped) = name.strip_prefix(&query.name_prefix) {
        let cleaned = stripped.trim();
        if !cleaned.is_empty() {
            tags.push(cleaned.to_string());
        }
    }
}

fn collect_field_matches(
    elem_type: &str,
    elem: &quick_xml::events::BytesStart<'_>,
    field_queries: &[FieldQuery],
    field_matches: &mut [Vec<String>],
    ancestors: &[(String, Option<String>)],
) {
    if field_queries.is_empty() {
        return;
    }
    for (idx, field) in field_queries.iter().enumerate() {
        if !node_matches(elem, elem_type, &field.query, ancestors) {
            continue;
        }
        let value = attr_text(elem, &field.attr).or_else(|| extract_preferred_text(elem));
        if let Some(value) = value {
            if let Some(bucket) = field_matches.get_mut(idx) {
                bucket.push(value);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_row(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    labels: Vec<String>,
    tags: Vec<String>,
    field_matches: Vec<Vec<String>>,
    primary_query: &PrimaryQuery,
    tag_query: Option<&TagQuery>,
    field_queries: &[FieldQuery],
    split_cfg: &SplitConfig,
) -> Option<RowMatch> {
    let raw_label = if primary_query.pick == "first" {
        labels.first().cloned().unwrap_or_default()
    } else {
        labels
            .into_iter()
            .max_by_key(|value| value.len())
            .unwrap_or_default()
    };
    if raw_label.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = raw_label
        .split(&split_cfg.delimiter)
        .filter_map(|value| normalize_text(value.to_string()))
        .collect();
    if let Some(first) = parts.first() {
        if split_cfg
            .ignore_prefixes
            .iter()
            .any(|prefix| prefix.eq_ignore_ascii_case(first))
        {
            parts.remove(0);
        }
    }
    if split_cfg.skip_metric_like {
        parts.retain(|part| !metric_like(part));
    }

    let mut fields = Vec::new();
    for (idx, field_name) in split_cfg.fields.iter().enumerate() {
        let value = parts.get(idx).cloned().unwrap_or_default();
        fields.push((field_name.clone(), value));
    }

    let mut extra_fields = Vec::new();
    for (idx, field) in field_queries.iter().enumerate() {
        let values = field_matches.get(idx).cloned().unwrap_or_default();
        if values.is_empty() {
            continue;
        }
        let value = match field.pick.as_str() {
            "last" => values.last().cloned(),
            "longest" => values.into_iter().max_by_key(|v| v.len()),
            "all" => {
                let joiner = field
                    .join_delimiter
                    .clone()
                    .unwrap_or_else(|| " | ".to_string());
                Some(values.join(&joiner))
            }
            _ => values.first().cloned(),
        };
        if let Some(value) = value {
            extra_fields.push((field.name.clone(), value));
        }
    }

    let (tag_field, tag_value) = if let Some(tag_query) = tag_query {
        let selected = if tag_query.pick == "first" {
            tags.first().cloned()
        } else {
            tags.last().cloned()
        };
        let cleaned = selected.map(|value| {
            if let Some(prefix) = &tag_query.strip_prefix {
                value
                    .strip_prefix(prefix)
                    .unwrap_or(&value)
                    .trim()
                    .to_string()
            } else {
                value
            }
        });
        (Some(tag_query.field.clone()), cleaned)
    } else {
        (None, None)
    };

    Some(RowMatch {
        x,
        y,
        width,
        height,
        raw_label,
        fields,
        extra_fields,
        tag_field,
        tag_value,
    })
}

fn preferred_row_tap_x(x: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return x;
    }
    x + (width * 0.18).clamp(16.0, 72.0)
}

fn preferred_row_tap_y(y: f64, height: f64) -> f64 {
    if height <= 0.0 {
        return y;
    }
    y + (height * 0.12).clamp(18.0, 56.0)
}

fn metric_like(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("star")
        || lower.contains("rating")
        || lower.ends_with("ratings")
        || lower.ends_with("rating")
        || lower.ends_with("reviews")
}

fn attr_text(elem: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for attr in elem.attributes().with_checks(false) {
        let Ok(attr) = attr else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(attr.key.as_ref()) else {
            continue;
        };
        if name != key {
            continue;
        }
        let Ok(raw) = attr.unescape_value() else {
            continue;
        };
        return normalize_text(raw.into_owned());
    }
    None
}

fn attr_f64(elem: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<f64> {
    attr_text(elem, key).and_then(|value| value.parse::<f64>().ok())
}

fn extract_preferred_text(elem: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    attr_text(elem, "label")
        .or_else(|| attr_text(elem, "name"))
        .or_else(|| attr_text(elem, "value"))
}

async fn session_delete(state: &AppState, arguments: &Value) -> Result<Value> {
    let _ = state.restore_persisted_runtime().await;
    let session_id = resolve_session_id(state, arguments).await?;
    let stop_appium = arguments
        .get("stopAppium")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let shutdown_wda = arguments
        .get("shutdownWDA")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let snapshot = state.snapshot().await;
    let wda_port = state
        .last_wda_local_port()
        .await
        .unwrap_or(DEFAULT_WDA_LOCAL_PORT);
    let driver = driver_from_state(state).await?;
    driver.delete_session(&session_id).await?;

    let wda_shutdown_ok = if shutdown_wda {
        shutdown_wda_on_port(wda_port).await.unwrap_or(false)
    } else {
        false
    };

    state.clear_session().await;

    let mut stopped_appium = false;
    if stop_appium {
        match snapshot.appium_source {
            Some(AppiumSource::Spawned) => {
                state.shutdown_spawned_appium().await;
                stopped_appium = true;
            }
            Some(AppiumSource::Env) => {
                state.clear_appium_metadata().await;
            }
            None => {}
        }
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "stopAppium": stop_appium,
            "stoppedAppium": stopped_appium,
            "shutdownWDA": shutdown_wda,
            "wdaLocalPort": wda_port,
            "wdaShutdownOk": wda_shutdown_ok
        }),
        "session deleted",
    ))
}

async fn session_info(state: &AppState) -> Result<Value> {
    let _ = state.restore_persisted_runtime().await;
    let snapshot = state.snapshot().await;
    Ok(tool_success(
        json!({
            "ok": true,
            "appiumBaseUrl": snapshot.appium_base_url,
            "appiumPid": snapshot.appium_pid,
            "session": snapshot.session
        }),
        "session info",
    ))
}

async fn app_activate(state: &AppState, arguments: &Value) -> Result<Value> {
    let session = state
        .active_session()
        .await
        .ok_or_else(|| anyhow!("no active session; call ios.session.create first"))?;
    if session.kind != "native_app" {
        bail!(
            "ios.app.activate requires a native_app session (current kind={})",
            session.kind
        );
    }

    let session_id = resolve_session_id(state, arguments).await?;
    if session_id != session.session_id {
        bail!("unknown sessionId (this worker supports a single active session)");
    }

    let bundle_id = required_str(arguments, "bundleId")?.trim().to_string();
    if bundle_id.is_empty() {
        bail!("bundleId is empty");
    }
    let terminate_first = arguments
        .get("terminateFirst")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let driver = driver_from_state(state).await?;
    let terminate_result = if terminate_first {
        driver
            .execute_script(
                &session_id,
                "mobile: terminateApp",
                json!([{ "bundleId": &bundle_id }]),
            )
            .await?
            .get("value")
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let activate_result = driver
        .execute_script(
            &session_id,
            "mobile: activateApp",
            json!([{ "bundleId": &bundle_id }]),
        )
        .await?
        .get("value")
        .cloned()
        .unwrap_or(Value::Null);

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "bundleId": bundle_id,
            "terminateFirst": terminate_first,
            "terminateResult": terminate_result,
            "activateResult": activate_result
        }),
        "app activated",
    ))
}

async fn wda_shutdown(state: &AppState, arguments: &Value) -> Result<Value> {
    let port_from_args = parse_port_value(arguments.get("port"), "port")?;

    let port = match port_from_args {
        Some(port) => port,
        None => state
            .last_wda_local_port()
            .await
            .unwrap_or(DEFAULT_WDA_LOCAL_PORT),
    };

    let ok = shutdown_wda_on_port(port).await.unwrap_or(false);

    Ok(tool_success(
        json!({
            "ok": true,
            "port": port,
            "shutdownOk": ok
        }),
        "wda shutdown attempted",
    ))
}

async fn web_goto(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let url = required_str(arguments, "url")?;
    let driver = driver_from_state(state).await?;
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
    loop {
        let response = driver
            .execute_script(&session_id, script, args.clone())
            .await?;
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

        if std::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                "timeout waiting for JavaScript condition",
                json!({"tool": "ios.web.wait_js"}),
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

    Ok(tool_success_with_content(
        json!({
            "ok": true,
            "sessionId": session_id,
            "mimeType": "image/png",
            "bytesBase64": data.len(),
            "data": data
        }),
        vec![
            json!({"type": "text", "text": "screenshot captured"}),
            json!({"type": "image", "mimeType": "image/png", "data": data}),
        ],
    ))
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

async fn workflow_list(arguments: &Value) -> Result<Value> {
    let system_filter = arguments
        .get("system")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let family_filter = arguments
        .get("family")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let flows = workflows::list_workflows(system_filter, family_filter);
    let systems = workflows::group_workflows_by_system(&flows);
    Ok(tool_success(
        json!({
            "systemCount": systems.len(),
            "workflowCount": flows.len(),
            "systems": systems,
            "workflows": flows
        }),
        "workflow list",
    ))
}

async fn workflow_failure_report_review(arguments: &Value) -> Result<Value> {
    let note = arguments
        .get("note")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let draft_value = arguments
        .get("draft")
        .or_else(|| arguments.get("payload"))
        .or_else(|| arguments.get("summary"))
        .ok_or_else(|| anyhow!("draft or summary is required"))?;
    let draft = workflow_failure_report::draft_from_value(draft_value, note)?;
    let review = workflow_failure_report::review_payload(&draft);

    Ok(tool_success(
        review,
        "phone automation failure draft ready for host",
    ))
}

async fn workflow_failure_report_submit(arguments: &Value) -> Result<Value> {
    let note = arguments
        .get("note")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let mut draft = if let Some(draft) = arguments.get("draft").or_else(|| arguments.get("payload"))
    {
        workflow_failure_report::draft_from_value(draft, None)?
    } else if let Some(summary) = arguments.get("summary") {
        workflow_failure_report::draft_from_value(summary, note.clone())?
    } else {
        bail!("draft or summary is required");
    };
    workflow_failure_report::apply_note_if_missing(&mut draft, note)?;

    let review = workflow_failure_report::review_payload(&draft);
    Ok(tool_success(
        json!({
            "ok": true,
            "status": "draft_only",
            "deprecated": true,
            "message": "Draft created. The host must submit it with user/auth context.",
            "review": review,
            "draft": draft,
            "hostEvent": draft.host_event(),
            "manualReportCommand": workflow_failure_report::manual_report_command(&draft)
        }),
        "phone automation failure draft created; host submission required",
    ))
}

async fn workflow_failure_report_queue(arguments: &Value) -> Result<Value> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("list");

    let payload = match action {
        "list" => json!({
            "ok": true,
            "queuedCount": 0,
            "reports": [],
            "message": "Local failure-report queues are deprecated. The host owns submission retries."
        }),
        "clear" => json!({
            "ok": true,
            "queuedCount": 0,
            "message": "No local failure-report queue is maintained by the worker."
        }),
        other => bail!("unknown queue action '{other}'"),
    };

    Ok(tool_success(payload, "workflow failure report queue"))
}

async fn capability_list(_arguments: &Value) -> Result<Value> {
    let mut grouped_tools: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tool in list_tool_definitions() {
        let Some(name) = tool.get("name").and_then(Value::as_str) else {
            continue;
        };
        let family = tool
            .get("capabilityFamily")
            .and_then(Value::as_str)
            .unwrap_or("other");
        grouped_tools
            .entry(family.to_string())
            .or_default()
            .push(name.to_string());
    }

    let families = policy::planner_capability_families();
    let family_order = families
        .iter()
        .filter_map(|family| family.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let mut tool_families = Vec::new();
    for family in &family_order {
        let Some(mut tools) = grouped_tools.remove(family) else {
            continue;
        };
        tools.sort();
        tool_families.push(json!({
            "family": family,
            "tools": tools
        }));
    }
    tool_families.extend(grouped_tools.into_iter().map(|(family, mut tools)| {
        tools.sort();
        json!({
            "family": family,
            "tools": tools
        })
    }));

    let mut grouped_workflows: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for workflow in workflows::list_workflows(None, None) {
        let Some(capability) = workflow.capability.as_ref() else {
            continue;
        };
        grouped_workflows
            .entry(capability.family.clone())
            .or_default()
            .push(json!({
                "id": workflow.id,
                "intent": capability.intent,
                "surface": capability.surface,
                "mutating": capability.mutating
            }));
    }

    let mut workflow_families = Vec::new();
    for family in &family_order {
        let Some(workflows) = grouped_workflows.remove(family) else {
            continue;
        };
        workflow_families.push(json!({
            "family": family,
            "workflows": workflows
        }));
    }
    workflow_families.extend(grouped_workflows.into_iter().map(|(family, workflows)| {
        json!({
            "family": family,
            "workflows": workflows
        })
    }));

    Ok(tool_success(
        json!({
            "ok": true,
            "tiers": {
                "tier1": "LLM-facing capability families for planning and intent selection.",
                "tier2": "Runtime-facing primitive tools used to execute a plan."
            },
            "families": families,
            "toolFamilies": tool_families,
            "workflowFamilies": workflow_families
        }),
        "capability taxonomy",
    ))
}

async fn workflow_run(state: &AppState, arguments: &Value) -> Result<Value> {
    let workflow_ref = resolve_workflow_ref(arguments)?;
    let commit = arguments
        .get("commit")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let disconnect_on_finish = arguments
        .get("disconnectOnFinish")
        .and_then(Value::as_bool)
        .or_else(|| arguments.get("closeOnFinish").and_then(Value::as_bool))
        .unwrap_or(true);
    let background_app_on_finish = arguments
        .get("backgroundAppOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lock_device_on_finish = arguments
        .get("lockDeviceOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let stop_appium_on_finish = arguments
        .get("stopAppiumOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let def = workflows::load_file_workflow(&workflow_ref)
        .ok_or_else(|| anyhow!("unknown workflow '{workflow_ref}' (no JSON workflow found)"))?;
    let output_result = {
        if let Some(ref steps) = def.steps {
            let mut vars = build_workflow_vars(arguments);
            copy_policy_arguments(arguments, &mut vars);
            workflows::merge_input_defaults(&def, &mut vars)?;
            run_steps(
                state,
                steps,
                commit,
                &vars,
                def.output.as_ref(),
                def.presentation.as_ref(),
            )
            .await
        } else {
            bail!("workflow '{workflow_ref}' has no executable steps")
        }
    };

    let output = match output_result {
        Ok(output) => output,
        Err(err) => {
            if stop_appium_on_finish {
                let _ = worker_shutdown(
                    state,
                    &json!({
                        "stopAppium": true,
                        "shutdownWDA": true,
                        "backgroundApp": background_app_on_finish,
                        "lockDevice": lock_device_on_finish
                    }),
                )
                .await;
            } else if disconnect_on_finish {
                let _ = worker_shutdown(
                    state,
                    &json!({
                        "stopAppium": false,
                        "shutdownWDA": true,
                        "backgroundApp": background_app_on_finish,
                        "lockDevice": lock_device_on_finish
                    }),
                )
                .await;
            } else if background_app_on_finish || lock_device_on_finish {
                let _ = perform_post_run_device_actions(
                    state,
                    background_app_on_finish,
                    lock_device_on_finish,
                )
                .await;
            }

            let message = format!("workflow '{workflow_ref}' failed: {err:#}");
            let lowered = message.to_lowercase();
            let code = if lowered.contains("device was not, or could not be, unlocked")
                || lowered.contains("could not be unlocked")
                || lowered.contains("bserrorcodedescription=locked")
                || lowered.contains(" for reason: locked")
            {
                ToolErrorCode::DeviceLocked
            } else if lowered.contains("timeout") {
                ToolErrorCode::Timeout
            } else {
                ToolErrorCode::ActionFailed
            };

            let failure_report = build_flow_failure_report_draft(
                &def,
                &workflow_ref,
                None,
                Some(code.as_str()),
                Some(&message),
            )?;
            let host_event = failure_report.host_event();
            return Err(ToolCallError::new(
                code,
                message,
                json!({
                    "workflow": workflow_ref,
                    "flowFailureReportDraft": failure_report,
                    "hostEvent": host_event
                }),
            )
            .into());
        }
    };

    if output.get("ok").and_then(Value::as_bool) == Some(false) {
        let failure_report = build_flow_failure_report_draft(
            &def,
            &workflow_ref,
            Some(&output),
            output.get("errorCode").and_then(Value::as_str),
            output.get("error").and_then(Value::as_str),
        )?;
        let host_event = failure_report.host_event();
        let message = output
            .get("error")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("workflow '{workflow_ref}' failed"));

        if stop_appium_on_finish {
            let _ = worker_shutdown(
                state,
                &json!({
                    "stopAppium": true,
                    "shutdownWDA": true,
                    "backgroundApp": background_app_on_finish,
                    "lockDevice": lock_device_on_finish
                }),
            )
            .await;
        } else if disconnect_on_finish {
            let _ = worker_shutdown(
                state,
                &json!({
                    "stopAppium": false,
                    "shutdownWDA": true,
                    "backgroundApp": background_app_on_finish,
                    "lockDevice": lock_device_on_finish
                }),
            )
            .await;
        } else if background_app_on_finish || lock_device_on_finish {
            let _ = perform_post_run_device_actions(
                state,
                background_app_on_finish,
                lock_device_on_finish,
            )
            .await;
        }

        return Err(ToolCallError::new(
            tool_error_code_from_value(output.get("errorCode")),
            message,
            json!({
                "workflow": workflow_ref,
                "flowFailureReportDraft": failure_report,
                "hostEvent": host_event
            }),
        )
        .into());
    }

    let screenshot_block = output
        .get("screenshot")
        .and_then(|value| value.get("data").and_then(Value::as_str))
        .filter(|data| !data.trim().is_empty())
        .map(|data| {
            json!({
                "type": "image",
                "mimeType": output.get("screenshot").and_then(|v| v.get("mimeType")).and_then(Value::as_str).unwrap_or("image/png"),
                "data": data
            })
        })
        .or_else(|| {
            output
                .get("trace")
                .and_then(Value::as_array)
                .and_then(|trace| {
                    trace.iter().rev().find_map(|entry| {
                        let result = entry.get("result")?;
                        let content = result.get("content")?.as_array()?;
                        content.iter().find_map(|block| {
                            let typ = block.get("type")?.as_str()?;
                            if typ != "image" {
                                return None;
                            }
                            let data = block.get("data")?.as_str()?;
                            if data.trim().is_empty() {
                                return None;
                            }
                            Some(block.clone())
                        })
                    })
                })
        })
        .unwrap_or_else(|| json!({"type": "text", "text": "no screenshot"}));

    let content = vec![
        json!({"type": "text", "text": format!("workflow '{workflow_ref}' completed")}),
        screenshot_block,
    ];

    if stop_appium_on_finish {
        let _ = worker_shutdown(
            state,
            &json!({
                "stopAppium": true,
                "shutdownWDA": true,
                "backgroundApp": background_app_on_finish,
                "lockDevice": lock_device_on_finish
            }),
        )
        .await;
    } else if disconnect_on_finish {
        let _ = worker_shutdown(
            state,
            &json!({
                "stopAppium": false,
                "shutdownWDA": true,
                "backgroundApp": background_app_on_finish,
                "lockDevice": lock_device_on_finish
            }),
        )
        .await;
    } else if background_app_on_finish || lock_device_on_finish {
        let _ =
            perform_post_run_device_actions(state, background_app_on_finish, lock_device_on_finish)
                .await;
    }

    Ok(tool_success_with_content(output, content))
}

fn build_flow_failure_report_draft(
    def: &workflows::FileWorkflowDefinition,
    workflow_ref: &str,
    output: Option<&Value>,
    fallback_error: Option<&str>,
    fallback_message: Option<&str>,
) -> Result<FlowFailureReportDraft> {
    let (system, flow) = workflow_ref
        .split_once('/')
        .map(|(system, workflow)| (system.to_string(), format!("{system}/{workflow}")))
        .unwrap_or_else(|| {
            let system = def
                .name
                .split_once('.')
                .map(|(system, _)| system)
                .unwrap_or("unknown");
            (system.to_string(), workflow_ref.to_string())
        });
    let surface = def
        .capability
        .as_ref()
        .and_then(|capability| capability.surface.clone())
        .unwrap_or(system);

    let failed_stage = output
        .and_then(|value| value.get("failedStepId").and_then(Value::as_str))
        .map(ToString::to_string)
        .or_else(|| {
            output.and_then(|value| {
                let failed_step = value.get("failedStep").and_then(Value::as_u64)?;
                value
                    .get("trace")
                    .and_then(Value::as_array)
                    .and_then(|trace| {
                        trace.iter().find_map(|entry| {
                            if entry.get("step").and_then(Value::as_u64) == Some(failed_step) {
                                entry.get("stepId").and_then(Value::as_str)
                            } else {
                                None
                            }
                        })
                    })
                    .map(ToString::to_string)
                    .or_else(|| Some(failed_step.to_string()))
            })
        })
        .unwrap_or_else(|| "workflow".to_string());

    workflow_failure_report::classify_failure(
        FlowFailureContext {
            surface,
            flow,
            flow_version: def.version.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: "ios".to_string(),
        },
        Some(&failed_stage),
        output
            .and_then(|value| value.get("errorCode").and_then(Value::as_str))
            .or(fallback_error),
        fallback_message,
        None,
    )
}

fn build_workflow_vars(arguments: &Value) -> Value {
    let mut vars = serde_json::Map::new();

    if let Some(obj) = arguments.get("args").and_then(Value::as_object) {
        for (k, v) in obj {
            vars.insert(k.clone(), v.clone());
        }
    }

    if let Some(obj) = arguments.get("session").and_then(Value::as_object) {
        for (k, v) in obj {
            if k == "signing" {
                continue;
            }
            vars.insert(k.clone(), v.clone());
        }
        if let Some(signing) = obj.get("signing").and_then(Value::as_object) {
            for (k, v) in signing {
                vars.insert(k.clone(), v.clone());
            }
        }
    }

    vars.entry("showXcodeLog".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("allowProvisioningUpdates".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("allowProvisioningDeviceRegistration".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("sessionCreateTimeoutMs".to_string())
        .or_insert_with(|| json!(600_000));
    vars.entry("wdaLaunchTimeoutMs".to_string())
        .or_insert_with(|| json!(240_000));
    vars.entry("wdaConnectionTimeoutMs".to_string())
        .or_insert_with(|| json!(120_000));

    vars.entry("xcodeOrgId".to_string())
        .or_insert_with(|| json!(""));
    vars.entry("xcodeSigningId".to_string())
        .or_insert_with(|| json!(""));
    vars.entry("updatedWDABundleId".to_string())
        .or_insert_with(|| json!(""));

    Value::Object(vars)
}

fn copy_policy_arguments(arguments: &Value, vars: &mut Value) {
    let Some(vars_obj) = vars.as_object_mut() else {
        return;
    };
    for key in ["privacyGate", "privacyGates", "privacyClass"] {
        if let Some(value) = arguments.get(key) {
            vars_obj
                .entry(key.to_string())
                .or_insert_with(|| value.clone());
        }
    }
}

async fn script_run(state: &AppState, arguments: &Value) -> Result<Value> {
    let steps = arguments
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("steps must be an array"))?;
    let commit = arguments
        .get("commit")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut vars = arguments.get("vars").cloned().unwrap_or_else(|| json!({}));
    copy_policy_arguments(arguments, &mut vars);
    let disconnect_on_finish = arguments
        .get("disconnectOnFinish")
        .and_then(Value::as_bool)
        .or_else(|| arguments.get("closeOnFinish").and_then(Value::as_bool))
        .unwrap_or(true);
    let background_app_on_finish = arguments
        .get("backgroundAppOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lock_device_on_finish = arguments
        .get("lockDeviceOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let stop_appium_on_finish = arguments
        .get("stopAppiumOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let result = run_steps(state, steps, commit, &vars, None, None).await?;

    if stop_appium_on_finish {
        let _ = worker_shutdown(
            state,
            &json!({
                "stopAppium": true,
                "backgroundApp": background_app_on_finish,
                "lockDevice": lock_device_on_finish
            }),
        )
        .await;
    } else if disconnect_on_finish {
        let _ = worker_shutdown(
            state,
            &json!({
                "stopAppium": false,
                "backgroundApp": background_app_on_finish,
                "lockDevice": lock_device_on_finish
            }),
        )
        .await;
    } else if background_app_on_finish || lock_device_on_finish {
        let _ =
            perform_post_run_device_actions(state, background_app_on_finish, lock_device_on_finish)
                .await;
    }

    Ok(tool_success(result, "script complete"))
}

async fn phone_messages_list_recent_threads(state: &AppState, arguments: &Value) -> Result<Value> {
    let device_id = required_device_id(arguments)?;
    let max_threads = bounded_usize_arg(arguments, "maxThreads", 25, 1, 50);
    let background_app_on_finish = bool_arg(arguments, "backgroundAppOnFinish", true);
    let lock_device_on_finish = bool_arg(arguments, "lockDeviceOnFinish", false);

    let steps = vec![
        json!({ "tool": "ios.appium.ensure", "arguments": {} }),
        json!({
            "tool": "ios.session.create",
            "arguments": {
                "udid": "{{udid}}",
                "kind": "native_app",
                "bundleId": "com.apple.MobileSMS",
                "replaceExisting": true,
                "noReset": true
            }
        }),
        json!({
            "tool": "ios.action.wait",
            "arguments": {
                "target": {
                    "using": "-ios predicate string",
                    "value": "name CONTAINS[c] 'Compose' OR label CONTAINS[c] 'Compose' OR name CONTAINS[c] 'Messages' OR label CONTAINS[c] 'Messages'"
                },
                "timeoutMs": 30000
            },
            "retries": 1
        }),
        json!({ "tool": "util.sleep", "arguments": { "minMs": 600, "maxMs": 1400 } }),
        json!({ "tool": "ios.ui.source", "arguments": {}, "saveAs": "messagesUiSource" }),
        json!({
            "tool": "ios.ui.extract_rows",
            "arguments": {
                "source": "{{steps.messagesUiSource.source}}",
                "row": { "type": "XCUIElementTypeCell" },
                "primary": {
                    "type": "XCUIElementTypeStaticText",
                    "attr": "label",
                    "pick": "first"
                },
                "fields": [
                    {
                        "name": "preview",
                        "attr": "label",
                        "pick": "longest",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 6 }
                    },
                    {
                        "name": "timestamp",
                        "attr": "label",
                        "pick": "last",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 6 }
                    }
                ],
                "split": {
                    "fields": ["title"],
                    "skipMetricLike": false
                },
                "limit": "{{maxThreads}}"
            },
            "saveAs": "threads"
        }),
        json!({ "tool": "ios.ui.screenshot", "arguments": {}, "saveAs": "messagesScreenshot" }),
    ];

    let mut output = run_phone_steps(
        state,
        "phone_messages.list_recent_threads",
        steps,
        json!({
            "udid": device_id,
            "maxThreads": max_threads
        }),
        json!({
            "systemId": "phone_messages",
            "deviceId": "{{udid}}",
            "threads": "{{steps.threads.rows}}",
            "screenshot": "{{steps.messagesScreenshot}}"
        }),
        background_app_on_finish,
        lock_device_on_finish,
    )
    .await?;

    let normalized = normalize_message_threads(
        output
            .get("threads")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );

    if let Some(obj) = output.as_object_mut() {
        obj.insert("threads".to_string(), Value::Array(normalized.clone()));
        obj.insert("threadCount".to_string(), json!(normalized.len()));
    }

    let content = phone_tool_content(
        format!("listed {} message threads", normalized.len()),
        normalized.len(),
        "threads",
        output.get("screenshot"),
    );

    Ok(tool_success_with_content(output, content))
}

async fn phone_messages_read_latest_messages(state: &AppState, arguments: &Value) -> Result<Value> {
    let device_id = required_device_id(arguments)?;
    let thread_index = resolve_thread_index(arguments)?;
    let max_messages = bounded_usize_arg(arguments, "maxMessages", 20, 1, 50);
    let background_app_on_finish = bool_arg(arguments, "backgroundAppOnFinish", true);
    let lock_device_on_finish = bool_arg(arguments, "lockDeviceOnFinish", false);

    let steps = vec![
        json!({ "tool": "ios.appium.ensure", "arguments": {} }),
        json!({
            "tool": "ios.session.create",
            "arguments": {
                "udid": "{{udid}}",
                "kind": "native_app",
                "bundleId": "com.apple.MobileSMS",
                "replaceExisting": true,
                "noReset": true
            }
        }),
        json!({
            "tool": "ios.action.wait",
            "arguments": {
                "target": {
                    "using": "-ios predicate string",
                    "value": "type == 'XCUIElementTypeCell' OR label CONTAINS[c] 'Messages'"
                },
                "timeoutMs": 30000
            },
            "retries": 1
        }),
        json!({
            "tool": "ios.action.tap",
            "arguments": {
                "target": {
                    "using": "-ios predicate string",
                    "value": "type == 'XCUIElementTypeCell'",
                    "index": "{{threadIndex}}"
                }
            }
        }),
        json!({ "tool": "util.sleep", "arguments": { "minMs": 700, "maxMs": 1500 } }),
        json!({ "tool": "ios.ui.source", "arguments": {}, "saveAs": "threadUiSource" }),
        json!({
            "tool": "ios.ui.extract_rows",
            "arguments": {
                "source": "{{steps.threadUiSource.source}}",
                "row": { "type": "XCUIElementTypeCell" },
                "primary": {
                    "type": "XCUIElementTypeStaticText",
                    "attr": "label",
                    "pick": "longest"
                },
                "fields": [
                    {
                        "name": "senderCandidate",
                        "attr": "label",
                        "pick": "first",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 8 }
                    },
                    {
                        "name": "timestamp",
                        "attr": "label",
                        "pick": "last",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 8 }
                    }
                ],
                "split": {
                    "fields": ["body"],
                    "skipMetricLike": false
                },
                "limit": "{{maxMessages}}",
                "maxScrolls": 2,
                "scroll": { "direction": "up", "distance": 0.55, "settleMs": 400 }
            },
            "saveAs": "messages"
        }),
        json!({ "tool": "ios.ui.screenshot", "arguments": {}, "saveAs": "threadScreenshot" }),
    ];

    let mut output = run_phone_steps(
        state,
        "phone_messages.read_latest_messages",
        steps,
        json!({
            "udid": device_id,
            "threadIndex": thread_index,
            "maxMessages": max_messages
        }),
        json!({
            "systemId": "phone_messages",
            "deviceId": "{{udid}}",
            "threadIndex": "{{threadIndex}}",
            "messages": "{{steps.messages.rows}}",
            "screenshot": "{{steps.threadScreenshot}}"
        }),
        background_app_on_finish,
        lock_device_on_finish,
    )
    .await?;

    let thread_id = arguments
        .get("threadId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            stable_phone_item_id(
                "phone_messages-thread",
                &[thread_index.to_string()],
                thread_index + 1,
            )
        });
    let thread_title = format!("Thread {}", thread_index + 1);
    let normalized = normalize_thread_messages(
        output
            .get("messages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &thread_id,
        &thread_title,
    );

    if let Some(obj) = output.as_object_mut() {
        obj.insert(
            "thread".to_string(),
            json!({
                "thread_id": thread_id,
                "title": thread_title
            }),
        );
        obj.insert("messages".to_string(), Value::Array(normalized.clone()));
        obj.insert("messageCount".to_string(), json!(normalized.len()));
    }

    let content = phone_tool_content(
        format!("read {} messages", normalized.len()),
        normalized.len(),
        "messages",
        output.get("screenshot"),
    );

    Ok(tool_success_with_content(output, content))
}

async fn phone_messages_find_recent_otp(state: &AppState, arguments: &Value) -> Result<Value> {
    let device_id = required_device_id(arguments)?;
    let max_threads = bounded_usize_arg(arguments, "maxThreads", 5, 1, 20);
    let max_messages = bounded_usize_arg(arguments, "maxMessages", 8, 1, 50);
    let background_app_on_finish = bool_arg(arguments, "backgroundAppOnFinish", true);
    let lock_device_on_finish = bool_arg(arguments, "lockDeviceOnFinish", false);
    let thread_contains = optional_string_arg(arguments, &["threadContains", "thread_contains"]);
    let sender_contains = optional_string_arg(arguments, &["senderContains", "sender_contains"]);
    let message_contains = optional_string_arg(arguments, &["messageContains", "message_contains"]);
    let exact_code_length =
        optional_bounded_usize_arg(arguments, &["codeLength", "code_length"], 4, 8);

    let (min_code_length, max_code_length) = if let Some(exact) = exact_code_length {
        (exact, exact)
    } else {
        let min_len =
            optional_bounded_usize_arg(arguments, &["minCodeLength", "min_code_length"], 4, 8)
                .unwrap_or(4);
        let max_len =
            optional_bounded_usize_arg(arguments, &["maxCodeLength", "max_code_length"], 4, 8)
                .unwrap_or(8);
        if min_len > max_len {
            return Err(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                "'minCodeLength' must be <= 'maxCodeLength'",
                json!({"minCodeLength": min_len, "maxCodeLength": max_len}),
            )
            .into());
        }
        (min_len, max_len)
    };

    let list_result = phone_messages_list_recent_threads(
        state,
        &json!({
            "deviceId": device_id.clone(),
            "maxThreads": max_threads,
            "backgroundAppOnFinish": background_app_on_finish,
            "lockDeviceOnFinish": lock_device_on_finish
        }),
    )
    .await?;
    let list_structured = list_result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let list_screenshot = list_structured.get("screenshot").cloned();
    let threads = list_structured
        .get("threads")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let filtered_threads: Vec<Value> = threads
        .into_iter()
        .filter(|thread| {
            if let Some(filter) = thread_contains.as_deref() {
                otp_thread_matches_filter(thread, filter)
            } else {
                true
            }
        })
        .take(max_threads)
        .collect();

    let mut inspected_threads: Vec<Value> = Vec::new();
    let mut candidates: Vec<Value> = Vec::new();
    let mut seen_candidates = HashSet::<String>::new();
    let mut scanned_messages = 0usize;
    let mut successful_thread_reads = 0usize;
    let mut last_thread_error: Option<anyhow::Error> = None;
    let mut best_screenshot = list_screenshot;

    for thread in filtered_threads.iter() {
        let thread_index = thread
            .get("position")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .saturating_sub(1) as usize;
        let read_result = phone_messages_read_latest_messages(
            state,
            &json!({
                "deviceId": device_id.clone(),
                "threadId": thread.get("thread_id").cloned().unwrap_or(Value::Null),
                "threadIndex": thread_index,
                "maxMessages": max_messages,
                "backgroundAppOnFinish": background_app_on_finish,
                "lockDeviceOnFinish": lock_device_on_finish
            }),
        )
        .await;

        let read_result = match read_result {
            Ok(result) => result,
            Err(err) => {
                inspected_threads.push(json!({
                    "thread": thread,
                    "error": format!("{err:#}")
                }));
                last_thread_error = Some(err);
                continue;
            }
        };

        successful_thread_reads += 1;
        let read_structured = read_result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let thread_meta = read_structured
            .get("thread")
            .cloned()
            .unwrap_or_else(|| thread.clone());
        let messages = read_structured
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        scanned_messages += messages.len();

        let thread_candidates = extract_otp_candidates_from_messages(
            &thread_meta,
            &messages,
            sender_contains.as_deref(),
            message_contains.as_deref(),
            min_code_length,
            max_code_length,
        );
        if !thread_candidates.is_empty() {
            best_screenshot = read_structured
                .get("screenshot")
                .cloned()
                .or(best_screenshot);
        }

        let mut unique_thread_candidates = Vec::new();
        for candidate in thread_candidates {
            let dedupe_key = format!(
                "{}:{}",
                candidate
                    .get("message_id")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                candidate.get("code").and_then(Value::as_str).unwrap_or("")
            );
            if dedupe_key == ":" || !seen_candidates.insert(dedupe_key) {
                continue;
            }
            unique_thread_candidates.push(candidate.clone());
            candidates.push(candidate);
        }

        inspected_threads.push(json!({
            "thread": thread_meta,
            "messageCount": messages.len(),
            "candidateCount": unique_thread_candidates.len(),
            "bestCandidate": unique_thread_candidates.first().cloned().unwrap_or(Value::Null)
        }));
    }

    if successful_thread_reads == 0 && !filtered_threads.is_empty() {
        if let Some(err) = last_thread_error {
            return Err(err);
        }
    }

    rank_otp_candidates(&mut candidates);

    let best_candidate = candidates.first().cloned();
    let found = best_candidate.is_some();
    let mut output = json!({
        "ok": true,
        "systemId": "phone_messages",
        "deviceId": device_id,
        "search": {
            "maxThreads": max_threads,
            "maxMessages": max_messages,
            "threadContains": thread_contains,
            "senderContains": sender_contains,
            "messageContains": message_contains,
            "minCodeLength": min_code_length,
            "maxCodeLength": max_code_length
        },
        "threadCount": filtered_threads.len(),
        "scannedThreads": successful_thread_reads,
        "scannedMessages": scanned_messages,
        "found": found,
        "candidateCount": candidates.len(),
        "bestCandidate": best_candidate.clone().unwrap_or(Value::Null),
        "candidates": candidates,
        "inspectedThreads": inspected_threads,
        "screenshot": best_screenshot.unwrap_or(Value::Null)
    });

    if let Some(best) = best_candidate.as_ref() {
        if let Some(obj) = output.as_object_mut() {
            obj.insert(
                "thread".to_string(),
                json!({
                    "thread_id": best.get("thread_id").cloned().unwrap_or(Value::Null),
                    "title": best.get("thread_title").cloned().unwrap_or(Value::Null),
                    "position": best.get("thread_position").cloned().unwrap_or(Value::Null)
                }),
            );
        }
    }

    let message = if let Some(best) = best_candidate.as_ref() {
        let code = best
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let title = best
            .get("thread_title")
            .and_then(Value::as_str)
            .unwrap_or("Messages");
        format!(
            "found {} OTP candidates; best code {} in {}",
            output
                .get("candidateCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            code,
            title
        )
    } else {
        format!(
            "found 0 OTP candidates across {} threads",
            successful_thread_reads
        )
    };

    let mut content = vec![json!({ "type": "text", "text": message })];
    if let Some(block) = screenshot_to_content_block(output.get("screenshot")) {
        content.push(block);
    }

    Ok(tool_success_with_content(output, content))
}

async fn phone_calls_list_recent_calls(state: &AppState, arguments: &Value) -> Result<Value> {
    let device_id = required_device_id(arguments)?;
    let max_calls = bounded_usize_arg(arguments, "maxCalls", 25, 1, 50);
    let background_app_on_finish = bool_arg(arguments, "backgroundAppOnFinish", true);
    let lock_device_on_finish = bool_arg(arguments, "lockDeviceOnFinish", false);

    let steps = vec![
        json!({ "tool": "ios.appium.ensure", "arguments": {} }),
        json!({
            "tool": "ios.session.create",
            "arguments": {
                "udid": "{{udid}}",
                "kind": "native_app",
                "bundleId": "com.apple.mobilephone",
                "replaceExisting": true,
                "noReset": true
            }
        }),
        json!({
            "tool": "ios.action.wait",
            "arguments": {
                "target": {
                    "using": "-ios predicate string",
                    "value": "name CONTAINS[c] 'Recents' OR label CONTAINS[c] 'Recents' OR name CONTAINS[c] 'Phone' OR label CONTAINS[c] 'Phone'"
                },
                "timeoutMs": 30000
            },
            "retries": 1
        }),
        json!({
            "tool": "ios.action.tap",
            "arguments": {
                "target": {
                    "using": "-ios predicate string",
                    "value": "name CONTAINS[c] 'Recents' OR label CONTAINS[c] 'Recents'"
                }
            }
        }),
        json!({ "tool": "util.sleep", "arguments": { "minMs": 700, "maxMs": 1500 } }),
        json!({ "tool": "ios.ui.source", "arguments": {}, "saveAs": "callsUiSource" }),
        json!({
            "tool": "ios.ui.extract_rows",
            "arguments": {
                "source": "{{steps.callsUiSource.source}}",
                "row": { "type": "XCUIElementTypeCell" },
                "primary": {
                    "type": "XCUIElementTypeStaticText",
                    "attr": "label",
                    "pick": "first"
                },
                "fields": [
                    {
                        "name": "timestamp",
                        "attr": "label",
                        "pick": "last",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 6 }
                    },
                    {
                        "name": "summary",
                        "attr": "label",
                        "pick": "longest",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 6 }
                    }
                ],
                "split": {
                    "fields": ["contact"],
                    "skipMetricLike": false
                },
                "limit": "{{maxCalls}}"
            },
            "saveAs": "calls"
        }),
        json!({ "tool": "ios.ui.screenshot", "arguments": {}, "saveAs": "callsScreenshot" }),
    ];

    let mut output = run_phone_steps(
        state,
        "phone_calls.list_recent_calls",
        steps,
        json!({
            "udid": device_id,
            "maxCalls": max_calls
        }),
        json!({
            "systemId": "phone_calls",
            "deviceId": "{{udid}}",
            "calls": "{{steps.calls.rows}}",
            "screenshot": "{{steps.callsScreenshot}}"
        }),
        background_app_on_finish,
        lock_device_on_finish,
    )
    .await?;

    let normalized = normalize_recent_calls(
        output
            .get("calls")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );

    if let Some(obj) = output.as_object_mut() {
        obj.insert("calls".to_string(), Value::Array(normalized.clone()));
        obj.insert("callCount".to_string(), json!(normalized.len()));
    }

    let content = phone_tool_content(
        format!("listed {} recent calls", normalized.len()),
        normalized.len(),
        "calls",
        output.get("screenshot"),
    );

    Ok(tool_success_with_content(output, content))
}

async fn phone_notifications_list_recent_notifications(
    state: &AppState,
    arguments: &Value,
) -> Result<Value> {
    let output = collect_recent_notifications(state, arguments).await?;
    let count = output
        .get("notifications")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);

    let content = phone_tool_content(
        format!("listed {} notifications", count),
        count,
        "notifications",
        output.get("screenshot"),
    );

    Ok(tool_success_with_content(output, content))
}

async fn phone_notifications_filter_notifications_by_app(
    state: &AppState,
    arguments: &Value,
) -> Result<Value> {
    let app_label = required_app_label(arguments)?;
    let mut output = collect_recent_notifications(state, arguments).await?;
    let notifications = output
        .get("notifications")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let lowered = app_label.to_lowercase();
    let matches: Vec<Value> = notifications
        .into_iter()
        .filter(|item| {
            item.get("app_name")
                .and_then(Value::as_str)
                .map(|value| value.to_lowercase().contains(&lowered))
                .unwrap_or(false)
        })
        .collect();

    if let Some(obj) = output.as_object_mut() {
        obj.insert("appLabel".to_string(), json!(app_label));
        obj.insert("matches".to_string(), Value::Array(matches.clone()));
        obj.insert("matchCount".to_string(), json!(matches.len()));
    }

    let content = phone_tool_content(
        format!("matched {} notifications for {}", matches.len(), app_label),
        matches.len(),
        "matches",
        output.get("screenshot"),
    );

    Ok(tool_success_with_content(output, content))
}

async fn collect_recent_notifications(state: &AppState, arguments: &Value) -> Result<Value> {
    let device_id = required_device_id(arguments)?;
    let max_notifications = bounded_usize_arg(arguments, "maxNotifications", 25, 1, 50);
    let background_app_on_finish = bool_arg(arguments, "backgroundAppOnFinish", false);
    let lock_device_on_finish = bool_arg(arguments, "lockDeviceOnFinish", false);

    let steps = vec![
        json!({ "tool": "ios.appium.ensure", "arguments": {} }),
        json!({
            "tool": "ios.session.create",
            "arguments": {
                "udid": "{{udid}}",
                "kind": "native_app",
                "bundleId": "com.apple.springboard",
                "replaceExisting": true,
                "noReset": true
            }
        }),
        json!({
            "tool": "ios.action.scroll",
            "arguments": {
                "direction": "down",
                "distance": 0.85
            }
        }),
        json!({ "tool": "util.sleep", "arguments": { "minMs": 900, "maxMs": 1700 } }),
        json!({ "tool": "ios.ui.source", "arguments": {}, "saveAs": "notificationUiSource" }),
        json!({
            "tool": "ios.ui.extract_rows",
            "arguments": {
                "source": "{{steps.notificationUiSource.source}}",
                "row": { "type": "XCUIElementTypeCell" },
                "primary": {
                    "type": "XCUIElementTypeStaticText",
                    "attr": "label",
                    "pick": "first"
                },
                "fields": [
                    {
                        "name": "body",
                        "attr": "label",
                        "pick": "longest",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 8 }
                    },
                    {
                        "name": "timestamp",
                        "attr": "label",
                        "pick": "last",
                        "query": { "type": "XCUIElementTypeStaticText", "max": 8 }
                    }
                ],
                "split": {
                    "fields": ["app_name"],
                    "skipMetricLike": false
                },
                "limit": "{{maxNotifications}}"
            },
            "saveAs": "notifications"
        }),
        json!({ "tool": "ios.ui.screenshot", "arguments": {}, "saveAs": "notificationsScreenshot" }),
    ];

    let mut output = run_phone_steps(
        state,
        "phone_notifications.list_recent_notifications",
        steps,
        json!({
            "udid": device_id,
            "maxNotifications": max_notifications
        }),
        json!({
            "systemId": "phone_notifications",
            "deviceId": "{{udid}}",
            "notifications": "{{steps.notifications.rows}}",
            "screenshot": "{{steps.notificationsScreenshot}}"
        }),
        background_app_on_finish,
        lock_device_on_finish,
    )
    .await?;

    let normalized = normalize_recent_notifications(
        output
            .get("notifications")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );

    if let Some(obj) = output.as_object_mut() {
        obj.insert(
            "notifications".to_string(),
            Value::Array(normalized.clone()),
        );
        obj.insert("notificationCount".to_string(), json!(normalized.len()));
    }

    Ok(output)
}

async fn run_phone_steps(
    state: &AppState,
    tool_name: &str,
    steps: Vec<Value>,
    vars: Value,
    output_template: Value,
    background_app_on_finish: bool,
    lock_device_on_finish: bool,
) -> Result<Value> {
    let output_result = run_steps(state, &steps, false, &vars, Some(&output_template), None).await;

    if let Err(err) = &output_result {
        let artifacts = capture_failure_artifacts(state)
            .await
            .unwrap_or_else(|_| json!({}));
        let _ = worker_shutdown(
            state,
            &json!({
                "stopAppium": false,
                "shutdownWDA": true,
                "backgroundApp": background_app_on_finish,
                "lockDevice": lock_device_on_finish
            }),
        )
        .await;
        let message = format!("{tool_name} failed: {err:#}");
        return Err(ToolCallError::new(
            classify_tool_error_code(&message),
            message,
            json!({
                "tool": tool_name,
                "artifacts": artifacts
            }),
        )
        .into());
    }

    let _ = worker_shutdown(
        state,
        &json!({
            "stopAppium": false,
            "shutdownWDA": true,
            "backgroundApp": background_app_on_finish,
            "lockDevice": lock_device_on_finish
        }),
    )
    .await;

    let output = output_result.expect("output_result already checked");
    if output.get("ok").and_then(Value::as_bool) == Some(false) {
        let message = output
            .get("error")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("{tool_name} failed"));
        return Err(ToolCallError::new(
            tool_error_code_from_value(output.get("errorCode")),
            message,
            json!({
                "tool": tool_name,
                "output": output
            }),
        )
        .into());
    }

    Ok(output)
}

fn phone_tool_content(
    message: String,
    count: usize,
    noun: &str,
    screenshot: Option<&Value>,
) -> Vec<Value> {
    let mut content = vec![json!({
        "type": "text",
        "text": format!("{message} ({count} {noun})")
    })];
    if let Some(block) = screenshot_to_content_block(screenshot) {
        content.push(block);
    }
    content
}

fn screenshot_to_content_block(screenshot: Option<&Value>) -> Option<Value> {
    let shot = screenshot?;
    let data = shot.get("data")?.as_str()?;
    if data.trim().is_empty() {
        return None;
    }
    Some(json!({
        "type": "image",
        "mimeType": shot.get("mimeType").and_then(Value::as_str).unwrap_or("image/png"),
        "data": data
    }))
}

fn normalize_message_threads(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let position = idx + 1;
            let title = row_text(row, &["title", "name", "rawLabel"])
                .unwrap_or_else(|| format!("Thread {position}"));
            let preview = row_text(row, &["preview", "subtitle"])
                .filter(|value| !strings_eq_ci(value, &title));
            let timestamp = row_text(row, &["timestamp"]);
            json!({
                "thread_id": stable_phone_item_id("phone_messages-thread", &[title.clone(), timestamp.clone().unwrap_or_default()], position),
                "title": title,
                "preview": preview,
                "timestamp": timestamp,
                "position": position,
                "raw_label": row_text(row, &["rawLabel"])
            })
        })
        .collect()
}

fn normalize_thread_messages(rows: &[Value], thread_id: &str, thread_title: &str) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let position = idx + 1;
            let body = row_text(row, &["body", "rawLabel"])
                .unwrap_or_else(|| format!("Message {position}"));
            let sender = row_text(row, &["senderCandidate"])
                .filter(|value| !strings_eq_ci(value, &body))
                .filter(|value| !looks_like_timestamp(value));
            let sent_at = row_text(row, &["timestamp"]).filter(|value| !strings_eq_ci(value, &body));
            json!({
                "message_id": stable_phone_item_id("phone_messages-message", &[thread_id.to_string(), body.clone(), sent_at.clone().unwrap_or_default()], position),
                "thread_id": thread_id,
                "thread_title": thread_title,
                "body": body,
                "sender": sender,
                "direction": Value::Null,
                "sent_at": sent_at,
                "position": position,
                "raw_label": row_text(row, &["rawLabel"])
            })
        })
        .collect()
}

fn normalize_recent_calls(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let position = idx + 1;
            let contact = row_text(row, &["contact", "name", "rawLabel"])
                .unwrap_or_else(|| format!("Call {position}"));
            let summary = row_text(row, &["summary", "rawLabel"]).unwrap_or_else(|| contact.clone());
            let timestamp = row_text(row, &["timestamp"]);
            let phone_number = extract_phone_number_candidate(&summary).or_else(|| extract_phone_number_candidate(&contact));
            let call_type = infer_call_type(&summary);
            json!({
                "call_id": stable_phone_item_id("phone_calls-call", &[contact.clone(), timestamp.clone().unwrap_or_default()], position),
                "contact": contact,
                "summary": summary,
                "phone_number": phone_number,
                "call_type": call_type,
                "timestamp": timestamp,
                "position": position,
                "raw_label": row_text(row, &["rawLabel"])
            })
        })
        .collect()
}

fn normalize_recent_notifications(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let position = idx + 1;
            let app_name = row_text(row, &["app_name", "name", "rawLabel"])
                .unwrap_or_else(|| format!("Notification {position}"));
            let body = row_text(row, &["body"])
                .filter(|value| !strings_eq_ci(value, &app_name))
                .or_else(|| row_text(row, &["rawLabel"]).filter(|value| !strings_eq_ci(value, &app_name)));
            let timestamp = row_text(row, &["timestamp"]);
            json!({
                "notification_id": stable_phone_item_id("phone_notifications-item", &[app_name.clone(), body.clone().unwrap_or_default(), timestamp.clone().unwrap_or_default()], position),
                "app_name": app_name,
                "body": body,
                "timestamp": timestamp,
                "position": position,
                "raw_label": row_text(row, &["rawLabel"])
            })
        })
        .collect()
}

fn optional_string_arg(arguments: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        arguments
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn optional_bounded_usize_arg(
    arguments: &Value,
    keys: &[&str],
    min: usize,
    max: usize,
) -> Option<usize> {
    keys.iter().find_map(|key| {
        arguments
            .get(*key)
            .and_then(Value::as_u64)
            .map(|value| (value as usize).clamp(min, max))
    })
}

fn otp_thread_matches_filter(thread: &Value, filter: &str) -> bool {
    row_text(thread, &["title"])
        .map(|value| string_contains_ci(&value, filter))
        .unwrap_or(false)
        || row_text(thread, &["preview"])
            .map(|value| string_contains_ci(&value, filter))
            .unwrap_or(false)
}

fn extract_otp_candidates_from_messages(
    thread: &Value,
    messages: &[Value],
    sender_contains: Option<&str>,
    message_contains: Option<&str>,
    min_code_length: usize,
    max_code_length: usize,
) -> Vec<Value> {
    static OTP_CODE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?:^|[^\d])(\d{4,8})(?:[^\d]|$)").expect("otp regex"));
    static AUTH_HINT_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)\b(otp|one[- ]time|verification|verify|passcode|security code|login code|auth(?:entication)? code|2fa|do not share|expires?)\b",
        )
        .expect("auth hint regex")
    });

    let thread_id =
        row_text(thread, &["thread_id"]).unwrap_or_else(|| "phone_messages-thread".to_string());
    let thread_title = row_text(thread, &["title"]).unwrap_or_else(|| "Messages".to_string());
    let thread_position = thread.get("position").and_then(Value::as_u64).unwrap_or(0) as usize;
    let thread_preview = row_text(thread, &["preview"]).unwrap_or_default();
    let sender_filter_matches_thread = sender_contains
        .map(|filter| {
            string_contains_ci(&thread_title, filter) || string_contains_ci(&thread_preview, filter)
        })
        .unwrap_or(false);

    let mut out = Vec::new();
    for message in messages {
        let body = row_text(message, &["body", "raw_label", "rawLabel"]).unwrap_or_default();
        if body.is_empty() {
            continue;
        }
        if let Some(filter) = message_contains {
            if !string_contains_ci(&body, filter) {
                continue;
            }
        }

        let sender = row_text(message, &["sender", "senderCandidate"]);
        if let Some(filter) = sender_contains {
            let sender_matches = sender
                .as_deref()
                .map(|value| string_contains_ci(value, filter))
                .unwrap_or(false)
                || sender_filter_matches_thread;
            if !sender_matches {
                continue;
            }
        }

        let has_auth_hint = AUTH_HINT_RE.is_match(&body)
            || AUTH_HINT_RE.is_match(&thread_title)
            || AUTH_HINT_RE.is_match(&thread_preview);
        let message_position =
            message.get("position").and_then(Value::as_u64).unwrap_or(0) as usize;
        let sent_at = row_text(message, &["sent_at", "timestamp"]);
        let message_id = row_text(message, &["message_id"]).unwrap_or_else(|| {
            stable_phone_item_id(
                "phone_messages-message",
                &[thread_id.clone(), body.clone()],
                message_position.max(1),
            )
        });

        for capture in OTP_CODE_RE.captures_iter(&body) {
            let Some(matched) = capture.get(1) else {
                continue;
            };
            let code = matched.as_str();
            if code.len() < min_code_length || code.len() > max_code_length {
                continue;
            }

            let mut score = 0i64;
            let mut reasons = Vec::<String>::new();

            if has_auth_hint {
                score += 60;
                reasons.push("auth_hint".to_string());
            }
            if sender_filter_matches_thread {
                score += 20;
                reasons.push("thread_sender_match".to_string());
            }
            if let Some(filter) = sender_contains {
                if sender
                    .as_deref()
                    .map(|value| string_contains_ci(value, filter))
                    .unwrap_or(false)
                {
                    score += 25;
                    reasons.push("sender_match".to_string());
                }
            }
            if let Some(filter) = message_contains {
                if string_contains_ci(&body, filter) {
                    score += 15;
                    reasons.push("message_match".to_string());
                }
            }
            if code.len() == 6 {
                score += 10;
                reasons.push("common_length".to_string());
            }
            if thread_position > 0 {
                score += (20usize.saturating_sub(thread_position.saturating_sub(1) * 3)) as i64;
            }
            score += message_position.min(20) as i64;

            out.push(json!({
                "code": code,
                "code_length": code.len(),
                "score": score,
                "reasons": reasons,
                "thread_id": thread_id.clone(),
                "thread_title": thread_title.clone(),
                "thread_position": thread_position,
                "message_id": message_id.clone(),
                "message_position": message_position,
                "message_body": body.clone(),
                "message_excerpt": build_message_excerpt(&body, matched.start(), matched.end()),
                "sender": sender.clone(),
                "sent_at": sent_at.clone()
            }));
        }
    }

    out
}

fn rank_otp_candidates(candidates: &mut [Value]) {
    candidates.sort_by(|left, right| {
        let left_score = left.get("score").and_then(Value::as_i64).unwrap_or(0);
        let right_score = right.get("score").and_then(Value::as_i64).unwrap_or(0);
        let left_thread = left
            .get("thread_position")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let right_thread = right
            .get("thread_position")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let left_message = left
            .get("message_position")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let right_message = right
            .get("message_position")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        right_score
            .cmp(&left_score)
            .then(left_thread.cmp(&right_thread))
            .then(right_message.cmp(&left_message))
    });

    for (idx, candidate) in candidates.iter_mut().enumerate() {
        if let Some(obj) = candidate.as_object_mut() {
            obj.insert("rank".to_string(), json!(idx + 1));
        }
    }
}

fn build_message_excerpt(body: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = body.chars().collect();
    let char_count = chars.len();
    let start_idx = body[..start].chars().count();
    let end_idx = body[..end].chars().count();
    let window_start = start_idx.saturating_sub(24);
    let window_end = (end_idx + 24).min(char_count);
    let mut excerpt: String = chars[window_start..window_end].iter().collect();
    if window_start > 0 {
        excerpt = format!("...{excerpt}");
    }
    if window_end < char_count {
        excerpt.push_str("...");
    }
    excerpt
}

fn string_contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn required_device_id(arguments: &Value) -> Result<String> {
    for key in ["deviceId", "device_id", "udid"] {
        if let Some(value) = arguments.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    Err(ToolCallError::new(
        ToolErrorCode::InvalidParams,
        "'deviceId' (or 'udid') is required",
        json!({"param": "deviceId"}),
    )
    .into())
}

fn required_app_label(arguments: &Value) -> Result<String> {
    for key in ["appLabel", "app_label", "appPackage", "app_package"] {
        if let Some(value) = arguments.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    Err(ToolCallError::new(
        ToolErrorCode::InvalidParams,
        "'appLabel' is required",
        json!({"param": "appLabel"}),
    )
    .into())
}

fn resolve_thread_index(arguments: &Value) -> Result<usize> {
    if let Some(value) = arguments.get("threadIndex").and_then(Value::as_u64) {
        return Ok(value as usize);
    }
    if let Some(value) = arguments.get("thread_index").and_then(Value::as_u64) {
        return Ok(value as usize);
    }
    if let Some(value) = arguments.get("threadId").and_then(Value::as_str) {
        if let Some(index) = parse_position_from_stable_id(value) {
            return Ok(index.saturating_sub(1));
        }
    }
    Ok(0)
}

fn bounded_usize_arg(
    arguments: &Value,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> usize {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
        .clamp(min, max)
}

fn bool_arg(arguments: &Value, key: &str, default: bool) -> bool {
    arguments
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn row_text(row: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        row.get(*key)
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string()))
    })
}

fn stable_phone_item_id(prefix: &str, parts: &[String], position: usize) -> String {
    let mut slug_parts = Vec::new();
    for part in parts {
        let slug = slugify_fragment(part);
        if !slug.is_empty() {
            slug_parts.push(slug);
        }
    }
    if slug_parts.is_empty() {
        format!("{prefix}-{position}")
    } else {
        format!("{prefix}-{position}-{}", slug_parts.join("-"))
    }
}

fn parse_position_from_stable_id(value: &str) -> Option<usize> {
    let mut seen_numeric = None;
    for part in value.split('-') {
        if let Ok(parsed) = part.parse::<usize>() {
            seen_numeric = Some(parsed);
            break;
        }
    }
    seen_numeric
}

fn slugify_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn extract_phone_number_candidate(value: &str) -> Option<String> {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || (ch == '+' && out.is_empty()) {
            out.push(ch);
        }
    }
    if out.chars().filter(|ch| ch.is_ascii_digit()).count() >= 7 {
        Some(out)
    } else {
        None
    }
}

fn infer_call_type(value: &str) -> Option<String> {
    let lower = value.to_lowercase();
    if lower.contains("missed") {
        Some("missed".to_string())
    } else if lower.contains("outgoing") {
        Some("outgoing".to_string())
    } else if lower.contains("incoming") {
        Some("incoming".to_string())
    } else if lower.contains("voicemail") {
        Some("voicemail".to_string())
    } else {
        None
    }
}

fn looks_like_timestamp(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("am")
        || lower.contains("pm")
        || lower.contains("today")
        || lower.contains("yesterday")
        || lower.contains("mon")
        || lower.contains("tue")
        || lower.contains("wed")
        || lower.contains("thu")
        || lower.contains("fri")
        || lower.contains("sat")
        || lower.contains("sun")
}

fn strings_eq_ci(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn classify_tool_error_code(message: &str) -> ToolErrorCode {
    let lowered = message.to_lowercase();
    if lowered.contains("device was not, or could not be, unlocked")
        || lowered.contains("could not be unlocked")
        || lowered.contains("bserrorcodedescription=locked")
        || lowered.contains(" for reason: locked")
    {
        ToolErrorCode::DeviceLocked
    } else if lowered.contains("timeout") {
        ToolErrorCode::Timeout
    } else if lowered.contains("no active session")
        || lowered.contains("sessionid is required")
        || lowered.contains("appium is not initialized")
    {
        ToolErrorCode::NoSession
    } else if lowered.contains("no elements found") || lowered.contains("no matching elements") {
        ToolErrorCode::ElementNotFound
    } else if lowered.contains("expected exactly one match")
        || lowered.contains("multiple matching elements")
        || lowered.contains("ambiguous")
    {
        ToolErrorCode::AmbiguousMatch
    } else if lowered.contains("required") || lowered.contains("invalid params") {
        ToolErrorCode::InvalidParams
    } else {
        ToolErrorCode::ActionFailed
    }
}

fn tool_error_code_from_value(value: Option<&Value>) -> ToolErrorCode {
    match value.and_then(Value::as_str).unwrap_or("") {
        "NO_SESSION" => ToolErrorCode::NoSession,
        "DEVICE_LOCKED" => ToolErrorCode::DeviceLocked,
        "INVALID_PARAMS" => ToolErrorCode::InvalidParams,
        "ELEMENT_NOT_FOUND" => ToolErrorCode::ElementNotFound,
        "AMBIGUOUS_MATCH" => ToolErrorCode::AmbiguousMatch,
        "TIMEOUT" => ToolErrorCode::Timeout,
        "COMMIT_REQUIRED" => ToolErrorCode::CommitRequired,
        "POLICY_DENIED" => ToolErrorCode::PolicyDenied,
        "NOT_SUPPORTED" => ToolErrorCode::NotSupported,
        "INTERNAL" => ToolErrorCode::Internal,
        _ => ToolErrorCode::ActionFailed,
    }
}

fn workflow_tool_may_mutate(tool: &str) -> bool {
    tool.starts_with("ios.action.")
        || tool == "ios.app.activate"
        || tool == "ios.appium.ensure"
        || tool == "ios.session.create"
        || tool == "ios.session.delete"
        || tool == "ios.wda.shutdown"
        || tool == "ios.web.goto"
        || tool == "ios.web.click_css"
        || tool == "ios.web.type_css"
        || tool == "ios.web.press_key"
        || tool == "ios.web.eval_js"
        || tool == "ios.alert.accept"
        || tool == "ios.alert.dismiss"
}

fn workflow_error_parts(
    err: &anyhow::Error,
    tool: &str,
    possibly_applied_on_timeout: bool,
) -> (String, Value, Value) {
    let tool_error = tool_error_from_anyhow(err, tool);
    let structured = tool_error
        .get("structuredContent")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let error_code = structured.get("errorCode").cloned().unwrap_or(Value::Null);
    let error_message = structured
        .get("error")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{err:#}"));
    let mut error_details = structured
        .get("details")
        .cloned()
        .unwrap_or_else(|| json!({ "tool": tool }));

    if possibly_applied_on_timeout && error_code.as_str() == Some(ToolErrorCode::Timeout.as_str()) {
        if let Some(details) = error_details.as_object_mut() {
            details.insert("possiblyApplied".to_string(), json!(true));
        } else {
            error_details = json!({
                "tool": tool,
                "details": error_details,
                "possiblyApplied": true
            });
        }
    }

    (error_message, error_code, error_details)
}

async fn run_steps(
    state: &AppState,
    steps: &[Value],
    commit: bool,
    vars: &Value,
    output_template: Option<&Value>,
    presentation_template: Option<&Value>,
) -> Result<Value> {
    let mut trace: Vec<Value> = Vec::with_capacity(steps.len());
    let mut vars = vars.clone();
    ensure_workflow_steps_var(&mut vars);

    for (idx, step) in steps.iter().enumerate() {
        let Some(obj) = step.as_object() else {
            bail!("step {idx} must be an object");
        };

        let step_id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        let tool = obj
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("step {idx} missing tool"))?;

        if let Some(when_value) = obj.get("when") {
            let rendered_when = substitute_vars(when_value.clone(), &vars);
            if !eval_when(&rendered_when, &vars)? {
                if let Some(save_as) = step_save_as(obj) {
                    store_step_output(&mut vars, &save_as, Value::Null);
                }
                trace.push(json!({
                    "step": idx + 1,
                    "stepId": step_id.clone(),
                    "tool": tool,
                    "attempt": 0,
                    "ok": true,
                    "skipped": true,
                    "durationMs": 0
                }));
                continue;
            }
        }

        if tool == "ios.script.run" || tool == "ios.workflow.run" {
            bail!("step {idx} tool '{tool}' is not allowed");
        }

        let requires_commit = obj
            .get("requiresCommit")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("requires_commit").and_then(Value::as_bool))
            .unwrap_or(false);
        if requires_commit && !commit {
            let message = format!("step {idx} requires commit=true (tool={tool})");
            trace.push(json!({
                "step": idx + 1,
                "stepId": step_id.clone(),
                "tool": tool,
                "attempt": 0,
                "ok": false,
                "durationMs": 0,
                "error": message,
                "errorCode": ToolErrorCode::CommitRequired.as_str(),
                "errorDetails": {"tool": tool, "step": idx + 1}
            }));
            return Ok(json!({
                "ok": false,
                "failedStep": idx + 1,
                "failedStepId": step_id,
                "error": message,
                "errorCode": ToolErrorCode::CommitRequired.as_str(),
                "trace": trace
            }));
        }

        if let Err(err) = tool_policy::enforce_workflow_tool_policy(tool, &vars) {
            let message = err.message;
            trace.push(json!({
                "step": idx + 1,
                "stepId": step_id.clone(),
                "tool": tool,
                "attempt": 0,
                "ok": false,
                "durationMs": 0,
                "error": message,
                "errorCode": err.code.as_str(),
                "errorDetails": err.details
            }));
            return Ok(json!({
                "ok": false,
                "failedStep": idx + 1,
                "failedStepId": step_id,
                "error": message,
                "errorCode": err.code.as_str(),
                "trace": trace
            }));
        }

        let retries = obj
            .get("retries")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 10) as u32;
        let timeout_ms = obj
            .get("timeoutMs")
            .and_then(Value::as_u64)
            .or_else(|| obj.get("timeout_ms").and_then(Value::as_u64))
            .unwrap_or(120_000)
            .clamp(250, 600_000);

        let raw_args = obj
            .get("arguments")
            .cloned()
            .or_else(|| obj.get("args").cloned())
            .unwrap_or_else(|| json!({}));
        let args = substitute_vars(raw_args, &vars);

        let max_attempts = retries.saturating_add(1);
        let possibly_applied_on_timeout = requires_commit || workflow_tool_may_mutate(tool);
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            let attempt_started = tokio::time::Instant::now();
            let call_fut = handle_tool_call_unchecked(state, tool, args.clone());
            let call =
                tokio::time::timeout(Duration::from_millis(timeout_ms), Box::pin(call_fut)).await;

            match call {
                Ok(Ok(result)) => {
                    if let Some(save_as) = step_save_as(obj) {
                        let stored = result
                            .get("structuredContent")
                            .cloned()
                            .unwrap_or_else(|| result.clone());
                        store_step_output(&mut vars, &save_as, stored);
                    }
                    trace.push(json!({
                        "step": idx + 1,
                        "stepId": step_id.clone(),
                        "tool": tool,
                        "attempt": attempt,
                        "ok": true,
                        "durationMs": attempt_started.elapsed().as_millis(),
                        "result": result
                    }));
                    break;
                }
                Ok(Err(err)) => {
                    let (error_message, error_code, error_details) =
                        workflow_error_parts(&err, tool, possibly_applied_on_timeout);
                    trace.push(json!({
                        "step": idx + 1,
                        "stepId": step_id.clone(),
                        "tool": tool,
                        "attempt": attempt,
                        "ok": false,
                        "durationMs": attempt_started.elapsed().as_millis(),
                        "error": error_message,
                        "errorCode": error_code,
                        "errorDetails": error_details,
                        "willRetry": attempt < max_attempts
                    }));
                    if attempt >= max_attempts {
                        let artifacts = capture_failure_artifacts(state)
                            .await
                            .unwrap_or_else(|_| json!({}));
                        return Ok(json!({
                            "ok": false,
                            "failedStep": idx + 1,
                            "failedStepId": step_id,
                            "error": error_message,
                            "errorCode": error_code,
                            "artifacts": artifacts,
                            "trace": trace
                        }));
                    }
                }
                Err(_) => {
                    let err = anyhow!("timeout after {timeout_ms}ms");
                    let (error_message, error_code, error_details) =
                        workflow_error_parts(&err, tool, possibly_applied_on_timeout);
                    trace.push(json!({
                        "step": idx + 1,
                        "stepId": step_id.clone(),
                        "tool": tool,
                        "attempt": attempt,
                        "ok": false,
                        "durationMs": attempt_started.elapsed().as_millis(),
                        "error": error_message,
                        "errorCode": error_code,
                        "errorDetails": error_details,
                        "willRetry": attempt < max_attempts
                    }));
                    if attempt >= max_attempts {
                        let artifacts = capture_failure_artifacts(state)
                            .await
                            .unwrap_or_else(|_| json!({}));
                        return Ok(json!({
                            "ok": false,
                            "failedStep": idx + 1,
                            "failedStepId": step_id,
                            "error": error_message,
                            "errorCode": error_code,
                            "artifacts": artifacts,
                            "trace": trace
                        }));
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    let mut output = if let Some(template) = output_template {
        render_workflow_output(
            template,
            presentation_template,
            &vars,
            steps.len(),
            trace.clone(),
        )
    } else {
        json!({
            "ok": true,
            "steps": steps.len(),
            "trace": trace
        })
    };

    if output_template.is_none() {
        if let Some(presentation) = presentation_template {
            if let Some(obj) = output.as_object_mut() {
                obj.insert(
                    "_presentation".to_string(),
                    substitute_vars(presentation.clone(), &vars),
                );
            }
        }
    }

    if let Some(obj) = output.as_object_mut() {
        obj.entry("ok".to_string()).or_insert_with(|| json!(true));
    }

    Ok(output)
}

async fn capture_failure_artifacts(state: &AppState) -> Result<Value> {
    let policy = FailureArtifactPolicy::from_env();
    if policy == FailureArtifactPolicy::Off {
        return Ok(json!({"policy": policy.as_str()}));
    }

    let Some(session) = state.active_session().await else {
        return Ok(json!({"policy": policy.as_str()}));
    };

    if policy == FailureArtifactPolicy::Minimal {
        return Ok(json!({
            "policy": policy.as_str(),
            "redacted": true,
            "sessionKind": session.kind
        }));
    }

    let driver = driver_from_state(state).await?;
    let mut out = serde_json::Map::new();
    out.insert("policy".to_string(), json!(policy.as_str()));

    if policy.captures_screenshot() {
        if let Ok(png_b64) = driver.screenshot(&session.session_id).await {
            out.insert(
                "screenshot".to_string(),
                json!({"mimeType": "image/png", "data": png_b64}),
            );
        }
    }

    if !policy.captures_ui_source() {
        return Ok(Value::Object(out));
    }

    let source_result = if session.kind == "native_app" {
        fetch_native_ui_source(&driver, &session.session_id).await
    } else {
        driver.page_source(&session.session_id).await
    };
    if let Ok(source) = source_result {
        let truncated = source.len() > 50_000;
        let slice = if truncated {
            source.chars().take(50_000).collect::<String>()
        } else {
            source
        };
        out.insert(
            "uiSource".to_string(),
            json!({"length": slice.len(), "truncated": truncated, "source": slice}),
        );
    }

    Ok(Value::Object(out))
}

async fn util_rank_by_name(arguments: &Value) -> Result<Value> {
    let items = arguments
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("items must be an array"))?;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .unwrap_or("name");
    let target = required_str(arguments, "target")?;
    let want = normalize_match_key(target);

    let mut rank: Option<usize> = None;
    for (idx, item) in items.iter().enumerate() {
        let Some(value) = item.get(field).and_then(Value::as_str) else {
            continue;
        };
        let candidate = normalize_match_key(value);
        if candidate == want {
            rank = Some(idx + 1);
            break;
        }
    }
    if rank.is_none() && !want.is_empty() {
        for (idx, item) in items.iter().enumerate() {
            let Some(value) = item.get(field).and_then(Value::as_str) else {
                continue;
            };
            let candidate = normalize_match_key(value);
            if candidate.contains(&want) || want.contains(&candidate) {
                rank = Some(idx + 1);
                break;
            }
        }
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "rank": rank,
            "index": rank.map(|value| value.saturating_sub(1))
        }),
        "rank computed",
    ))
}

async fn util_list_length(arguments: &Value) -> Result<Value> {
    let list = arguments
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("list must be an array"))?;

    Ok(tool_success(
        json!({ "ok": true, "count": list.len() }),
        "length computed",
    ))
}

async fn util_list_first(arguments: &Value) -> Result<Value> {
    let list = arguments
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("list must be an array"))?;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim);

    let value = list.first().cloned().unwrap_or(Value::Null);
    let extracted = if let Some(field) = field.filter(|f| !f.is_empty()) {
        value.get(field).cloned().unwrap_or(Value::Null)
    } else {
        value
    };

    Ok(tool_success(
        json!({ "ok": true, "found": !list.is_empty(), "value": extracted }),
        "first item selected",
    ))
}

async fn util_list_nth(arguments: &Value) -> Result<Value> {
    let list = arguments
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("list must be an array"))?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("index must be an integer >= 1"))? as usize;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim);

    let found = index > 0 && index <= list.len();
    let value = if found {
        list.get(index - 1).cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let extracted = if let Some(field) = field.filter(|f| !f.is_empty()) {
        value.get(field).cloned().unwrap_or(Value::Null)
    } else {
        value
    };

    Ok(tool_success(
        json!({ "ok": true, "index": index, "found": found, "value": extracted }),
        "nth item selected",
    ))
}

async fn util_list_find(arguments: &Value) -> Result<Value> {
    let list = arguments
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("list must be an array"))?;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let match_query = parse_string_match_query(arguments, "util.list.find")?;
    let start_offset = arguments
        .get("startOffset")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;

    for (offset, item) in list.iter().enumerate().skip(start_offset) {
        let candidate = if let Some(field_name) = field {
            item.get(field_name).and_then(Value::as_str)
        } else {
            item.as_str()
        };
        let Some(candidate) = candidate else {
            continue;
        };

        if !matches_string_query(candidate, &match_query) {
            continue;
        }

        return Ok(tool_success(
            json!({
                "ok": true,
                "found": true,
                "index": offset + 1,
                "zeroBasedIndex": offset,
                "matchedText": candidate,
                "value": item
            }),
            "matching item selected",
        ));
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "found": false,
            "index": Value::Null,
            "zeroBasedIndex": Value::Null,
            "matchedText": Value::Null,
            "value": Value::Null
        }),
        "no matching item found",
    ))
}

async fn util_rect_relative_point(arguments: &Value) -> Result<Value> {
    fn num_arg(arguments: &Value, key: &str) -> Result<f64> {
        arguments
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("{key} must be a number"))
    }

    let x = num_arg(arguments, "x")?;
    let y = num_arg(arguments, "y")?;
    let width = num_arg(arguments, "width")?;
    let height = num_arg(arguments, "height")?;
    let relative_x = arguments
        .get("relativeX")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let relative_y = arguments
        .get("relativeY")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    Ok(tool_success(
        json!({
            "ok": true,
            "x": x + (width * relative_x),
            "y": y + (height * relative_y)
        }),
        "relative point computed",
    ))
}

async fn util_fail(arguments: &Value) -> Result<Value> {
    let message = arguments
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("message is required"))?;
    let details = arguments
        .get("details")
        .cloned()
        .unwrap_or_else(|| json!({}));

    Err(ToolCallError::new(ToolErrorCode::ActionFailed, message, details).into())
}

async fn util_sleep(arguments: &Value) -> Result<Value> {
    let min_ms = arguments
        .get("minMs")
        .and_then(Value::as_u64)
        .unwrap_or(400)
        .min(600_000);
    let max_ms = arguments
        .get("maxMs")
        .and_then(Value::as_u64)
        .unwrap_or(900)
        .min(600_000);

    if max_ms < min_ms {
        bail!("maxMs must be >= minMs");
    }

    let spread = max_ms - min_ms;
    let jitter = if spread == 0 {
        0
    } else {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_nanos() as u64;
        let seed = now_nanos
            .wrapping_mul(6364136223846793005)
            .wrapping_add(u64::from(std::process::id()));
        seed % (spread + 1)
    };
    let slept_ms = min_ms + jitter;

    tokio::time::sleep(Duration::from_millis(slept_ms)).await;

    Ok(tool_success(
        json!({
            "ok": true,
            "minMs": min_ms,
            "maxMs": max_ms,
            "sleptMs": slept_ms
        }),
        "sleep complete",
    ))
}

async fn util_date_bucket_counts(arguments: &Value) -> Result<Value> {
    let items = arguments
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("items must be an array"))?;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim);
    let windows = arguments
        .get("windowsDays")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("windowsDays must be an array"))?;
    let windows_days: Vec<i64> = windows
        .iter()
        .filter_map(Value::as_i64)
        .filter(|value| *value > 0)
        .collect();
    if windows_days.is_empty() {
        return Err(anyhow!(
            "windowsDays must contain at least one positive integer"
        ));
    }

    let now_ms = arguments
        .get("nowEpochMs")
        .and_then(Value::as_i64)
        .unwrap_or_else(now_epoch_ms);
    let now_days = now_ms / 86_400_000;

    let mut ages_days: Vec<i64> = Vec::new();
    let mut parsed = 0usize;
    let mut skipped = 0usize;

    for item in items {
        let text_opt = if let Some(field) = field.filter(|f| !f.is_empty()) {
            item.get(field).and_then(Value::as_str).map(str::to_string)
        } else {
            item.as_str().map(str::to_string)
        };
        let Some(text) = text_opt else {
            skipped += 1;
            continue;
        };
        if let Some(age_days) = parse_age_days(&text, now_ms, now_days) {
            ages_days.push(age_days);
            parsed += 1;
        } else {
            skipped += 1;
        }
    }

    let mut counts = Vec::new();
    for window in windows_days {
        let count = ages_days.iter().filter(|age| **age <= window).count();
        counts.push(json!({ "windowDays": window, "count": count }));
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "nowEpochMs": now_ms,
            "parsed": parsed,
            "skipped": skipped,
            "counts": counts
        }),
        "bucket counts computed",
    ))
}

fn parse_age_days(text: &str, now_ms: i64, now_days: i64) -> Option<i64> {
    if let Some(days) = parse_relative_days(text) {
        return Some(days);
    }
    parse_absolute_days(text, now_ms, now_days)
}

fn parse_relative_days(text: &str) -> Option<i64> {
    static RELATIVE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)(\d+)\s*(sec|secs|second|seconds|min|mins|minute|minutes|h|hr|hrs|hour|hours|d|day|days|w|wk|wks|week|weeks|mo|mos|month|months|y|yr|yrs|year|years)\b")
            .expect("relative regex")
    });

    let caps = RELATIVE_RE.captures(text)?;
    let value: i64 = caps.get(1)?.as_str().parse().ok()?;
    let unit = caps.get(2)?.as_str().to_lowercase();
    let days = match unit.as_str() {
        "sec" | "secs" | "second" | "seconds" => 0,
        "min" | "mins" | "minute" | "minutes" => 0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 0,
        "d" | "day" | "days" => value,
        "w" | "wk" | "wks" | "week" | "weeks" => value * 7,
        "mo" | "mos" | "month" | "months" => value * 30,
        "y" | "yr" | "yrs" | "year" | "years" => value * 365,
        _ => return None,
    };
    Some(days)
}

fn parse_absolute_days(text: &str, _now_ms: i64, now_days: i64) -> Option<i64> {
    static ABSOLUTE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)\b(Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:t|tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\s+(\d{1,2})(?:,\s*(\d{4}))?"
        )
        .expect("absolute regex")
    });

    let caps = ABSOLUTE_RE.captures(text)?;
    let month_str = caps.get(1)?.as_str();
    let day: i32 = caps.get(2)?.as_str().parse().ok()?;
    let year_opt = caps.get(3).and_then(|m| m.as_str().parse::<i32>().ok());
    let (current_year, _, _) = civil_from_days(now_days);
    let mut year = year_opt.unwrap_or(current_year);

    let month = month_str_to_number(month_str)?;
    let mut date_days = days_from_civil(year, month, day);
    if year_opt.is_none() && date_days - now_days > 7 {
        year -= 1;
        date_days = days_from_civil(year, month, day);
    }
    let age = now_days - date_days;
    if age < 0 {
        None
    } else {
        Some(age)
    }
}

fn month_str_to_number(raw: &str) -> Option<u32> {
    let value = raw.to_lowercase();
    let month = match value.as_str() {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    };
    Some(month)
}

fn days_from_civil(year: i32, month: u32, day: i32) -> i64 {
    let mut y = year as i64;
    let m = month as i64;
    let d = day as i64;
    y -= if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i32) + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as i64
}

fn substitute_vars(value: Value, vars: &Value) -> Value {
    match value {
        Value::String(s) => {
            if let Some(exact) = substitute_exact_value(&s, vars) {
                exact
            } else {
                Value::String(substitute_string(&s, vars))
            }
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| substitute_vars(v, vars))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, substitute_vars(v, vars));
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn ensure_workflow_steps_var(vars: &mut Value) {
    let Some(map) = vars.as_object_mut() else {
        *vars = json!({ "steps": {} });
        return;
    };
    match map.get_mut("steps") {
        Some(Value::Object(_)) => {}
        _ => {
            map.insert("steps".to_string(), json!({}));
        }
    }
}

fn step_save_as(step: &serde_json::Map<String, Value>) -> Option<String> {
    step.get("saveAs")
        .and_then(Value::as_str)
        .or_else(|| step.get("save_as").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn store_step_output(vars: &mut Value, save_as: &str, value: Value) {
    ensure_workflow_steps_var(vars);
    if let Some(map) = vars.as_object_mut() {
        if let Some(Value::Object(steps)) = map.get_mut("steps") {
            steps.insert(save_as.to_string(), value);
        }
    }
}

fn render_workflow_output(
    template: &Value,
    presentation_template: Option<&Value>,
    vars: &Value,
    step_count: usize,
    trace: Vec<Value>,
) -> Value {
    let timing_summary = summarize_trace(&trace);
    let rendered = substitute_vars(template.clone(), vars);
    match rendered {
        Value::Object(mut obj) => {
            obj.insert("steps".to_string(), json!(step_count));
            obj.insert("timings".to_string(), timing_summary);
            obj.insert("trace".to_string(), json!(trace));
            if let Some(presentation) = presentation_template {
                obj.insert(
                    "_presentation".to_string(),
                    substitute_vars(presentation.clone(), vars),
                );
            }
            Value::Object(obj)
        }
        other => json!({
            "output": other,
            "steps": step_count,
            "timings": timing_summary,
            "trace": trace,
            "_presentation": presentation_template.map(|value| substitute_vars(value.clone(), vars)).unwrap_or(Value::Null)
        }),
    }
}

fn summarize_trace(trace: &[Value]) -> Value {
    let mut total_duration_ms = 0u64;
    let mut steps = Vec::with_capacity(trace.len());
    let mut by_tool: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for entry in trace {
        let duration_ms = entry.get("durationMs").and_then(Value::as_u64).unwrap_or(0);
        let tool = entry
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let step = entry.get("step").and_then(Value::as_u64).unwrap_or(0);
        let ok = entry.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let attempt = entry.get("attempt").and_then(Value::as_u64).unwrap_or(0);
        let skipped = entry
            .get("skipped")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        total_duration_ms += duration_ms;
        if !tool.is_empty() {
            let aggregate = by_tool.entry(tool.clone()).or_insert((0, 0));
            aggregate.0 += duration_ms;
            aggregate.1 += 1;
        }

        steps.push(json!({
            "step": step,
            "tool": tool,
            "ok": ok,
            "skipped": skipped,
            "attempt": attempt,
            "durationMs": duration_ms
        }));
    }

    let by_tool = by_tool
        .into_iter()
        .map(|(tool, (duration_ms, count))| {
            json!({
                "tool": tool,
                "durationMs": duration_ms,
                "count": count
            })
        })
        .collect::<Vec<_>>();

    json!({
        "totalDurationMs": total_duration_ms,
        "steps": steps,
        "byTool": by_tool
    })
}

fn substitute_exact_value(input: &str, vars: &Value) -> Option<Value> {
    let trimmed = input.trim();
    if !trimmed.starts_with("{{") || !trimmed.ends_with("}}") {
        return None;
    }
    let key = trimmed
        .trim_start_matches("{{")
        .trim_end_matches("}}")
        .trim();
    if key.is_empty() {
        return None;
    }
    lookup_var_value(vars, key)
}

fn substitute_string(input: &str, vars: &Value) -> String {
    let mut out = String::new();
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + end;
        out.push_str(&rest[..start]);
        let key = rest[start + 2..end].trim();
        if let Some(repl) = lookup_var_string(vars, key) {
            out.push_str(&repl);
        } else {
            out.push_str(&rest[start..end + 2]);
        }
        rest = &rest[end + 2..];
    }

    out.push_str(rest);
    out
}

fn eval_when(when: &Value, vars: &Value) -> Result<bool> {
    match when {
        Value::Bool(value) => Ok(*value),
        Value::String(path) => Ok(value_truthy(
            lookup_var_value(vars, path).unwrap_or(Value::Null),
        )),
        Value::Object(map) => {
            let var = map
                .get("var")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("when.var is required"))?;
            let value = lookup_var_value(vars, var).unwrap_or(Value::Null);

            if let Some(eq) = map.get("equals") {
                return Ok(value_matches(&value, eq));
            }
            if let Some(ne) = map.get("notEquals") {
                return Ok(!value_matches(&value, ne));
            }
            if let Some(exists) = map.get("exists").and_then(Value::as_bool) {
                let has_value = !matches!(value, Value::Null);
                return Ok(if exists { has_value } else { !has_value });
            }
            if let Some(truthy) = map.get("truthy").and_then(Value::as_bool) {
                let is_truthy = value_truthy(value);
                return Ok(if truthy { is_truthy } else { !is_truthy });
            }

            Err(anyhow!(
                "when object requires equals, notEquals, exists, or truthy"
            ))
        }
        _ => Err(anyhow!("when must be a boolean, string, or object")),
    }
}

fn value_truthy(value: Value) -> bool {
    match value {
        Value::Bool(b) => b,
        Value::Number(n) => n.as_i64().map(|v| v != 0).unwrap_or(true),
        Value::String(s) => {
            let trimmed = s.trim().to_lowercase();
            !trimmed.is_empty() && trimmed != "false" && trimmed != "0"
        }
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Null => false,
    }
}

fn value_matches(value: &Value, candidate: &Value) -> bool {
    match candidate {
        Value::Array(list) => list.iter().any(|item| value_matches(value, item)),
        _ => value_equals(value, candidate),
    }
}

fn value_equals(value: &Value, candidate: &Value) -> bool {
    match (value, candidate) {
        (Value::String(a), Value::String(b)) => a.eq_ignore_ascii_case(b),
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Null, Value::Null) => true,
        (Value::String(a), Value::Bool(b)) => {
            let normalized = a.trim().to_lowercase();
            if normalized == "true" {
                *b
            } else if normalized == "false" {
                !*b
            } else {
                false
            }
        }
        (Value::String(a), Value::Number(b)) => a
            .trim()
            .parse::<f64>()
            .ok()
            .is_some_and(|v| b.as_f64().is_some_and(|bv| (bv - v).abs() < f64::EPSILON)),
        _ => value == candidate,
    }
}

fn lookup_var_value(vars: &Value, key: &str) -> Option<Value> {
    let mut cur = vars;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur.clone())
}

fn lookup_var_string(vars: &Value, key: &str) -> Option<String> {
    let value = lookup_var_value(vars, key)?;
    match value {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

async fn driver_from_state(state: &AppState) -> Result<WebDriverClient> {
    let _ = state.restore_persisted_runtime().await;
    let base_url = state.appium_base_url().await.ok_or_else(|| {
        anyhow::Error::new(ToolCallError::new(
            ToolErrorCode::ActionFailed,
            "Appium is not initialized. Call ios.appium.ensure first.",
            json!({}),
        ))
    })?;
    let driver = WebDriverClient::new(&base_url).map_err(|err| {
        anyhow::Error::new(ToolCallError::new(
            ToolErrorCode::ActionFailed,
            format!("{err:#}"),
            json!({ "baseUrl": &base_url }),
        ))
    })?;
    Ok(driver)
}

async fn resolve_session_id(state: &AppState, arguments: &Value) -> Result<String> {
    let _ = state.restore_persisted_runtime().await;
    if let Some(value) = arguments.get("sessionId").and_then(Value::as_str) {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }

    state
        .active_session()
        .await
        .map(|session| session.session_id)
        .ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::NoSession,
                "sessionId is required when no active session exists",
                json!({}),
            ))
        })
}

fn session_matches_request(
    existing: &crate::state::SessionState,
    udid: &str,
    kind: &str,
    bundle_id: Option<&str>,
) -> bool {
    if existing.udid != udid || existing.kind != kind {
        return false;
    }

    if kind == "native_app" {
        return existing.bundle_id.as_deref() == bundle_id;
    }

    true
}

async fn session_is_alive(driver: &WebDriverClient, session_id: &str) -> bool {
    driver.page_source(session_id).await.is_ok()
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                format!("'{key}' is required"),
                json!({"param": key}),
            ))
        })
}

fn resolve_workflow_ref(arguments: &Value) -> Result<String> {
    if let (Some(system), Some(workflow)) = (
        arguments.get("system").and_then(Value::as_str),
        arguments.get("workflow").and_then(Value::as_str),
    ) {
        return workflows::compose_workflow_reference(system, workflow).ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                "workflow ref requires a non-empty system and workflow",
                json!({"params": ["system", "workflow"]}),
            ))
        });
    }

    for key in ["workflow", "name"] {
        if let Some(value) = arguments
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(value.to_string());
        }
    }

    Err(anyhow::Error::new(ToolCallError::new(
        ToolErrorCode::InvalidParams,
        "workflow ref is required via 'name' or 'workflow' (optionally pair 'workflow' with 'system')",
        json!({"params": ["name", "workflow", "system"]}),
    )))
}

async fn wait_for_selector(
    driver: &WebDriverClient,
    session_id: &str,
    selector: &str,
    index: usize,
    require_unique: bool,
    timeout: Duration,
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let ids = driver.find_elements_css(session_id, selector).await?;
        if ids.is_empty() {
            // keep waiting
        } else if require_unique && ids.len() != 1 {
            return Err(ToolCallError::new(
                ToolErrorCode::AmbiguousMatch,
                format!(
                    "expected exactly one match for selector '{selector}', got {}",
                    ids.len()
                ),
                json!({"selector": selector, "matchCount": ids.len()}),
            )
            .into());
        } else if let Some(value) = ids.get(index) {
            return Ok(value.clone());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                format!("timeout waiting for selector '{selector}'"),
                json!({"selector": selector, "index": index}),
            )
            .into());
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::ui_compact::TargetLocator;
    use once_cell::sync::Lazy;
    use serde_json::json;
    use std::collections::HashMap;
    use tokio::sync::Mutex as TokioMutex;

    static ENV_LOCK: Lazy<TokioMutex<()>> = Lazy::new(|| TokioMutex::new(()));

    #[test]
    fn tool_error_from_anyhow_downcasts_tool_call_error() {
        let err = anyhow::Error::new(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "missing",
            json!({"k": 1}),
        ));
        let value = tool_error_from_anyhow(&err, "ios.action.tap");
        let structured = value
            .get("structuredContent")
            .and_then(Value::as_object)
            .expect("structured");
        assert_eq!(
            structured.get("errorCode").and_then(Value::as_str),
            Some("ELEMENT_NOT_FOUND")
        );
        assert_eq!(
            structured
                .get("details")
                .and_then(|v| v.get("tool"))
                .and_then(Value::as_str),
            Some("ios.action.tap")
        );
    }

    #[test]
    fn list_tool_definitions_includes_phone_system_tools() {
        let definitions = list_tool_definitions();
        let names: Vec<String> = definitions
            .iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();

        assert!(names.contains(&"phone_messages.list_recent_threads".to_string()));
        assert!(names.contains(&"phone_messages.read_latest_messages".to_string()));
        assert!(names.contains(&"phone_messages.find_recent_otp".to_string()));
        assert!(names.contains(&"phone_calls.list_recent_calls".to_string()));
        assert!(names.contains(&"phone_notifications.list_recent_notifications".to_string()));
        assert!(names.contains(&"phone_notifications.filter_notifications_by_app".to_string()));
        assert!(names.contains(&"ios.capability.list".to_string()));

        let capability_tool = definitions
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("ios.web.goto"))
            .expect("ios.web.goto definition");
        assert_eq!(
            capability_tool
                .get("capabilityFamily")
                .and_then(Value::as_str),
            Some("navigate")
        );
    }

    #[test]
    fn tool_policy_metadata_is_present_for_complete_registry() {
        for definition in list_tool_definitions() {
            let name = definition
                .get("name")
                .and_then(Value::as_str)
                .expect("tool name");
            for field in [
                "mutating",
                "risk",
                "privacyClass",
                "allowedDirect",
                "allowedInWorkflow",
                "policy",
            ] {
                assert!(
                    definition.get(field).is_some(),
                    "{name} missing policy field {field}"
                );
            }

            let policy = definition.get("policy").expect("policy object");
            assert_eq!(definition.get("mutating"), policy.get("mutating"));
            assert_eq!(definition.get("risk"), policy.get("risk"));
            assert_eq!(definition.get("privacyClass"), policy.get("privacyClass"));
            assert_eq!(definition.get("allowedDirect"), policy.get("allowedDirect"));
            assert_eq!(
                definition.get("allowedInWorkflow"),
                policy.get("allowedInWorkflow")
            );
        }
    }

    #[test]
    fn tool_policy_gated_schemas_expose_gate_arguments() {
        let definitions = list_tool_definitions();
        let tap = definitions
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("ios.action.tap"))
            .expect("tap definition");
        assert!(tap
            .get("inputSchema")
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.get("commit"))
            .is_some());

        let otp = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some("phone_messages.find_recent_otp")
            })
            .expect("otp definition");
        assert_eq!(otp.get("privacyClass").and_then(Value::as_str), Some("otp"));
        assert!(otp
            .get("inputSchema")
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.get("privacyGate"))
            .is_some());
    }

    #[tokio::test]
    async fn tool_policy_direct_mutating_tools_require_commit_gate() {
        let state = AppState::new();
        for name in [
            "ios.action.tap",
            "ios.action.type",
            "ios.web.click_css",
            "ios.web.type_css",
            "ios.web.press_key",
            "ios.alert.accept",
            "ios.alert.dismiss",
        ] {
            let err = handle_tool_call(&state, name, json!({}))
                .await
                .expect_err("direct mutating tool should be gated");
            let typed = err
                .chain()
                .find_map(|cause| cause.downcast_ref::<ToolCallError>())
                .expect("typed policy error");
            assert_eq!(typed.code, ToolErrorCode::CommitRequired, "{name}");
        }
    }

    #[tokio::test]
    async fn tool_policy_direct_eval_js_is_gated_before_execution() {
        let state = AppState::new();
        let err = handle_tool_call(&state, "ios.web.eval_js", json!({"script": "1"}))
            .await
            .expect_err("eval_js should be gated");
        let typed = err
            .chain()
            .find_map(|cause| cause.downcast_ref::<ToolCallError>())
            .expect("typed policy error");
        assert_eq!(typed.code, ToolErrorCode::PolicyDenied);
    }

    #[tokio::test]
    async fn runtime_guardrails_encoded_compact_targets_default_to_unique_resolution() {
        let state = AppState::new();
        let mut targets = HashMap::new();
        targets.insert(
            "target-1".to_string(),
            TargetLocator {
                using: "accessibility id".to_string(),
                value: "Like".to_string(),
            },
        );
        state
            .set_compact_observation("snap-1".to_string(), "sess-1".to_string(), targets)
            .await;

        let resolved = resolve_target(
            &state,
            &json!({"target": {"encodedId": "target-1", "snapshotId": "snap-1"}}),
        )
        .await
        .expect("resolve")
        .expect("target");

        assert!(resolved.require_unique);
    }

    #[tokio::test]
    async fn privacy_failure_artifacts_default_to_minimal_without_screenshot_or_source() {
        let _guard = ENV_LOCK.lock().await;
        let old_policy = std::env::var_os("RZN_IOS_FAILURE_ARTIFACTS");
        std::env::remove_var("RZN_IOS_FAILURE_ARTIFACTS");
        let state = AppState::new();
        state
            .set_session(
                "sess-1".to_string(),
                "native_app".to_string(),
                "udid-1".to_string(),
                None,
                Some(8100),
            )
            .await;

        let artifacts = capture_failure_artifacts(&state).await.expect("artifacts");

        assert_eq!(
            artifacts.get("policy").and_then(Value::as_str),
            Some("minimal")
        );
        assert!(artifacts.get("screenshot").is_none());
        assert!(artifacts.get("uiSource").is_none());

        if let Some(value) = old_policy {
            std::env::set_var("RZN_IOS_FAILURE_ARTIFACTS", value);
        }
    }

    #[tokio::test]
    async fn tool_policy_private_phone_tools_require_privacy_specific_gates() {
        let state = AppState::new();
        for (name, privacy_class) in [
            ("phone_messages.list_recent_threads", "messages"),
            ("phone_messages.read_latest_messages", "messages"),
            ("phone_messages.find_recent_otp", "otp"),
            ("phone_calls.list_recent_calls", "calls"),
            (
                "phone_notifications.list_recent_notifications",
                "notifications",
            ),
            (
                "phone_notifications.filter_notifications_by_app",
                "notifications",
            ),
        ] {
            let err = handle_tool_call(&state, name, json!({}))
                .await
                .expect_err("private phone tool should be gated");
            let typed = err
                .chain()
                .find_map(|cause| cause.downcast_ref::<ToolCallError>())
                .expect("typed policy error");
            assert_eq!(typed.code, ToolErrorCode::PolicyDenied, "{name}");
            assert_eq!(
                typed
                    .details
                    .get("policy")
                    .and_then(|policy| policy.get("privacyClass"))
                    .and_then(Value::as_str),
                Some(privacy_class)
            );
        }
    }

    #[test]
    fn extract_otp_candidates_prefers_auth_hint_messages() {
        let thread = json!({
            "thread_id": "phone_messages-thread-1-openai",
            "title": "OpenAI",
            "preview": "Your verification code is here",
            "position": 1
        });
        let messages = vec![
            json!({
                "message_id": "msg-1",
                "body": "Your verification code is 123456. Do not share it.",
                "sender": "OpenAI",
                "position": 3
            }),
            json!({
                "message_id": "msg-2",
                "body": "Order 654321 has shipped.",
                "sender": "Shop",
                "position": 2
            }),
        ];

        let mut candidates =
            extract_otp_candidates_from_messages(&thread, &messages, None, None, 4, 8);
        rank_otp_candidates(&mut candidates);

        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].get("code").and_then(Value::as_str),
            Some("123456")
        );
        assert!(
            candidates[0]
                .get("score")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                > candidates[1]
                    .get("score")
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
        );
    }

    #[test]
    fn extract_otp_candidates_respects_sender_and_length_filters() {
        let thread = json!({
            "thread_id": "phone_messages-thread-2-bank",
            "title": "Bank",
            "preview": "Security code",
            "position": 2
        });
        let messages = vec![
            json!({
                "message_id": "msg-1",
                "body": "Your code is 4321",
                "sender": "Bank",
                "position": 4
            }),
            json!({
                "message_id": "msg-2",
                "body": "Use 555555 to sign in",
                "sender": "Another App",
                "position": 5
            }),
        ];

        let candidates =
            extract_otp_candidates_from_messages(&thread, &messages, Some("Bank"), None, 4, 4);

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].get("code").and_then(Value::as_str),
            Some("4321")
        );
    }

    #[test]
    fn stable_phone_ids_encode_position_for_thread_lookup() {
        let id = stable_phone_item_id(
            "phone_messages-thread",
            &[String::from("Alice"), String::from("Today")],
            3,
        );
        assert_eq!(parse_position_from_stable_id(&id), Some(3));
    }

    #[tokio::test]
    async fn util_list_find_matches_regex_after_offset() {
        let result = util_list_find(&json!({
            "list": [
                {"rawLabel": "r/AppBusiness, 0 comments"},
                {"rawLabel": "r/appledevelopers, 2 comments"},
                {"rawLabel": "r/ProductivityApps, 1 Comment"}
            ],
            "field": "rawLabel",
            "regex": "(^|, )([1-9][0-9]*)\\s+[Cc]omments?\\b",
            "startOffset": 1
        }))
        .await
        .expect("util.list.find should succeed");

        let structured = result
            .get("structuredContent")
            .and_then(Value::as_object)
            .expect("structured content");
        assert_eq!(structured.get("found").and_then(Value::as_bool), Some(true));
        assert_eq!(structured.get("index").and_then(Value::as_u64), Some(2));
        assert_eq!(
            structured
                .get("value")
                .and_then(|value| value.get("rawLabel"))
                .and_then(Value::as_str),
            Some("r/appledevelopers, 2 comments")
        );
    }

    #[tokio::test]
    async fn util_list_find_returns_not_found_when_no_match_exists() {
        let result = util_list_find(&json!({
            "list": [
                {"rawLabel": "r/AppBusiness, 0 comments"}
            ],
            "field": "rawLabel",
            "regex": "(^|, )([1-9][0-9]*)\\s+[Cc]omments?\\b"
        }))
        .await
        .expect("util.list.find should succeed");

        let structured = result
            .get("structuredContent")
            .and_then(Value::as_object)
            .expect("structured content");
        assert_eq!(
            structured.get("found").and_then(Value::as_bool),
            Some(false)
        );
        assert!(structured.get("value").is_some());
        assert!(structured.get("value").unwrap().is_null());
    }

    #[tokio::test]
    async fn util_rect_relative_point_computes_expected_coordinates() {
        let result = util_rect_relative_point(&json!({
            "x": 17.0,
            "y": 438.0,
            "width": 358.0,
            "height": 148.0,
            "relativeX": 0.63,
            "relativeY": 0.88
        }))
        .await
        .expect("util.rect.relative_point should succeed");

        let structured = result
            .get("structuredContent")
            .and_then(Value::as_object)
            .expect("structured content");
        assert_eq!(structured.get("x").and_then(Value::as_f64), Some(242.54));
        assert_eq!(structured.get("y").and_then(Value::as_f64), Some(568.24));
    }

    #[test]
    fn extract_rows_captures_geometry_and_safe_tap_point() {
        let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<AppiumAUT>
  <XCUIElementTypeCell
    type="XCUIElementTypeCell"
    name="2033810430435667998"
    label="Felix Rieseberg. Verified. Example post."
    x="0"
    y="138"
    width="375"
    height="488">
    <XCUIElementTypeOther
      type="XCUIElementTypeOther"
      label="nested"
      x="0"
      y="138"
      width="375"
      height="488"/>
  </XCUIElementTypeCell>
</AppiumAUT>"#;

        let row_query =
            parse_row_query(Some(&json!({"type": "XCUIElementTypeCell"}))).expect("row query");
        let primary_query = parse_primary_query(Some(
            &json!({"type": "XCUIElementTypeCell", "attr": "label", "pick": "first"}),
        ))
        .expect("primary query");
        let rows = extract_rows_from_source(
            source,
            &row_query,
            &primary_query,
            None,
            &[],
            &parse_split_config(None),
        );

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.width, 375.0);
        assert_eq!(row.height, 488.0);
        assert!((preferred_row_tap_x(row.x, row.width) - 67.5).abs() < f64::EPSILON);
        assert!((preferred_row_tap_y(row.y, row.height) - 194.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_rows_visible_only_filters_hidden_nodes() {
        let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<AppiumAUT>
  <XCUIElementTypeStaticText
    type="XCUIElementTypeStaticText"
    name="38 comments"
    label="38 comments"
    visible="false"
    x="10"
    y="700"
    width="80"
    height="20"/>
  <XCUIElementTypeStaticText
    type="XCUIElementTypeStaticText"
    name="12 comments"
    label="12 comments"
    visible="true"
    x="20"
    y="220"
    width="80"
    height="20"/>
</AppiumAUT>"#;

        let row_query = parse_row_query(Some(&json!({
            "type": "XCUIElementTypeStaticText",
            "labelContains": "comments",
            "visibleOnly": true
        })))
        .expect("row query");
        let primary_query = parse_primary_query(Some(&json!({
            "type": "XCUIElementTypeStaticText",
            "attr": "label",
            "pick": "first"
        })))
        .expect("primary query");
        let rows = extract_rows_from_source(
            source,
            &row_query,
            &primary_query,
            None,
            &[],
            &parse_split_config(None),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].raw_label, "12 comments");
    }

    #[test]
    fn find_matching_row_in_rows_supports_field_selection_and_match_index() {
        let rows = vec![
            RowMatch {
                x: 0.0,
                y: 50.0,
                width: 375.0,
                height: 100.0,
                raw_label: "r/foo, Alpha, 1 comment".to_string(),
                fields: vec![("subtitle".to_string(), "Alpha".to_string())],
                extra_fields: vec![("body".to_string(), "First alpha body".to_string())],
                tag_field: None,
                tag_value: None,
            },
            RowMatch {
                x: 0.0,
                y: 160.0,
                width: 375.0,
                height: 120.0,
                raw_label: "r/bar, Beta, 3 comments".to_string(),
                fields: vec![("subtitle".to_string(), "Beta".to_string())],
                extra_fields: vec![("body".to_string(), "Second beta body".to_string())],
                tag_field: None,
                tag_value: None,
            },
            RowMatch {
                x: 0.0,
                y: 300.0,
                width: 375.0,
                height: 110.0,
                raw_label: "r/baz, Beta+, 5 comments".to_string(),
                fields: vec![("subtitle".to_string(), "Beta+".to_string())],
                extra_fields: vec![("body".to_string(), "Third beta body".to_string())],
                tag_field: None,
                tag_value: None,
            },
        ];
        let match_query =
            parse_string_match_query(&json!({"contains": "beta", "caseSensitive": false}), "test")
                .expect("match query");
        let mut seen = HashSet::new();
        let mut matched_count = 0usize;

        let found = find_matching_row_in_rows(
            &rows,
            "subtitle",
            &match_query,
            true,
            &mut seen,
            &mut matched_count,
            1,
        )
        .expect("second matching row");

        assert_eq!(found.0, 2);
        assert_eq!(found.1, "Beta+");
        assert_eq!(found.2.raw_label, "r/baz, Beta+, 5 comments");
        assert_eq!(matched_count, 1);
    }

    #[test]
    fn find_matching_row_in_rows_dedupes_repeated_candidates_across_passes() {
        let row = RowMatch {
            x: 0.0,
            y: 50.0,
            width: 375.0,
            height: 100.0,
            raw_label: "r/foo, Alpha, 1 comment".to_string(),
            fields: vec![("subtitle".to_string(), "Alpha".to_string())],
            extra_fields: vec![],
            tag_field: None,
            tag_value: None,
        };
        let rows = vec![row.clone()];
        let match_query =
            parse_string_match_query(&json!({"contains": "alpha"}), "test").expect("match query");
        let mut seen = HashSet::new();
        let mut matched_count = 0usize;

        let first = find_matching_row_in_rows(
            &rows,
            "subtitle",
            &match_query,
            true,
            &mut seen,
            &mut matched_count,
            0,
        )
        .expect("first row");
        assert_eq!(first.2.raw_label, row.raw_label);

        let second = find_matching_row_in_rows(
            &rows,
            "subtitle",
            &match_query,
            true,
            &mut seen,
            &mut matched_count,
            0,
        );
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn run_steps_blocks_requires_commit_with_error_code() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({"tool": "ios.web.goto", "requiresCommit": true, "arguments": {"url": "https://example.com"}})],
            false,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("result");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("errorCode").and_then(Value::as_str),
            Some("COMMIT_REQUIRED")
        );
        let trace = result
            .get("trace")
            .and_then(Value::as_array)
            .expect("trace");
        assert_eq!(trace.len(), 1);
        assert_eq!(
            trace[0].get("errorCode").and_then(Value::as_str),
            Some("COMMIT_REQUIRED")
        );
    }

    #[tokio::test]
    async fn tool_policy_run_steps_allows_safe_read_only_and_committed_steps() {
        let state = AppState::new();
        let read_only = run_steps(
            &state,
            &[json!({"tool": "util.sleep", "arguments": {"minMs": 0, "maxMs": 0}})],
            false,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("read-only result");
        assert_eq!(read_only.get("ok").and_then(Value::as_bool), Some(true));

        let committed = run_steps(
            &state,
            &[json!({"tool": "util.sleep", "requiresCommit": true, "arguments": {"minMs": 0, "maxMs": 0}})],
            true,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("committed result");
        assert_eq!(committed.get("ok").and_then(Value::as_bool), Some(true));
    }

    #[tokio::test]
    async fn run_steps_preserves_tool_error_code() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({"tool": "ios.ui.source", "arguments": {}})],
            false,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("result");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("errorCode").and_then(Value::as_str),
            Some("NO_SESSION")
        );
        let trace = result
            .get("trace")
            .and_then(Value::as_array)
            .expect("trace");
        assert!(!trace.is_empty());
        let last = trace.last().unwrap();
        assert_eq!(
            last.get("errorCode").and_then(Value::as_str),
            Some("NO_SESSION")
        );
    }

    #[tokio::test]
    async fn workflow_retry_count_matches_declared_retries() {
        for (retries, expected_attempts) in [(0, 1usize), (1, 2), (2, 3)] {
            let state = AppState::new();
            let result = run_steps(
                &state,
                &[json!({
                    "tool": "util.fail",
                    "retries": retries,
                    "arguments": {"message": "forced retry failure"}
                })],
                false,
                &json!({}),
                None,
                None,
            )
            .await
            .expect("result");

            let trace = result
                .get("trace")
                .and_then(Value::as_array)
                .expect("trace");
            assert_eq!(trace.len(), expected_attempts, "retries={retries}");
            assert_eq!(
                trace
                    .last()
                    .and_then(|entry| entry.get("attempt"))
                    .and_then(Value::as_u64),
                Some(expected_attempts as u64)
            );
        }
    }

    #[tokio::test]
    async fn workflow_retry_trace_records_every_failed_attempt() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({
                "tool": "util.fail",
                "retries": 2,
                "arguments": {"message": "try again"}
            })],
            false,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("result");

        let trace = result
            .get("trace")
            .and_then(Value::as_array)
            .expect("trace");
        assert_eq!(trace.len(), 3);
        for (idx, entry) in trace.iter().enumerate() {
            assert_eq!(entry.get("ok").and_then(Value::as_bool), Some(false));
            assert_eq!(
                entry.get("attempt").and_then(Value::as_u64),
                Some((idx + 1) as u64)
            );
            assert!(entry.get("durationMs").and_then(Value::as_u64).is_some());
            assert_eq!(
                entry.get("errorCode").and_then(Value::as_str),
                Some("ACTION_FAILED")
            );
        }
        assert_eq!(
            trace[0].get("willRetry").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            trace[1].get("willRetry").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            trace[2].get("willRetry").and_then(Value::as_bool),
            Some(false)
        );
    }

    #[tokio::test]
    async fn workflow_retry_timeout_on_mutating_step_is_marked_possibly_applied() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({
                "tool": "util.sleep",
                "requiresCommit": true,
                "timeoutMs": 250,
                "arguments": {"minMs": 1000, "maxMs": 1000}
            })],
            true,
            &json!({}),
            None,
            None,
        )
        .await
        .expect("result");

        assert_eq!(
            result.get("errorCode").and_then(Value::as_str),
            Some("TIMEOUT")
        );
        let trace = result
            .get("trace")
            .and_then(Value::as_array)
            .expect("trace");
        let details = trace[0]
            .get("errorDetails")
            .and_then(Value::as_object)
            .expect("error details");
        assert_eq!(
            details.get("possiblyApplied").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn run_steps_renders_presentation_and_null_for_skipped_saved_steps() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({
                "tool": "util.sleep",
                "when": false,
                "saveAs": "optionalArtifact",
                "arguments": {"minMs": 0, "maxMs": 0}
            })],
            false,
            &json!({
                "query": "demo",
                "items": [{"title": "One", "url": "https://example.com", "snippet": ""}]
            }),
            Some(&json!({
                "pageSource": "{{steps.optionalArtifact}}"
            })),
            Some(&json!({
                "cli": {
                    "type": "result_list",
                    "title": "Results for {{query}}",
                    "items": "{{items}}",
                    "titleField": "title",
                    "urlField": "url",
                    "snippetField": "snippet",
                    "footer": "done"
                }
            })),
        )
        .await
        .expect("result");

        assert_eq!(result.get("pageSource"), Some(&Value::Null));
        let cli = result
            .get("_presentation")
            .and_then(|value| value.get("cli"))
            .expect("cli presentation");
        assert_eq!(cli.get("type").and_then(Value::as_str), Some("result_list"));
        assert_eq!(
            cli.get("title").and_then(Value::as_str),
            Some("Results for demo")
        );
        assert_eq!(
            cli.get("items").and_then(Value::as_array).map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn util_sleep_zero_window_returns_zero() {
        let result = util_sleep(&json!({"minMs": 0, "maxMs": 0}))
            .await
            .expect("sleep result");
        let structured = result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| json!({}));
        assert_eq!(structured.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(structured.get("sleptMs").and_then(Value::as_u64), Some(0));
    }

    #[test]
    fn session_matching_requires_same_native_bundle() {
        let session = crate::state::SessionState {
            session_id: "session-1".to_string(),
            kind: "native_app".to_string(),
            udid: "udid-1".to_string(),
            bundle_id: Some("com.apple.AppStore".to_string()),
            wda_local_port: Some(8100),
            created_at_epoch: 1,
        };

        assert!(session_matches_request(
            &session,
            "udid-1",
            "native_app",
            Some("com.apple.AppStore")
        ));
        assert!(!session_matches_request(
            &session,
            "udid-1",
            "native_app",
            Some("com.reddit.Reddit")
        ));
    }

    #[test]
    fn summarize_trace_aggregates_step_and_tool_timings() {
        let summary = summarize_trace(&[
            json!({"step": 1, "tool": "ios.web.goto", "ok": true, "attempt": 1, "durationMs": 120}),
            json!({"step": 2, "tool": "ios.web.eval_js", "ok": true, "attempt": 1, "durationMs": 45}),
            json!({"step": 3, "tool": "ios.web.goto", "ok": true, "attempt": 1, "durationMs": 30}),
        ]);

        assert_eq!(
            summary.get("totalDurationMs").and_then(Value::as_u64),
            Some(195)
        );
        let by_tool = summary
            .get("byTool")
            .and_then(Value::as_array)
            .expect("byTool array");
        assert!(by_tool.iter().any(|entry| {
            entry.get("tool").and_then(Value::as_str) == Some("ios.web.goto")
                && entry.get("durationMs").and_then(Value::as_u64) == Some(150)
                && entry.get("count").and_then(Value::as_u64) == Some(2)
        }));
    }
}
