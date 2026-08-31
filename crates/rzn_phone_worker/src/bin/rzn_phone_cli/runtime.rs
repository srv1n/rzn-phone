#[derive(Clone)]
struct RuntimePaths {
    root: PathBuf,
    plugin_root: PathBuf,
    worker: PathBuf,
    workflow_dir: PathBuf,
    systems_dir: PathBuf,
    examples_dir: PathBuf,
    skills_dir: PathBuf,
    version_file: PathBuf,
    workflow_pack_version_file: PathBuf,
    update_source_file: PathBuf,
}

#[derive(serde::Deserialize)]
struct PersistedRuntimeState {
    #[serde(default)]
    _appium_base_url: Option<String>,
    #[serde(default)]
    _appium_pid: Option<u32>,
    #[serde(default)]
    last_used_epoch_ms: u64,
    #[serde(default)]
    last_udid: Option<String>,
    #[serde(default)]
    session: Option<PersistedSessionState>,
}

#[derive(serde::Deserialize)]
struct PersistedSessionState {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    udid: Option<String>,
}

fn runtime_paths() -> Result<RuntimePaths> {
    if let Some(root) = env::var_os("RZN_PLUGIN_DIR").map(PathBuf::from) {
        return build_packaged_runtime_paths(root);
    }

    let exe = env::current_exe().context("unable to locate current executable")?;
    let mut candidates = BTreeSet::new();
    for ancestor in exe.ancestors() {
        candidates.insert(ancestor.to_path_buf());
    }
    let current_dir = env::current_dir()?;
    for ancestor in current_dir.ancestors() {
        candidates.insert(ancestor.to_path_buf());
    }

    for candidate in candidates {
        if candidate.join("resources/workflows").is_dir() {
            return build_packaged_runtime_paths(candidate);
        }
        if candidate.join("../resources/workflows").is_dir() {
            return build_packaged_runtime_paths(candidate.join(".."));
        }
        if candidate
            .join("crates/rzn_phone_worker/resources/workflows")
            .is_dir()
        {
            return build_repo_runtime_paths(candidate);
        }
    }

    bail!("unable to determine runtime root")
}

fn build_packaged_runtime_paths(root: PathBuf) -> Result<RuntimePaths> {
    let root = root.canonicalize().unwrap_or(root);
    Ok(RuntimePaths {
        plugin_root: root.clone(),
        worker: root.join("libexec/rzn-phone-worker"),
        workflow_dir: root.join("resources/workflows"),
        systems_dir: root.join("resources/systems"),
        examples_dir: root.join("examples"),
        skills_dir: root.join("skills"),
        version_file: root.join("VERSION"),
        workflow_pack_version_file: root.join("WORKFLOW_PACK_VERSION"),
        update_source_file: root.join("UPDATE_SOURCE"),
        root,
    })
}

fn build_repo_runtime_paths(root: PathBuf) -> Result<RuntimePaths> {
    let root = root.canonicalize().unwrap_or(root);
    let plugin_root = root.join("crates/rzn_phone_worker");
    let worker = if root.join("target/release/rzn-phone-worker").is_file() {
        root.join("target/release/rzn-phone-worker")
    } else if root.join("target/debug/rzn-phone-worker").is_file() {
        root.join("target/debug/rzn-phone-worker")
    } else {
        root.join("libexec/rzn-phone-worker")
    };
    Ok(RuntimePaths {
        root: root.clone(),
        plugin_root: plugin_root.clone(),
        worker,
        workflow_dir: plugin_root.join("resources/workflows"),
        systems_dir: plugin_root.join("resources/systems"),
        examples_dir: root.join("examples"),
        skills_dir: root.join("skills"),
        version_file: root.join("VERSION"),
        workflow_pack_version_file: root.join("WORKFLOW_PACK_VERSION"),
        update_source_file: root.join("UPDATE_SOURCE"),
    })
}

fn runtime_version(runtime: &RuntimePaths) -> Result<String> {
    Ok(fs::read_to_string(&runtime.version_file)
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
        .trim()
        .to_string())
}

fn workflow_pack_version(runtime: &RuntimePaths) -> Result<String> {
    let runtime_version = runtime_version(runtime)?;
    Ok(fs::read_to_string(&runtime.workflow_pack_version_file)
        .unwrap_or(runtime_version)
        .trim()
        .to_string())
}

fn default_update_source(runtime: &RuntimePaths) -> Option<String> {
    fs::read_to_string(&runtime.update_source_file)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
fn iso_now() -> String {
    chrono_like_now()
}

fn chrono_like_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now)
}

