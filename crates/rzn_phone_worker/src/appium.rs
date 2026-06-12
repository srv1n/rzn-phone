use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

use crate::state::{AppState, AppiumSource};

const DEFAULT_PORT: u16 = 4723;
static PROBE_HTTP_CLIENT: once_cell::sync::Lazy<Client> = once_cell::sync::Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("build Appium probe HTTP client")
});

#[derive(Debug, Clone)]
pub struct EnsureOptions {
    pub port: Option<u16>,
    pub log_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnsureResult {
    pub base_url: String,
    pub source: String,
    pub pid: Option<u32>,
}

pub fn parse_port_value(value: Option<&Value>, field_name: &str) -> Result<Option<u16>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(raw) = value.as_u64() else {
        return Err(anyhow!("{field_name} must be an integer in 1..=65535"));
    };
    let port =
        u16::try_from(raw).map_err(|_| anyhow!("{field_name} must be in 1..=65535, got {raw}"))?;
    if port == 0 {
        return Err(anyhow!("{field_name} must be in 1..=65535, got 0"));
    }
    Ok(Some(port))
}

pub async fn ensure_appium(state: &AppState, options: EnsureOptions) -> Result<EnsureResult> {
    let _ = state.restore_persisted_runtime().await;

    if let Ok(env_url) = env::var("RZN_IOS_APPIUM_URL") {
        let trimmed = env_url.trim();
        if !trimmed.is_empty() {
            let normalized = probe_webdriver_base(trimmed)
                .await
                .with_context(|| format!("RZN_IOS_APPIUM_URL is set but unreachable: {trimmed}"))?;
            state
                .set_appium(normalized.clone(), AppiumSource::Env, None, None)
                .await;
            return Ok(EnsureResult {
                base_url: normalized,
                source: "env".to_string(),
                pid: None,
            });
        }
    }

    if let Some(existing_url) = state.appium_base_url().await {
        if let Ok(normalized) = probe_webdriver_base(&existing_url).await {
            let snapshot = state.snapshot().await;
            let source = snapshot.appium_source.unwrap_or(AppiumSource::Spawned);
            let pid = snapshot.appium_pid;
            state
                .refresh_appium_metadata(normalized.clone(), source, pid)
                .await;
            return Ok(EnsureResult {
                base_url: normalized,
                source: "existing".to_string(),
                pid,
            });
        }

        state.clear_session().await;
        state.clear_appium_metadata().await;
    }

    let port = options.port.unwrap_or(DEFAULT_PORT);
    let log_level = options.log_level.unwrap_or_else(|| "warn".to_string());
    let root_url = format!("http://127.0.0.1:{port}");

    let mut spawn_errors = Vec::new();

    for cmd in appium_command_candidates() {
        for arg_pattern in appium_arg_patterns(port, &log_level) {
            match spawn_candidate(&cmd, &arg_pattern).await {
                Ok(mut child) => match wait_until_ready(&root_url).await {
                    Ok(normalized) => {
                        let pid = child.id();
                        state
                            .set_appium(normalized.clone(), AppiumSource::Spawned, pid, Some(child))
                            .await;
                        return Ok(EnsureResult {
                            base_url: normalized,
                            source: "spawned".to_string(),
                            pid,
                        });
                    }
                    Err(err) => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        spawn_errors.push(format!(
                            "spawned '{cmd} {}' but health-check failed: {err}",
                            arg_pattern.join(" ")
                        ));
                    }
                },
                Err(err) => {
                    spawn_errors.push(format!("failed '{cmd} {}': {err}", arg_pattern.join(" ")));
                }
            }
        }
    }

    Err(anyhow!(build_spawn_remediation(&spawn_errors)))
}

pub async fn probe_webdriver_base(input_url: &str) -> Result<String> {
    for base in base_candidates(input_url) {
        let status_url = format!("{base}/status");
        let response = match PROBE_HTTP_CLIENT.get(&status_url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };

        if !response.status().is_success() {
            continue;
        }

        let payload: Value = response.json().await.unwrap_or(Value::Null);
        if status_payload_is_ready(&payload) {
            return Ok(base);
        }
    }

    Err(anyhow!(
        "no healthy Appium/WebDriver endpoint found for {input_url}"
    ))
}

fn status_payload_is_ready(payload: &Value) -> bool {
    if payload
        .get("ready")
        .and_then(Value::as_bool)
        .is_some_and(|ready| !ready)
    {
        return false;
    }

    if let Some(value_ready) = payload
        .get("value")
        .and_then(|value| value.get("ready"))
        .and_then(Value::as_bool)
    {
        return value_ready;
    }

    if let Some(top_ready) = payload.get("ready").and_then(Value::as_bool) {
        return top_ready;
    }

    payload.get("status").and_then(Value::as_i64) == Some(0)
        && payload.get("value").is_some_and(Value::is_object)
}

