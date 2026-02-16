use anyhow::{anyhow, bail, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::{json, Value};
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
    str,
};

use crate::appium::{ensure_appium, EnsureOptions};
use crate::state::AppState;
use crate::ui_compact::{build_compact_snapshot, NodeFilter};
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
            description: "Open Safari on iPhone, search Google, and return top results."
                .to_string(),
        });
    by_name
        .entry("appstore.typeahead".to_string())
        .or_insert_with(|| WorkflowInfo {
            name: "appstore.typeahead".to_string(),
            version: "1.0.0".to_string(),
            description: "Open App Store Search and capture ordered typeahead suggestions."
                .to_string(),
        });
    by_name
        .entry("appstore.search_results".to_string())
        .or_insert_with(|| WorkflowInfo {
            name: "appstore.search_results".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Run App Store search and capture ordered results plus optional observed rank."
                    .to_string(),
        });

    let mut out: Vec<WorkflowInfo> = by_name.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn run_workflow(state: &AppState, name: &str, args: &Value) -> Result<Value> {
    match name {
        "safari.google_search" => run_safari_google_search(state, args).await,
        "appstore.typeahead" => run_appstore_typeahead(state, args).await,
        "appstore.search_results" => run_appstore_search_results(state, args).await,
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
    dirs.push(PathBuf::from(
        "crates/rzn_ios_tools_worker/resources/workflows",
    ));

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

#[derive(Debug, Clone)]
struct NativeSessionConfig {
    udid: String,
    wda_local_port: Option<u16>,
    session_create_timeout_ms: u64,
    wda_launch_timeout_ms: u64,
    wda_connection_timeout_ms: u64,
    show_xcode_log: Option<bool>,
    allow_provisioning_updates: Option<bool>,
    allow_provisioning_device_registration: Option<bool>,
    xcode_org_id: Option<String>,
    xcode_signing_id: Option<String>,
    updated_wda_bundle_id: Option<String>,
    language: Option<String>,
    locale: Option<String>,
    country: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct AppStoreSuggestion {
    text: String,
    position: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct AppStorePrefixSuggestions {
    prefix: String,
    suggestions: Vec<AppStoreSuggestion>,
    #[serde(rename = "suggestionCount")]
    suggestion_count: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct AppStoreSearchResult {
    position: usize,
    name: String,
    subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer: Option<String>,
}

#[derive(Debug, Clone)]
struct AppStoreSearchResultCandidate {
    y: f64,
    name: String,
    subtitle: String,
    developer: Option<String>,
}

#[derive(Debug, Clone)]
struct SearchResultCellBuilder {
    depth: usize,
    y: f64,
    labels: Vec<String>,
    artwork_tags: Vec<String>,
}

async fn run_appstore_typeahead(state: &AppState, args: &Value) -> Result<Value> {
    let query_input = workflow_arg_str(args, "query");
    let typing_mode = workflow_arg_str(args, "typing_mode")
        .or_else(|| workflow_arg_str(args, "typingMode"))
        .unwrap_or_else(|| "full".to_string());
    let limit = workflow_arg_u64(args, "limit").unwrap_or(10).clamp(1, 20) as usize;

    let config = parse_native_session_config(args)?;
    let (driver, session_id) =
        create_native_app_session(state, &config, "com.apple.AppStore").await?;

    maybe_dismiss_native_popups(&driver, &session_id).await;
    open_appstore_search(&driver, &session_id).await?;
    let prefixes = resolve_typeahead_prefixes(args, query_input.as_deref());
    if prefixes.is_empty() {
        bail!("workflow args.query or args.prefixes[] is required");
    }
    let query = query_input.unwrap_or_else(|| prefixes.last().cloned().unwrap_or_default());
    let mut source = String::new();
    let mut prefixes_out: Vec<AppStorePrefixSuggestions> = Vec::new();
    let mut final_suggestions: Vec<AppStoreSuggestion> = Vec::new();

    for prefix in &prefixes {
        type_in_appstore_search_field(&driver, &session_id, prefix, &typing_mode).await?;
        tokio::time::sleep(Duration::from_millis(900)).await;

        source = driver
            .page_source(&session_id)
            .await
            .context("failed to capture App Store source after typing")?;
        let mut suggestions = parse_appstore_typeahead_suggestions(&source, limit);
        if suggestions.is_empty() && typing_mode != "char-by-char" {
            type_in_appstore_search_field(&driver, &session_id, prefix, "char-by-char").await?;
            tokio::time::sleep(Duration::from_millis(900)).await;
            source = driver
                .page_source(&session_id)
                .await
                .context("failed to capture App Store source after char-by-char fallback")?;
            suggestions = parse_appstore_typeahead_suggestions(&source, limit);
        }

        final_suggestions = suggestions.clone();
        prefixes_out.push(AppStorePrefixSuggestions {
            prefix: prefix.clone(),
            suggestion_count: suggestions.len(),
            suggestions,
        });
    }

    let screenshot_b64 = driver
        .screenshot(&session_id)
        .await
        .context("failed to capture App Store screenshot")?;

    Ok(json!({
        "ok": true,
        "workflow": "appstore.typeahead",
        "query": query,
        "typingMode": typing_mode,
        "limit": limit,
        "country": config.country,
        "locale": config.locale,
        "activePrefix": prefixes.last().cloned(),
        "prefixes": prefixes_out,
        "prefixCount": prefixes.len(),
        "suggestions": final_suggestions,
        "suggestionCount": final_suggestions.len(),
        "locatorStrategy": {
            "searchTab": "accessibility id: AppStore.tabBar.search",
            "searchField": "accessibility id: AppStore.searchField",
            "suggestionRows": "collection: AppStore.searchHints -> XCUIElementTypeCell label/name"
        },
        "screenshot": {
            "mimeType": "image/png",
            "data": screenshot_b64
        },
        "uiSource": {
            "length": source.len(),
            "source": source
        }
    }))
}

fn resolve_typeahead_prefixes(args: &Value, query: Option<&str>) -> Vec<String> {
    let explicit: Vec<String> = workflow_arg(args, "prefixes")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter_map(normalize_spacing)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !explicit.is_empty() {
        return explicit;
    }

    let mut prefixes = Vec::new();
    let mut cur = String::new();
    let query = query.unwrap_or("");
    for ch in query.chars() {
        cur.push(ch);
        if let Some(normalized) = normalize_spacing(&cur) {
            prefixes.push(normalized);
        }
    }
    if prefixes.is_empty() {
        prefixes.push(query.trim().to_string());
    }

    let mut dedup = HashSet::new();
    prefixes
        .into_iter()
        .filter(|value| dedup.insert(value.clone()))
        .collect()
}

async fn run_appstore_search_results(state: &AppState, args: &Value) -> Result<Value> {
    let query = workflow_arg_str(args, "query")
        .ok_or_else(|| anyhow!("workflow args.query is required"))?;
    let typing_mode = workflow_arg_str(args, "typing_mode")
        .or_else(|| workflow_arg_str(args, "typingMode"))
        .unwrap_or_else(|| "full".to_string());
    let limit = workflow_arg_u64(args, "limit").unwrap_or(10).clamp(1, 50) as usize;
    let max_scrolls = workflow_arg_u64(args, "maxScrolls")
        .unwrap_or_else(|| limit.div_ceil(2) as u64)
        .clamp(0, 12) as usize;
    let target_app_name = workflow_arg_str(args, "target_app_name")
        .or_else(|| workflow_arg_str(args, "targetAppName"));

    let config = parse_native_session_config(args)?;
    let (driver, session_id) =
        create_native_app_session(state, &config, "com.apple.AppStore").await?;

    maybe_dismiss_native_popups(&driver, &session_id).await;
    open_appstore_search(&driver, &session_id).await?;
    type_in_appstore_search_field(&driver, &session_id, &query, &typing_mode).await?;
    tokio::time::sleep(Duration::from_millis(900)).await;

    let source_after_type = driver
        .page_source(&session_id)
        .await
        .context("failed to capture source before submitting search")?;
    let suggestions = parse_appstore_typeahead_suggestions(&source_after_type, 12);
    submit_appstore_search(&driver, &session_id, &query, &suggestions).await?;

    wait_for_first_locator(
        &driver,
        &session_id,
        &[
            ("accessibility id", "AppStore.searchResults"),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeCollectionView' AND name == 'AppStore.searchResults'",
            ),
            (
                "-ios predicate string",
                "name BEGINSWITH 'AppStore.shelfItem.searchResult['",
            ),
        ],
        Duration::from_secs(25),
    )
    .await
    .context("App Store results list did not appear")?;

    tokio::time::sleep(Duration::from_millis(600)).await;

    let first_source = driver
        .page_source(&session_id)
        .await
        .context("failed to capture App Store results source")?;
    let screenshot_b64 = driver
        .screenshot(&session_id)
        .await
        .context("failed to capture App Store results screenshot")?;

    let compact_snapshot = build_compact_snapshot(&first_source, NodeFilter::All, 220)
        .context("failed to build compact snapshot for App Store results")?;

    let mut rows: Vec<AppStoreSearchResultCandidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut sources_seen: HashSet<String> = HashSet::new();
    sources_seen.insert(first_source.clone());

    collect_search_results_from_source(&first_source, &mut rows, &mut seen);
    let mut scrolls_performed = 0usize;

    while rows.len() < limit && scrolls_performed < max_scrolls {
        perform_scroll_gesture(&driver, &session_id, "down", 0.62).await?;
        tokio::time::sleep(Duration::from_millis(800)).await;
        let next_source = driver
            .page_source(&session_id)
            .await
            .context("failed to capture App Store source while scrolling results")?;
        if !sources_seen.insert(next_source.clone()) {
            break;
        }
        collect_search_results_from_source(&next_source, &mut rows, &mut seen);
        scrolls_performed += 1;
    }

    rows.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    rows.truncate(limit);

    let results: Vec<AppStoreSearchResult> = rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| AppStoreSearchResult {
            position: idx + 1,
            name: row.name,
            subtitle: row.subtitle,
            developer: row.developer,
        })
        .collect();

    let observed_rank = target_app_name
        .as_deref()
        .and_then(|target| observed_rank_for_target(target, &results));

    Ok(json!({
        "ok": true,
        "workflow": "appstore.search_results",
        "query": query,
        "limit": limit,
        "country": config.country,
        "locale": config.locale,
        "target_app_name": target_app_name,
        "observed_rank": observed_rank,
        "typeaheadSuggestions": suggestions,
        "results": results,
        "resultCount": results.len(),
        "scrollsPerformed": scrolls_performed,
        "locatorStrategy": {
            "resultsCollection": "accessibility id: AppStore.searchResults",
            "resultRows": "name BEGINSWITH AppStore.shelfItem.searchResult[",
            "titleSubtitle": "primary row button label split by commas (layout-agnostic)"
        },
        "compactSnapshot": {
            "snapshotId": compact_snapshot.snapshot_id,
            "stats": compact_snapshot.stats,
            "nodes": compact_snapshot.nodes
        },
        "screenshot": {
            "mimeType": "image/png",
            "data": screenshot_b64
        },
        "uiSource": {
            "length": first_source.len(),
            "source": first_source
        }
    }))
}

fn parse_native_session_config(args: &Value) -> Result<NativeSessionConfig> {
    let session = args.get("session").cloned().unwrap_or_else(|| json!({}));
    let udid = session
        .get("udid")
        .or_else(|| args.get("udid"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("session.udid is required"))?
        .to_string();

    let locale = session
        .get("locale")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| workflow_arg_str(args, "locale"));
    let country = workflow_arg_str(args, "country");
    let language = session
        .get("language")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| locale.as_deref().and_then(language_from_locale));

    Ok(NativeSessionConfig {
        udid,
        wda_local_port: session
            .get("wdaLocalPort")
            .and_then(Value::as_u64)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v > 0),
        session_create_timeout_ms: session
            .get("sessionCreateTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(600_000),
        wda_launch_timeout_ms: session
            .get("wdaLaunchTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(240_000),
        wda_connection_timeout_ms: session
            .get("wdaConnectionTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(120_000),
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
        language,
        locale,
        country,
    })
}

async fn create_native_app_session(
    state: &AppState,
    config: &NativeSessionConfig,
    bundle_id: &str,
) -> Result<(WebDriverClient, String)> {
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

    let created = driver
        .create_session_native_app(
            SessionCreateRequest {
                udid: config.udid.clone(),
                no_reset: true,
                new_command_timeout_sec: 60,
                session_create_timeout_ms: Some(config.session_create_timeout_ms),
                wda_local_port: config.wda_local_port,
                wda_launch_timeout_ms: Some(config.wda_launch_timeout_ms),
                wda_connection_timeout_ms: Some(config.wda_connection_timeout_ms),
                language: config.language.clone(),
                locale: config.locale.clone(),
                show_xcode_log: config.show_xcode_log,
                allow_provisioning_updates: config.allow_provisioning_updates,
                allow_provisioning_device_registration: config
                    .allow_provisioning_device_registration,
                xcode_org_id: config.xcode_org_id.clone(),
                xcode_signing_id: config.xcode_signing_id.clone(),
                updated_wda_bundle_id: config.updated_wda_bundle_id.clone(),
            },
            bundle_id.to_string(),
        )
        .await
        .context("create App Store session failed")?;

    state
        .set_session(
            created.session_id.clone(),
            "native_app".to_string(),
            config.udid.clone(),
            config.wda_local_port,
        )
        .await;

    Ok((driver, created.session_id))
}

async fn maybe_dismiss_native_popups(driver: &WebDriverClient, session_id: &str) {
    let _ = driver.alert_dismiss(session_id).await;
    let _ = driver.alert_accept(session_id).await;

    let dismiss_locators = [
        ("accessibility id", "Not Now"),
        ("accessibility id", "Cancel"),
        ("accessibility id", "Close"),
        (
            "-ios predicate string",
            "type == 'XCUIElementTypeButton' AND (name == 'Not Now' OR label == 'Not Now' OR name == 'Close' OR label == 'Close' OR name == 'Cancel' OR label == 'Cancel')",
        ),
    ];
    for (using, value) in dismiss_locators {
        if let Ok(ids) = driver.find_elements(session_id, using, value).await {
            if let Some(element_id) = ids.first() {
                let _ = driver.click_element(session_id, element_id).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn open_appstore_search(driver: &WebDriverClient, session_id: &str) -> Result<()> {
    tap_first_available(
        driver,
        session_id,
        &[
            ("accessibility id", "AppStore.tabBar.search"),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeButton' AND (name == 'AppStore.tabBar.search' OR name == 'Search' OR label == 'Search')",
            ),
        ],
        Duration::from_secs(12),
    )
    .await
    .context("failed to open App Store Search tab")?;

    wait_for_first_locator(
        driver,
        session_id,
        &[
            ("accessibility id", "AppStore.searchField"),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeSearchField' AND name == 'AppStore.searchField'",
            ),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeSearchField'",
            ),
        ],
        Duration::from_secs(15),
    )
    .await
    .context("search field did not appear")?;

    Ok(())
}

async fn type_in_appstore_search_field(
    driver: &WebDriverClient,
    session_id: &str,
    query: &str,
    typing_mode: &str,
) -> Result<()> {
    let field_id = tap_first_available(
        driver,
        session_id,
        &[
            ("accessibility id", "AppStore.searchField"),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeSearchField' AND name == 'AppStore.searchField'",
            ),
            (
                "-ios predicate string",
                "type == 'XCUIElementTypeSearchField'",
            ),
        ],
        Duration::from_secs(12),
    )
    .await
    .context("failed to focus App Store search field")?;

    let _ = driver.clear_element(session_id, &field_id).await;
    if let Ok(ids) = driver
        .find_elements(session_id, "accessibility id", "Clear text")
        .await
    {
        if let Some(clear_id) = ids.first() {
            let _ = driver.click_element(session_id, clear_id).await;
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    }

    if typing_mode == "char-by-char" {
        for ch in query.chars() {
            let step = ch.to_string();
            driver
                .type_element(session_id, &field_id, &step)
                .await
                .with_context(|| format!("failed typing char '{step}'"))?;
            tokio::time::sleep(Duration::from_millis(90)).await;
        }
    } else {
        driver
            .type_element(session_id, &field_id, query)
            .await
            .context("failed typing App Store query")?;
    }

    Ok(())
}

async fn submit_appstore_search(
    driver: &WebDriverClient,
    session_id: &str,
    query: &str,
    suggestions: &[AppStoreSuggestion],
) -> Result<()> {
    let preferred = suggestions
        .iter()
        .find(|entry| normalize_for_match(&entry.text) == normalize_for_match(query))
        .or_else(|| suggestions.first())
        .map(|entry| entry.text.clone());

    if let Some(target_text) = preferred {
        if let Ok(ids) = driver
            .find_elements(session_id, "accessibility id", &target_text)
            .await
        {
            if let Some(element_id) = ids.first() {
                driver
                    .click_element(session_id, element_id)
                    .await
                    .context("failed to tap matching typeahead suggestion")?;
                return Ok(());
            }
        }
    }

    if let Ok(ids) = driver
        .find_elements(session_id, "accessibility id", "Search")
        .await
    {
        if let Some(element_id) = ids.last() {
            driver
                .click_element(session_id, element_id)
                .await
                .context("failed to tap Search key")?;
            return Ok(());
        }
    }

    bail!("unable to submit App Store search query");
}

fn parse_appstore_typeahead_suggestions(source: &str, limit: usize) -> Vec<AppStoreSuggestion> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut depth = 0usize;
    let mut hints_collection_depth: Option<usize> = None;
    let mut out: Vec<(f64, f64, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if elem_name == "XCUIElementTypeCollectionView"
                    && attr_text(&e, "name").as_deref() == Some("AppStore.searchHints")
                {
                    hints_collection_depth = Some(depth);
                } else if elem_name == "XCUIElementTypeCell"
                    && hints_collection_depth.is_some()
                    && attr_bool(&e, "visible", true)
                {
                    if let Some(text) = extract_preferred_text(&e) {
                        if !text.starts_with("AppStore.") {
                            out.push((
                                attr_f64(&e, "y").unwrap_or(9_999.0),
                                attr_f64(&e, "x").unwrap_or(0.0),
                                text,
                            ));
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if elem_name == "XCUIElementTypeCell"
                    && hints_collection_depth.is_some()
                    && attr_bool(&e, "visible", true)
                {
                    if let Some(text) = extract_preferred_text(&e) {
                        if !text.starts_with("AppStore.") {
                            out.push((
                                attr_f64(&e, "y").unwrap_or(9_999.0),
                                attr_f64(&e, "x").unwrap_or(0.0),
                                text,
                            ));
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if elem_name == "XCUIElementTypeCollectionView"
                    && hints_collection_depth == Some(depth)
                {
                    hints_collection_depth = None;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut dedup = HashSet::new();
    let mut ordered: Vec<String> = Vec::new();
    for (_, _, text) in out {
        let key = normalize_for_match(&text);
        if key.is_empty() || dedup.contains(&key) {
            continue;
        }
        dedup.insert(key);
        ordered.push(text);
    }

    ordered
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(idx, text)| AppStoreSuggestion {
            text,
            position: idx + 1,
        })
        .collect()
}

fn parse_appstore_search_results(source: &str) -> Vec<AppStoreSearchResultCandidate> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;

    let mut buf = Vec::new();
    let mut depth = 0usize;
    let mut in_results_depth: Option<usize> = None;
    let mut current_cell: Option<SearchResultCellBuilder> = None;
    let mut out: Vec<AppStoreSearchResultCandidate> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if elem_name == "XCUIElementTypeCollectionView"
                    && attr_text(&e, "name").as_deref() == Some("AppStore.searchResults")
                {
                    in_results_depth = Some(depth);
                }

                if elem_name == "XCUIElementTypeCell" && in_results_depth.is_some() {
                    let cell_name = attr_text(&e, "name").unwrap_or_default();
                    if cell_name.starts_with("AppStore.shelfItem.searchResult[") {
                        current_cell = Some(SearchResultCellBuilder {
                            depth,
                            y: attr_f64(&e, "y").unwrap_or(9_999.0),
                            labels: Vec::new(),
                            artwork_tags: Vec::new(),
                        });
                    }
                }

                if let Some(cell) = current_cell.as_mut() {
                    collect_result_cell_field(cell, &elem_name, &e);
                }
            }
            Ok(Event::Empty(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if let Some(cell) = current_cell.as_mut() {
                    collect_result_cell_field(cell, &elem_name, &e);
                }
            }
            Ok(Event::End(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if elem_name == "XCUIElementTypeCell" {
                    let should_finalize = current_cell
                        .as_ref()
                        .map(|cell| cell.depth == depth)
                        .unwrap_or(false);
                    if should_finalize {
                        if let Some(candidate) = finalize_result_cell(current_cell.take()) {
                            out.push(candidate);
                        }
                    }
                }

                if elem_name == "XCUIElementTypeCollectionView" && in_results_depth == Some(depth) {
                    in_results_depth = None;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn collect_result_cell_field(
    cell: &mut SearchResultCellBuilder,
    elem_name: &str,
    elem: &BytesStart<'_>,
) {
    if elem_name == "XCUIElementTypeButton" {
        if let Some(label) = extract_preferred_text(elem) {
            if looks_like_primary_result_label(&label) {
                cell.labels.push(label);
            }
        }
    } else if elem_name == "XCUIElementTypeOther" {
        if let Some(name) = attr_text(elem, "name") {
            if let Some(tag) = name.strip_prefix("Artwork, ").map(str::trim) {
                if !tag.is_empty() {
                    cell.artwork_tags.push(tag.to_string());
                }
            }
        }
    }
}

fn finalize_result_cell(
    cell: Option<SearchResultCellBuilder>,
) -> Option<AppStoreSearchResultCandidate> {
    let cell = cell?;
    let summary = cell
        .labels
        .into_iter()
        .max_by_key(|label| label.len())
        .unwrap_or_default();
    if summary.is_empty() {
        return None;
    }

    let (name, subtitle) = parse_search_result_summary(&summary)?;
    let developer = if cell.artwork_tags.len() > 1 {
        cell.artwork_tags.last().cloned()
    } else {
        None
    };

    Some(AppStoreSearchResultCandidate {
        y: cell.y,
        name,
        subtitle,
        developer,
    })
}

fn parse_search_result_summary(summary: &str) -> Option<(String, String)> {
    let mut parts: Vec<String> = summary
        .split(',')
        .filter_map(|value| normalize_spacing(value))
        .collect();
    if parts.is_empty() {
        return None;
    }

    if parts
        .first()
        .map(|head| head.eq_ignore_ascii_case("advertisement") || head.eq_ignore_ascii_case("ad"))
        .unwrap_or(false)
    {
        parts.remove(0);
    }
    if parts.is_empty() {
        return None;
    }

    let name = parts[0].clone();
    let subtitle = parts
        .iter()
        .skip(1)
        .find(|part| !looks_like_rating_or_metric(part))
        .cloned()
        .unwrap_or_default();

    Some((name, subtitle))
}

fn collect_search_results_from_source(
    source: &str,
    rows: &mut Vec<AppStoreSearchResultCandidate>,
    seen: &mut HashSet<String>,
) {
    for candidate in parse_appstore_search_results(source) {
        let key = normalize_for_match(&candidate.name);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        rows.push(candidate);
    }
}

fn observed_rank_for_target(target: &str, rows: &[AppStoreSearchResult]) -> Option<usize> {
    let want = normalize_for_match(target);
    if want.is_empty() {
        return None;
    }

    rows.iter()
        .find(|row| normalize_for_match(&row.name) == want)
        .map(|row| row.position)
        .or_else(|| {
            rows.iter()
                .find(|row| {
                    let got = normalize_for_match(&row.name);
                    got.contains(&want) || want.contains(&got)
                })
                .map(|row| row.position)
        })
}

fn workflow_arg<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    args.get("args")
        .and_then(|v| v.get(key))
        .or_else(|| args.get(key))
}

fn workflow_arg_str(args: &Value, key: &str) -> Option<String> {
    workflow_arg(args, key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn workflow_arg_u64(args: &Value, key: &str) -> Option<u64> {
    workflow_arg(args, key).and_then(Value::as_u64)
}

fn language_from_locale(locale: &str) -> Option<String> {
    locale
        .split(['_', '-'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_spacing(input: &str) -> Option<String> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_for_match(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn looks_like_primary_result_label(label: &str) -> bool {
    if label.starts_with("AppStore.") {
        return false;
    }
    let lower = label.to_lowercase();
    if lower == "redownload"
        || lower == "get"
        || lower == "open"
        || lower == "update"
        || lower == "cancel"
        || lower == "search"
        || lower == "advertisement"
        || lower == "ad"
    {
        return false;
    }
    true
}

fn looks_like_rating_or_metric(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("star")
        || lower.contains("rating")
        || lower.ends_with("ratings")
        || lower.ends_with("rating")
        || lower.ends_with("reviews")
}

fn attr_text(elem: &BytesStart<'_>, key: &str) -> Option<String> {
    for attr in elem.attributes().with_checks(false) {
        let Ok(attr) = attr else {
            continue;
        };
        let Ok(name) = str::from_utf8(attr.key.as_ref()) else {
            continue;
        };
        if name != key {
            continue;
        }
        let Ok(raw) = attr.unescape_value() else {
            continue;
        };
        if let Some(normalized) = normalize_spacing(&raw) {
            return Some(normalized);
        }
    }
    None
}

fn attr_f64(elem: &BytesStart<'_>, key: &str) -> Option<f64> {
    attr_text(elem, key).and_then(|value| value.parse::<f64>().ok())
}

fn attr_bool(elem: &BytesStart<'_>, key: &str, default: bool) -> bool {
    match attr_text(elem, key) {
        Some(value) => matches!(value.to_lowercase().as_str(), "true" | "1"),
        None => default,
    }
}

fn extract_preferred_text(elem: &BytesStart<'_>) -> Option<String> {
    attr_text(elem, "label")
        .or_else(|| attr_text(elem, "name"))
        .or_else(|| attr_text(elem, "value"))
}

async fn wait_for_first_locator(
    driver: &WebDriverClient,
    session_id: &str,
    locators: &[(&str, &str)],
    timeout: Duration,
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error: Option<anyhow::Error> = None;

    loop {
        for (using, value) in locators {
            match driver.find_elements(session_id, using, value).await {
                Ok(ids) if !ids.is_empty() => return Ok(ids[0].clone()),
                Ok(_) => {}
                Err(err) => last_error = Some(err),
            }
        }

        if tokio::time::Instant::now() >= deadline {
            let message = last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "no matching elements found".to_string());
            bail!("wait_for_first_locator timed out: {message}");
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn tap_first_available(
    driver: &WebDriverClient,
    session_id: &str,
    locators: &[(&str, &str)],
    timeout: Duration,
) -> Result<String> {
    let element_id = wait_for_first_locator(driver, session_id, locators, timeout).await?;
    driver.click_element(session_id, &element_id).await?;
    Ok(element_id)
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
                {"type": "pointerMove", "duration": 420, "x": end_x, "y": end_y, "origin": "viewport"},
                {"type": "pointerUp", "button": 0}
            ]
        }]
    });
    driver.perform_actions(session_id, payload).await?;
    Ok(())
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