fn build_session_json(udid: &str) -> Value {
    // Only forward env vars that are actually set. Everything omitted here is
    // resolved by the core (env > ~/.config/rzn-phone/config.json > built-in)
    // inside ios.session.create, so the config file is honored on the run path
    // exactly like on `tool call` and the MCP worker.
    let mut session = Map::new();
    session.insert("udid".to_string(), json!(udid));

    let mut signing = Map::new();
    if let Some(value) = env_present_str("IOS_XCODE_ORG_ID") {
        signing.insert("xcodeOrgId".to_string(), json!(value));
    }
    if let Some(value) = env_present_str("IOS_XCODE_SIGNING_ID") {
        signing.insert("xcodeSigningId".to_string(), json!(value));
    }
    if let Some(value) = env_present_str("IOS_UPDATED_WDA_BUNDLE_ID") {
        signing.insert("updatedWDABundleId".to_string(), json!(value));
    }
    if !signing.is_empty() {
        session.insert("signing".to_string(), Value::Object(signing));
    }

    if let Some(value) = env_present_bool("IOS_SHOW_XCODE_LOG") {
        session.insert("showXcodeLog".to_string(), json!(value));
    }
    if let Some(value) = env_present_bool("IOS_ALLOW_PROVISIONING_UPDATES") {
        session.insert("allowProvisioningUpdates".to_string(), json!(value));
    }
    if let Some(value) = env_present_bool("IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION") {
        session.insert(
            "allowProvisioningDeviceRegistration".to_string(),
            json!(value),
        );
    }
    if let Some(value) = env_present_int("IOS_SESSION_CREATE_TIMEOUT_MS") {
        session.insert("sessionCreateTimeoutMs".to_string(), json!(value));
    }
    if let Some(value) = env_present_int("IOS_WDA_LAUNCH_TIMEOUT_MS") {
        session.insert("wdaLaunchTimeoutMs".to_string(), json!(value));
    }
    if let Some(value) = env_present_int("IOS_WDA_CONNECTION_TIMEOUT_MS") {
        session.insert("wdaConnectionTimeoutMs".to_string(), json!(value));
    }

    Value::Object(session)
}

fn env_present_str(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_present_bool(name: &str) -> Option<bool> {
    match env_present_str(name)?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_present_int(name: &str) -> Option<i64> {
    env_present_str(name)?.parse::<i64>().ok()
}


async fn resolve_run_udid(explicit_udid: Option<String>) -> Result<String> {
    if let Some(udid) = explicit_udid.map(|value| value.trim().to_string()) {
        if !udid.is_empty() {
            return Ok(udid);
        }
    }
    if let Ok(udid) = env::var("RZN_IOS_DEFAULT_UDID") {
        if !udid.trim().is_empty() {
            return Ok(udid);
        }
    }
    let state = AppState::new();
    let payload = call_tool(
        &state,
        "ios.device.list",
        json!({"includeSimulators": false}),
    )
    .await?;
    let devices = payload
        .get("devices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|device| {
            !device
                .get("is_simulator")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && device
                    .get("is_available")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if devices.len() == 1 {
        return Ok(devices[0]
            .get("udid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string());
    }
    if devices.is_empty() {
        bail!("rzn-phone: no available physical devices found; run `rzn-phone devices`");
    }
    let names = devices
        .into_iter()
        .map(|device| {
            device
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| device.get("udid").and_then(Value::as_str))
                .unwrap_or("?")
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "rzn-phone: multiple available devices found; pass --udid explicitly ({})",
        names
    );
}

async fn maybe_cleanup_stale_runtime_cache(_runtime: &RuntimePaths) -> Result<()> {
    let state_file = runtime_state_file_path();
    let ttl_secs = env::var("RZN_IOS_RUNTIME_CACHE_TTL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(300);
    if ttl_secs == 0 || !state_file.exists() {
        return Ok(());
    }
    let payload = serde_json::from_str::<PersistedRuntimeState>(&fs::read_to_string(&state_file)?)?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if now_ms.saturating_sub(payload.last_used_epoch_ms) > ttl_secs.saturating_mul(1000) {
        env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
        let state = AppState::new();
        let _ = call_tool(
            &state,
            "rzn.worker.shutdown",
            json!({"commit": true, "stopAppium": true, "shutdownWDA": true, "backgroundApp": false, "lockDevice": false}),
        )
        .await;
    }
    Ok(())
}

fn runtime_cache_warm_for_udid(udid: &str) -> bool {
    let state_file = runtime_state_file_path();
    let Ok(raw) = fs::read_to_string(state_file) else {
        return false;
    };
    let Ok(payload) = serde_json::from_str::<PersistedRuntimeState>(&raw) else {
        return false;
    };
    let session = match payload.session {
        Some(session) => session,
        None => return false,
    };
    session.session_id.is_some()
        && session
            .udid
            .or(payload.last_udid)
            .as_deref()
            .map(|value| value == udid)
            .unwrap_or(false)
}

fn maybe_print_cold_start_notice(smart_cache_active: bool, udid: &str, output_json: bool) {
    if output_json || !io::stderr().is_terminal() {
        return;
    }
    if smart_cache_active && runtime_cache_warm_for_udid(udid) {
        return;
    }
    if smart_cache_active {
        eprintln!(
            "Preparing device session. Cold starts can take a few seconds; once this session is warm, later runs are faster."
        );
    } else {
        eprintln!(
            "Preparing device session. This run is starting cold, so it can take a few seconds."
        );
    }
}

fn runtime_state_file_path() -> PathBuf {
    env::var("RZN_IOS_RUNTIME_STATE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| rzn_phone_worker::config::state_dir().join("runtime-state.json"))
}

fn payload_input_args(_payload: &Value) -> Option<Value> {
    None
}