async fn wait_until_ready(root_url: &str) -> Result<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);

    loop {
        match probe_webdriver_base(root_url).await {
            Ok(base) => return Ok(base),
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(err);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

fn appium_command_candidates() -> Vec<String> {
    let mut candidates = Vec::new();

    if let Ok(explicit) = env::var("RZN_IOS_APPIUM_BIN") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_string());
        }
    }

    candidates.push("appium".to_string());
    candidates.push("/opt/homebrew/bin/appium".to_string());
    candidates.push("/usr/local/bin/appium".to_string());

    candidates
}

fn appium_arg_patterns(port: u16, log_level: &str) -> Vec<Vec<String>> {
    vec![
        vec![
            "server".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--log-level".to_string(),
            log_level.to_string(),
        ],
        vec![
            "-p".to_string(),
            port.to_string(),
            "--log-level".to_string(),
            log_level.to_string(),
        ],
    ]
}

async fn spawn_candidate(command: &str, args: &[String]) -> Result<Child> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn()
        .with_context(|| format!("spawn failed for {command}"))
}

fn base_candidates(input_url: &str) -> Vec<String> {
    let mut normalized = input_url.trim().trim_end_matches('/').to_string();
    if normalized.is_empty() {
        normalized = format!("http://127.0.0.1:{DEFAULT_PORT}");
    }

    let mut out = Vec::new();
    out.push(normalized.clone());

    if normalized.ends_with("/wd/hub") {
        let stripped = normalized.trim_end_matches("/wd/hub").to_string();
        if !stripped.is_empty() {
            out.push(stripped);
        }
    } else {
        out.push(format!("{normalized}/wd/hub"));
    }

    out.into_iter().fold(Vec::new(), |mut dedup, entry| {
        if !dedup.contains(&entry) {
            dedup.push(entry);
        }
        dedup
    })
}

fn build_spawn_remediation(errors: &[String]) -> String {
    let mut message = String::from(
        "unable to start Appium. Prefer setting RZN_IOS_APPIUM_URL to an already-running Appium endpoint.",
    );

    message.push_str("\n\nTroubleshooting:\n");
    message.push_str("- Ensure Node.js is installed and available to GUI-launched apps.\n");
    message.push_str("- Install Appium globally: npm i -g appium\n");
    message.push_str("- Install XCUITest driver: appium driver install xcuitest\n");
    message.push_str("- If PATH is minimal in the desktop context, set RZN_IOS_APPIUM_BIN or use RZN_IOS_APPIUM_URL.\n");

    if !errors.is_empty() {
        message.push_str("\nSpawn attempts:\n");
        for err in errors {
            message.push_str(&format!("- {err}\n"));
        }
    }

    message
}

#[cfg(test)]
mod tests {
    use super::{base_candidates, ensure_appium, parse_port_value, EnsureOptions};
    #[cfg(unix)]
    use crate::state::{AppState, AppiumSource, TEST_ENV_LOCK};
    use httpmock::{Method::GET, MockServer};
    use serde_json::json;
    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::process::{Command as StdCommand, Stdio};
    #[cfg(unix)]
    use tokio::process::{Child, Command};
    #[cfg(unix)]
    use tokio::time::{sleep, Duration, Instant};

    #[test]
    fn base_candidates_adds_wd_hub_variant() {
        let values = base_candidates("http://127.0.0.1:4723");
        assert!(values.contains(&"http://127.0.0.1:4723".to_string()));
        assert!(values.contains(&"http://127.0.0.1:4723/wd/hub".to_string()));
    }

    #[test]
    fn base_candidates_strips_wd_hub_variant() {
        let values = base_candidates("http://127.0.0.1:4723/wd/hub");
        assert!(values.contains(&"http://127.0.0.1:4723/wd/hub".to_string()));
        assert!(values.contains(&"http://127.0.0.1:4723".to_string()));
    }

    #[test]
    fn runtime_guardrails_port_parser_rejects_zero_and_overflow() {
        assert_eq!(
            parse_port_value(Some(&json!(65535)), "port").unwrap(),
            Some(65535)
        );
        assert!(parse_port_value(Some(&json!(0)), "port").is_err());
        assert!(parse_port_value(Some(&json!(65536)), "port").is_err());
        assert!(parse_port_value(Some(&json!("4723")), "port").is_err());
    }

