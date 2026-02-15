use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

use crate::state::{AppState, AppiumSource};

const DEFAULT_PORT: u16 = 4723;

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

pub async fn ensure_appium(state: &AppState, options: EnsureOptions) -> Result<EnsureResult> {
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
            state.clear_appium_metadata().await;
            state
                .set_appium(normalized.clone(), AppiumSource::Spawned, None, None)
                .await;
            return Ok(EnsureResult {
                base_url: normalized,
                source: "existing".to_string(),
                pid: None,
            });
        }
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
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("build HTTP client")?;

    for base in base_candidates(input_url) {
        let status_url = format!("{base}/status");
        let response = match client.get(&status_url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };

        if !response.status().is_success() {
            continue;
        }

        let payload: Value = response.json().await.unwrap_or_else(|_| Value::Null);
        if payload.get("value").is_some() || payload.get("ready").is_some() || payload.is_object() {
            return Ok(base);
        }
    }

    Err(anyhow!(
        "no healthy Appium/WebDriver endpoint found for {input_url}"
    ))
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
    use super::base_candidates;

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
}
