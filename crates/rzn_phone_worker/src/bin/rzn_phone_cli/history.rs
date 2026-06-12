#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct HistoryEntry {
    ts: String,
    #[serde(rename = "workflowRef")]
    workflow_ref: String,
    udid: String,
    #[serde(rename = "argsJson")]
    args_json: Value,
    commit: bool,
    #[serde(rename = "disconnectOnFinish")]
    disconnect_on_finish: bool,
    #[serde(rename = "stopAppiumOnFinish")]
    stop_appium_on_finish: bool,
    #[serde(rename = "backgroundOnExit")]
    background_on_exit: bool,
    #[serde(rename = "lockDeviceOnExit")]
    lock_device_on_exit: bool,
    #[serde(rename = "smartCache")]
    smart_cache: bool,
}

fn state_dir() -> Result<PathBuf> {
    let root = if let Ok(custom) = env::var("RZN_PHONE_STATE_DIR") {
        PathBuf::from(custom)
    } else {
        PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())).join(".rzn-phone")
    };
    create_private_dir(&root)?;
    Ok(root)
}

fn history_path() -> Result<PathBuf> {
    Ok(state_dir()?.join("history.jsonl"))
}

fn favorites_path() -> Result<PathBuf> {
    Ok(state_dir()?.join("favorites.json"))
}

fn load_history() -> Result<Vec<HistoryEntry>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(trimmed) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn save_history(entries: &[HistoryEntry]) -> Result<()> {
    let trimmed = if entries.len() > 200 {
        &entries[entries.len() - 200..]
    } else {
        entries
    };
    let text = trimmed
        .iter()
        .map(redact_history_entry)
        .map(|entry| serde_json::to_string(&entry))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join("\n");
    let body = if text.is_empty() {
        String::new()
    } else {
        text + "\n"
    };
    write_private_file(&history_path()?, body.as_bytes())?;
    Ok(())
}

fn load_recent(limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut entries = load_history()?;
    entries.reverse();
    entries.truncate(limit);
    Ok(entries)
}

fn record_recent_run(entry: HistoryEntry) -> Result<()> {
    if history_disabled() {
        return Ok(());
    }
    let mut entries = load_history()?;
    entries.push(entry);
    save_history(&entries)
}

fn clear_history() -> Result<()> {
    let path = history_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn redact_history_file() -> Result<usize> {
    let entries = load_history()?;
    let count = entries.len();
    save_history(&entries)?;
    Ok(count)
}

fn rerun_entry(index: usize) -> Result<HistoryEntry> {
    let mut entries = load_history()?;
    entries.reverse();
    if index == 0 || index > entries.len() {
        bail!(
            "rzn-phone: recent entry {} does not exist; run `rzn-phone recent`",
            index
        );
    }
    Ok(entries[index - 1].clone())
}

fn history_disabled() -> bool {
    env_flag("RZN_PHONE_HISTORY_DISABLED")
        || env::var("RZN_PHONE_HISTORY")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "off" | "none" | "disabled"
                )
            })
            .unwrap_or(false)
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn redact_history_entry(entry: &HistoryEntry) -> HistoryEntry {
    let mut redacted = entry.clone();
    redacted.args_json = redact_json_value(&redacted.args_json, None);
    redacted
}

fn redact_json_value(value: &Value, key: Option<&str>) -> Value {
    if key.map(is_sensitive_history_key).unwrap_or(false) {
        return json!("[REDACTED]");
    }

    match value {
        Value::String(text) => {
            if looks_sensitive_history_value(text) {
                json!("[REDACTED]")
            } else {
                Value::String(text.clone())
            }
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_json_value(item, key))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = Map::new();
            for (child_key, child_value) in map {
                out.insert(
                    child_key.clone(),
                    redact_json_value(child_value, Some(child_key)),
                );
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn is_sensitive_history_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "auth",
        "authorization",
        "bearer",
        "body",
        "comment",
        "content",
        "cookie",
        "email",
        "message",
        "otp",
        "passcode",
        "password",
        "phone",
        "recipient",
        "secret",
        "text",
        "token",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn looks_sensitive_history_value(value: &str) -> bool {
    static EMAIL_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap());
    static PHONE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?x)(?:\+?\d[\d\s().-]{7,}\d)").unwrap());
    static OTP_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\b(?:otp|code|passcode)[^\d]{0,16}\d{4,8}\b").unwrap());
    static TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(?:bearer|basic)\s+[a-z0-9._~+/=-]{8,}|sk-[a-z0-9]{16,}|\beyJ[a-z0-9_-]+\.[a-z0-9_-]+\.[a-z0-9_-]+\b").unwrap()
    });

    EMAIL_RE.is_match(value)
        || PHONE_RE.is_match(value)
        || OTP_RE.is_match(value)
        || TOKEN_RE.is_match(value)
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, bytes)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static ENV_LOCK: Lazy<StdMutex<()>> = Lazy::new(|| StdMutex::new(()));

    fn sample_history_entry(args_json: Value) -> HistoryEntry {
        HistoryEntry {
            ts: "2026-05-31T00:00:00Z".to_string(),
            workflow_ref: "messages/send".to_string(),
            udid: "TEST-UDID-HISTORY-001".to_string(),
            args_json,
            commit: false,
            disconnect_on_finish: false,
            stop_appium_on_finish: false,
            background_on_exit: false,
            lock_device_on_exit: false,
            smart_cache: false,
        }
    }

    #[test]
    fn privacy_history_redaction_removes_private_values_before_persistence() {
        let entry = sample_history_entry(json!({
            "recipientPhone": "+1 (555) 555-0123",
            "messageBody": "meet me at 4",
            "email": "person@example.com",
            "otp": "code 123456",
            "authHeader": "Bearer abcdefghijklmnop",
            "safe": "open inbox"
        }));

        let redacted = redact_history_entry(&entry);
        let raw = serde_json::to_string(&redacted).expect("json");

        assert!(!raw.contains("+1 (555) 555-0123"));
        assert!(!raw.contains("meet me at 4"));
        assert!(!raw.contains("person@example.com"));
        assert!(!raw.contains("123456"));
        assert!(!raw.contains("Bearer"));
        assert!(raw.contains("open inbox"));
    }

    #[test]
    fn privacy_history_can_be_disabled_by_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_disabled = env::var_os("RZN_PHONE_HISTORY_DISABLED");
        let old_history = env::var_os("RZN_PHONE_HISTORY");
        env::set_var("RZN_PHONE_HISTORY", "off");
        env::remove_var("RZN_PHONE_HISTORY_DISABLED");

        assert!(history_disabled());

        restore_env("RZN_PHONE_HISTORY_DISABLED", old_disabled);
        restore_env("RZN_PHONE_HISTORY", old_history);
    }

    #[cfg(unix)]
    #[test]
    fn privacy_history_file_uses_private_unix_permissions() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_state_dir = env::var_os("RZN_PHONE_STATE_DIR");
        let root = env::temp_dir().join(format!(
            "rzn-phone-history-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        env::set_var("RZN_PHONE_STATE_DIR", &root);

        save_history(&[sample_history_entry(json!({"query": "open inbox"}))]).expect("history");
        let dir_mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        let file_mode = fs::metadata(root.join("history.jsonl"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);

        restore_env("RZN_PHONE_STATE_DIR", old_state_dir);
        let _ = fs::remove_dir_all(root);
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