    #[tokio::test]
    async fn probe_webdriver_base_rejects_not_ready_status_payload() {
        let server = MockServer::start_async().await;
        let status_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/status");
                then.status(200)
                    .json_body(json!({ "value": { "ready": false } }));
            })
            .await;

        let err = super::probe_webdriver_base(&server.url(""))
            .await
            .expect_err("not-ready status must not pass health probe");

        status_mock.assert_async().await;
        assert!(format!("{err:#}").contains("no healthy Appium/WebDriver endpoint"));
    }

    #[tokio::test]
    async fn probe_webdriver_base_accepts_legacy_success_status_payload() {
        let server = MockServer::start_async().await;
        let status_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/status");
                then.status(200)
                    .json_body(json!({ "status": 0, "value": { "build": { "version": "1" } } }));
            })
            .await;

        let base = super::probe_webdriver_base(&server.url(""))
            .await
            .expect("legacy Appium status should pass");

        status_mock.assert_async().await;
        assert_eq!(base, server.url(""));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn repeated_existing_spawned_ensure_preserves_child_for_shutdown() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let _appium_url = EnvVarGuard::unset("RZN_IOS_APPIUM_URL");
        let server = mock_appium_server();
        let base_url = server.base_url();
        let child = spawn_shell_child("sleep 60");
        let pid = child.id().expect("spawned child pid");
        let mut cleanup = PidCleanup::new(pid);
        let state = AppState::new();

        state
            .set_appium(
                base_url.clone(),
                AppiumSource::Spawned,
                Some(pid),
                Some(child),
            )
            .await;

        for _ in 0..2 {
            let result = ensure_appium(
                &state,
                EnsureOptions {
                    port: None,
                    log_level: None,
                },
            )
            .await
            .expect("existing spawned Appium should probe healthy");
            assert_eq!(result.base_url, base_url);
            assert_eq!(result.source, "existing");
            assert_eq!(result.pid, Some(pid));
            assert_eq!(state.appium_child_id().await, Some(pid));
        }

        let snapshot = state.snapshot().await;
        assert_eq!(snapshot.appium_source, Some(AppiumSource::Spawned));
        assert_eq!(snapshot.appium_pid, Some(pid));

        state.shutdown_spawned_appium().await;
        assert!(
            wait_for_process_exit(pid).await,
            "spawned child should be killed by shutdown"
        );
        cleanup.disarm();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn env_owned_ensure_does_not_keep_spawned_child_handle() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let server = mock_appium_server();
        let base_url = server.base_url();
        let _appium_url = EnvVarGuard::set("RZN_IOS_APPIUM_URL", &base_url);
        let child = spawn_waited_child().await;
        let state = AppState::new();

        state
            .set_appium(base_url.clone(), AppiumSource::Spawned, None, Some(child))
            .await;
        assert!(state.has_appium_child().await);

        let result = ensure_appium(
            &state,
            EnsureOptions {
                port: None,
                log_level: None,
            },
        )
        .await
        .expect("env Appium should probe healthy");
        assert_eq!(result.base_url, base_url);
        assert_eq!(result.source, "env");
        assert_eq!(result.pid, None);
        assert!(!state.has_appium_child().await);

        let snapshot = state.snapshot().await;
        assert_eq!(snapshot.appium_source, Some(AppiumSource::Env));
        assert_eq!(snapshot.appium_pid, None);
    }

    #[cfg(unix)]
    fn mock_appium_server() -> MockServer {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/status");
            then.status(200)
                .json_body(json!({ "value": { "ready": true } }));
        });
        server
    }

    #[cfg(unix)]
    fn spawn_shell_child(script: &str) -> Child {
        Command::new("sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn shell child")
    }

    #[cfg(unix)]
    async fn spawn_waited_child() -> Child {
        let mut child = spawn_shell_child(":");
        child.wait().await.expect("wait short-lived child");
        child
    }

    #[cfg(unix)]
    async fn process_is_running(pid: u32) -> bool {
        let Ok(status) = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(Stdio::null())
            .status()
            .await
        else {
            return false;
        };
        status.success()
    }

    #[cfg(unix)]
    async fn wait_for_process_exit(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if !process_is_running(pid).await {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    #[cfg(unix)]
    struct EnvVarGuard {
        name: &'static str,
        previous: Option<OsString>,
    }

    #[cfg(unix)]
    impl EnvVarGuard {
        fn unset(name: &'static str) -> Self {
            let previous = std::env::var_os(name);
            std::env::remove_var(name);
            Self { name, previous }
        }

        fn set(name: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(name);
            std::env::set_var(name, value);
            Self { name, previous }
        }
    }

    #[cfg(unix)]
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.name, previous);
            } else {
                std::env::remove_var(self.name);
            }
        }
    }

    #[cfg(unix)]
    struct PidCleanup {
        pid: u32,
        armed: bool,
    }

    #[cfg(unix)]
    impl PidCleanup {
        fn new(pid: u32) -> Self {
            Self { pid, armed: true }
        }

        fn disarm(&mut self) {
            self.armed = false;
        }
    }

    #[cfg(unix)]
    impl Drop for PidCleanup {
        fn drop(&mut self) {
            if self.armed {
                let _ = StdCommand::new("kill")
                    .arg("-KILL")
                    .arg(self.pid.to_string())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}
