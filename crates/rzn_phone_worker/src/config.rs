//! Persistent user configuration for rzn-phone.
//!
//! Stores signing/provisioning defaults and run preferences so testers do not
//! have to re-supply them on every invocation. The file lives at the
//! XDG-standard config location (`$XDG_CONFIG_HOME/rzn-phone/config.json`,
//! defaulting to `~/.config/rzn-phone/config.json`).
//!
//! Resolution precedence for every value is:
//!   call arguments  >  environment variable  >  config file  >  built-in default
//!
//! The resolver is consulted inside `ios.session.create`, so the CLI `run`,
//! the CLI `tool call`, and the MCP worker all benefit identically.

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// On-disk config schema. Every field is optional so the file stays small and
/// forward-compatible; missing values fall through to env/built-in defaults.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RznConfig {
    pub signing: SigningConfig,
    pub session: SessionConfig,
    pub run: RunConfig,
    /// Provenance/diagnostics; never read back for behavior.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SigningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xcode_org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xcode_signing_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_wda_bundle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_provisioning_updates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_provisioning_device_registration: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_xcode_log: Option<bool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wda_launch_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wda_connection_timeout_ms: Option<u64>,
    /// URL Safari opens at session create (default about:blank). Avoids appium
    /// stalling on web-context selection when Safari Web Extensions are present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safari_initial_url: Option<String>,
    /// Comma-separated hostnames whose pages appium ignores when picking a web
    /// context (e.g. the UUID hosts of `safari-web-extension://` pages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safari_ignore_web_hostnames: Option<String>,
    /// Override for appium's web view / remote-debugger connect timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webview_connect_timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunConfig {
    /// When false (recommended), the WDA session is kept warm between runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect_on_finish: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast: Option<bool>,
}

/// Fully resolved signing inputs handed to `SessionCreateRequest`.
#[derive(Debug, Default, Clone)]
pub struct ResolvedSigning {
    pub xcode_org_id: Option<String>,
    pub xcode_signing_id: Option<String>,
    pub updated_wda_bundle_id: Option<String>,
    pub allow_provisioning_updates: Option<bool>,
    pub allow_provisioning_device_registration: Option<bool>,
    pub show_xcode_log: Option<bool>,
}

/// Result of auto-detecting signing identity from the local machine.
#[derive(Debug, Default, Clone, Serialize)]
pub struct DetectedSigning {
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    /// True when the team is a free "Personal Team" (7-day profile expiry);
    /// None when membership is unknown (e.g. cert-only detection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
    /// Where the team came from: "xcode-account", "apple-development-cert", or "none".
    pub source: String,
    pub notes: Vec<String>,
}

/// One signing/provisioning team known to the machine.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Team {
    pub team_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    pub is_free: bool,
}

/// Outcome of a single doctor check.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

/// Who can resolve a failing/warning check.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FixKind {
    /// A coding agent / CLI can run the `fix` command to resolve it.
    Agent,
    /// A human must act (GUI login, tap a device prompt, plug in hardware).
    Manual,
}

/// A single, agent-actionable readiness check.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_kind: Option<FixKind>,
}

impl DoctorCheck {
    fn ok(id: &str, label: &str, detail: impl Into<String>) -> Self {
        DoctorCheck {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Ok,
            detail: detail.into(),
            fix: None,
            fix_kind: None,
        }
    }
    fn warn(id: &str, label: &str, detail: impl Into<String>, fix: &str, kind: FixKind) -> Self {
        DoctorCheck {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
            fix: Some(fix.to_string()),
            fix_kind: Some(kind),
        }
    }
    fn fail(id: &str, label: &str, detail: impl Into<String>, fix: &str, kind: FixKind) -> Self {
        DoctorCheck {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            fix: Some(fix.to_string()),
            fix_kind: Some(kind),
        }
    }
}

/// A structured remediation step emitted when WDA/signing fails, written so a
/// human or a coding agent can act on it without reading raw xcodebuild logs.
#[derive(Debug, Clone, Serialize)]
pub struct RemediationStep {
    pub kind: FixKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// Classified WDA/signing failure: a stable cause id, a plain-English summary,
/// and ordered remediation steps tagged manual vs agent-runnable.
#[derive(Debug, Clone, Serialize)]
pub struct WdaRemediation {
    pub cause: String,
    pub summary: String,
    pub steps: Vec<RemediationStep>,
    pub docs: String,
}

// ---------------------------------------------------------------------------
// Paths (XDG)
// ---------------------------------------------------------------------------

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

fn xdg_dir(xdg_var: &str, default_suffix: &str) -> PathBuf {
    if let Some(dir) = env::var_os(xdg_var).map(PathBuf::from) {
        if dir.is_absolute() {
            return dir.join("rzn-phone");
        }
    }
    home().join(default_suffix).join("rzn-phone")
}

/// `~/.config/rzn-phone` (or `$XDG_CONFIG_HOME/rzn-phone`).
pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Absolute path to `config.json`. Overridable with `RZN_PHONE_CONFIG_FILE`.
pub fn config_path() -> PathBuf {
    if let Some(explicit) = env::var_os("RZN_PHONE_CONFIG_FILE").map(PathBuf::from) {
        if !explicit.as_os_str().is_empty() {
            return explicit;
        }
    }
    config_dir().join("config.json")
}

/// Persistent runtime state and CLI history.
///
/// `RZN_PHONE_STATE_DIR` overrides the platform/XDG state directory.
pub fn state_dir() -> PathBuf {
    if let Some(custom) = env::var_os("RZN_PHONE_STATE_DIR") {
        if !custom.as_os_str().is_empty() {
            return PathBuf::from(custom);
        }
    }
    xdg_dir("XDG_STATE_HOME", ".local/state")
}

/// `~/.local/share/rzn-phone` — installed runtime/binaries.
pub fn data_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share")
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

impl RznConfig {
    /// Load config, returning defaults if the file is absent or unreadable.
    /// Never fails: a broken config must not break automation.
    pub fn load() -> RznConfig {
        let path = config_path();
        let Ok(raw) = fs::read_to_string(&path) else {
            return RznConfig::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    /// Whether a config file exists on disk.
    pub fn exists() -> bool {
        config_path().exists()
    }

    /// Persist to `config_path()`, creating the directory if needed.
    pub fn save(&self) -> Result<PathBuf> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let body = serde_json::to_string_pretty(self).context("serializing config")?;
        fs::write(&path, body + "\n").with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// Env helpers
// ---------------------------------------------------------------------------

fn env_str(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    let v = env::var(name).ok()?;
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Read a non-negative integer env var (e.g. a timeout in ms).
pub fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.trim().parse::<u64>().ok()
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

// ---------------------------------------------------------------------------
// Resolution (args > env > config > built-in)
// ---------------------------------------------------------------------------

/// Resolve all signing/provisioning inputs for a `session.create` call.
/// `args` is the raw tool arguments object; its nested `signing` block is
/// honored first, then env vars, then the config file.
pub fn resolve_signing(args: &Value) -> ResolvedSigning {
    let cfg = RznConfig::load();
    let signing = args.get("signing").cloned().unwrap_or(Value::Null);

    let xcode_org_id = arg_str(&signing, "xcodeOrgId")
        .or_else(|| env_str("IOS_XCODE_ORG_ID"))
        .or_else(|| cfg.signing.xcode_org_id.clone());

    let xcode_signing_id = arg_str(&signing, "xcodeSigningId")
        .or_else(|| env_str("IOS_XCODE_SIGNING_ID"))
        .or_else(|| cfg.signing.xcode_signing_id.clone());

    let updated_wda_bundle_id = arg_str(&signing, "updatedWDABundleId")
        .or_else(|| env_str("IOS_UPDATED_WDA_BUNDLE_ID"))
        .or_else(|| cfg.signing.updated_wda_bundle_id.clone());

    let allow_provisioning_updates = args
        .get("allowProvisioningUpdates")
        .and_then(Value::as_bool)
        .or_else(|| env_bool("IOS_ALLOW_PROVISIONING_UPDATES"))
        .or(cfg.signing.allow_provisioning_updates);

    let allow_provisioning_device_registration = args
        .get("allowProvisioningDeviceRegistration")
        .and_then(Value::as_bool)
        .or_else(|| env_bool("IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION"))
        .or(cfg.signing.allow_provisioning_device_registration);

    let show_xcode_log = args
        .get("showXcodeLog")
        .and_then(Value::as_bool)
        .or_else(|| env_bool("IOS_SHOW_XCODE_LOG"))
        .or(cfg.signing.show_xcode_log);

    ResolvedSigning {
        xcode_org_id,
        xcode_signing_id,
        updated_wda_bundle_id,
        allow_provisioning_updates,
        allow_provisioning_device_registration,
        show_xcode_log,
    }
}

/// Resolve the session-create timeout (ms): arg > env > config > default.
pub fn session_create_timeout_ms(args: &Value, default_ms: u64) -> u64 {
    args.get("sessionCreateTimeoutMs")
        .and_then(Value::as_u64)
        .or_else(|| env_u64("IOS_SESSION_CREATE_TIMEOUT_MS"))
        .or_else(|| RznConfig::load().session.create_timeout_ms)
        .unwrap_or(default_ms)
}

/// Resolve the WDA launch timeout (ms): arg > env > config > default.
pub fn wda_launch_timeout_ms(args: &Value, default_ms: u64) -> u64 {
    args.get("wdaLaunchTimeoutMs")
        .and_then(Value::as_u64)
        .or_else(|| env_u64("IOS_WDA_LAUNCH_TIMEOUT_MS"))
        .or_else(|| RznConfig::load().session.wda_launch_timeout_ms)
        .unwrap_or(default_ms)
}

/// Resolve the WDA connection timeout (ms): arg > env > config > default.
pub fn wda_connection_timeout_ms(args: &Value, default_ms: u64) -> u64 {
    args.get("wdaConnectionTimeoutMs")
        .and_then(Value::as_u64)
        .or_else(|| env_u64("IOS_WDA_CONNECTION_TIMEOUT_MS"))
        .or_else(|| RznConfig::load().session.wda_connection_timeout_ms)
        .unwrap_or(default_ms)
}

/// Safari web-context options for a `session.create` call.
#[derive(Debug, Default, Clone)]
pub struct ResolvedSafariWeb {
    /// `None` lets the WebDriver layer apply its default (about:blank).
    pub safari_initial_url: Option<String>,
    pub safari_ignore_web_hostnames: Option<String>,
    pub webview_connect_timeout_ms: Option<u64>,
}

/// Resolve Safari web-context options: args > env > config (no built-in here;
/// the WebDriver layer defaults `safariInitialUrl` to about:blank when unset).
pub fn resolve_safari_web(args: &Value) -> ResolvedSafariWeb {
    let cfg = RznConfig::load();

    let safari_initial_url = arg_str(args, "safariInitialUrl")
        .or_else(|| env_str("IOS_SAFARI_INITIAL_URL"))
        .or_else(|| cfg.session.safari_initial_url.clone());

    let safari_ignore_web_hostnames = arg_str(args, "safariIgnoreWebHostnames")
        .or_else(|| env_str("IOS_SAFARI_IGNORE_WEB_HOSTNAMES"))
        .or_else(|| cfg.session.safari_ignore_web_hostnames.clone());

    let webview_connect_timeout_ms = args
        .get("webviewConnectTimeoutMs")
        .and_then(Value::as_u64)
        .or_else(|| env_u64("IOS_WEBVIEW_CONNECT_TIMEOUT_MS"))
        .or(cfg.session.webview_connect_timeout_ms);

    ResolvedSafariWeb {
        safari_initial_url,
        safari_ignore_web_hostnames,
        webview_connect_timeout_ms,
    }
}

// ---------------------------------------------------------------------------
// Auto-detection of signing identity (macOS only)
// ---------------------------------------------------------------------------

const DOCS_URL: &str =
    "https://appium.github.io/appium-xcuitest-driver/latest/preparation/real-device-config/";

/// Detect the best signing team for WDA on this machine.
///
/// Strategy (mirrors what actually works with `-allowProvisioningUpdates`):
///   1. Prefer a team signed into Xcode (`IDEProvisioningTeams`) — only those
///      can mint provisioning profiles on demand. Prefer paid over free.
///   2. Fall back to a team that owns an "Apple Development" codesigning cert.
pub fn detect_signing() -> DetectedSigning {
    let mut notes = Vec::new();

    let teams = read_xcode_teams();
    if let Some(team) = pick_team(&teams) {
        if team.is_free {
            notes.push(
                "free Personal Team: provisioning profiles expire every 7 days; \
                 rzn-phone rebuilds and re-signs WDA automatically on the next run"
                    .to_string(),
            );
        } else {
            notes.push("paid team: profiles do not expire weekly".to_string());
        }
        notes.push("team is signed into Xcode (can auto-create profiles)".to_string());
        return DetectedSigning {
            team_id: Some(team.team_id),
            team_name: team.team_name,
            is_free: Some(team.is_free),
            source: "xcode-account".to_string(),
            notes,
        };
    }
    notes.push("no Xcode account team found; checking codesigning certs".to_string());

    if let Some((team, name)) = apple_development_identities().into_iter().next() {
        notes.push(
            "found an Apple Development cert, but it is NOT signed into Xcode; \
             add the Apple ID in Xcode > Settings > Accounts so profiles can be created"
                .to_string(),
        );
        return DetectedSigning {
            team_id: Some(team),
            team_name: name,
            is_free: None,
            source: "apple-development-cert".to_string(),
            notes,
        };
    }

    notes.push("no usable signing identity detected".to_string());
    DetectedSigning {
        team_id: None,
        team_name: None,
        is_free: None,
        source: "none".to_string(),
        notes,
    }
}

fn pick_team(teams: &[Team]) -> Option<Team> {
    teams
        .iter()
        .find(|t| !t.is_free)
        .or_else(|| teams.first())
        .cloned()
}

#[cfg(target_os = "macos")]
fn run_capture(cmd: &str, args: &[&str]) -> Option<String> {
    use std::process::Command;
    let out = Command::new(cmd).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn run_capture(_cmd: &str, _args: &[&str]) -> Option<String> {
    None
}

/// All teams signed into Xcode (`IDEProvisioningTeams`), paid first.
pub fn read_xcode_teams() -> Vec<Team> {
    let Some(text) = run_capture(
        "defaults",
        &["read", "com.apple.dt.Xcode", "IDEProvisioningTeams"],
    ) else {
        return Vec::new();
    };
    let mut teams = parse_xcode_teams(&text);
    teams.sort_by_key(|t| t.is_free); // false (paid) sorts before true (free)
    teams
}

/// Teams that own an "Apple Development" codesigning cert in the keychain.
pub fn apple_development_identities() -> Vec<(String, Option<String>)> {
    let Some(text) = run_capture("security", &["find-identity", "-v", "-p", "codesigning"]) else {
        return Vec::new();
    };
    parse_apple_development_identities(&text)
}

/// Locate the Appium WebDriverAgent Xcode project, if installed.
pub fn find_wda_project() -> Option<PathBuf> {
    let h = home();
    let candidates = [
        h.join(".appium/node_modules/appium-xcuitest-driver/node_modules/appium-webdriveragent/WebDriverAgent.xcodeproj"),
        h.join(".appium/node_modules/appium-webdriveragent/WebDriverAgent.xcodeproj"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Parse `IDEProvisioningTeams`. `defaults` sorts each team dict's keys
/// alphabetically (isFreeProvisioningTeam, teamID, teamName, teamType), so we
/// split on the dict boundary `}` to keep one team's fields together.
fn parse_xcode_teams(text: &str) -> Vec<Team> {
    let mut teams = Vec::new();
    for chunk in text.split('}') {
        let Some(pos) = chunk.find("teamID") else {
            continue;
        };
        let after = chunk[pos + "teamID".len()..].trim_start();
        let Some(rest) = after.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start().trim_start_matches('"');
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if id.len() != 10 {
            continue;
        }
        let is_free = chunk.contains("isFreeProvisioningTeam = 1");
        let team_name = chunk
            .split("teamName")
            .nth(1)
            .and_then(|s| s.split('"').nth(1))
            .map(ToString::to_string);
        teams.push(Team {
            team_id: id,
            team_name,
            is_free,
        });
    }
    teams
}

fn parse_apple_development_identities(text: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.contains("Apple Development") && !line.contains("iPhone Developer") {
            continue;
        }
        let Some(open) = line.rfind('(') else {
            continue;
        };
        let Some(rel_close) = line[open..].find(')') else {
            continue;
        };
        let team = line[open + 1..open + rel_close].trim().to_string();
        if team.len() != 10 {
            continue;
        }
        let name = line
            .split(':')
            .nth(1)
            .and_then(|s| s.split('(').next())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push((team, name));
    }
    out
}

// ---------------------------------------------------------------------------
// Signing readiness doctor
// ---------------------------------------------------------------------------

/// Verifiable signing/provisioning checks, each tagged manual vs agent-fixable.
/// Composed with `ios.env.doctor` (Node/Appium/xcuitest) by the CLI.
pub fn signing_doctor() -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    // 1. Xcode command-line tools.
    if run_capture("xcodebuild", &["-version"]).is_some() {
        let ver = run_capture("xcodebuild", &["-version"])
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or("Xcode")
            .to_string();
        checks.push(DoctorCheck::ok("xcode", "Xcode build tools", ver));
    } else {
        checks.push(DoctorCheck::fail(
            "xcode",
            "Xcode build tools",
            "xcodebuild not found",
            "Install Xcode from the App Store, then run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer && sudo xcodebuild -license accept",
            FixKind::Manual,
        ));
    }

    // 2. Apple ID signed into Xcode (the real gate for provisioning).
    let teams = read_xcode_teams();
    if teams.is_empty() {
        checks.push(DoctorCheck::fail(
            "xcode_account",
            "Apple ID in Xcode",
            "no Apple ID is signed into Xcode (provisioning cannot be created)",
            "Open Xcode > Settings > Accounts > + and sign in with any Apple ID (a free one works). This is a manual GUI step a coding agent cannot do for you.",
            FixKind::Manual,
        ));
    } else {
        let summary = teams
            .iter()
            .map(|t| {
                format!(
                    "{}{}",
                    t.team_id,
                    if t.is_free { " (free)" } else { " (paid)" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        checks.push(DoctorCheck::ok(
            "xcode_account",
            "Apple ID in Xcode",
            summary,
        ));
    }

    // 3. Chosen signing team + free-team caveat.
    match pick_team(&teams) {
        Some(team) if team.is_free => checks.push(DoctorCheck::warn(
            "signing_team",
            "Signing team",
            format!(
                "{} (free Personal Team)",
                team.team_id
            ),
            "Free teams work, but profiles expire every 7 days — rzn-phone re-signs WDA automatically on the next run (one slower run). For no expiry, enroll a paid team and run: rzn-phone config set signing.xcode_org_id <PAID_TEAM_ID>",
            FixKind::Agent,
        )),
        Some(team) => checks.push(DoctorCheck::ok(
            "signing_team",
            "Signing team",
            format!("{} (paid)", team.team_id),
        )),
        None => {
            let certs = apple_development_identities();
            if let Some((team, _)) = certs.into_iter().next() {
                checks.push(DoctorCheck::fail(
                    "signing_team",
                    "Signing team",
                    format!("cert for team {team} exists but it is NOT signed into Xcode"),
                    "Sign that Apple ID into Xcode > Settings > Accounts (manual), then run: rzn-phone setup --force",
                    FixKind::Manual,
                ));
            } else {
                checks.push(DoctorCheck::fail(
                    "signing_team",
                    "Signing team",
                    "no signing team available",
                    "Sign into Xcode > Settings > Accounts (manual), then run: rzn-phone setup --force",
                    FixKind::Manual,
                ));
            }
        }
    }

    // 4. WebDriverAgent project (installed by the xcuitest driver).
    match find_wda_project() {
        Some(path) => checks.push(DoctorCheck::ok(
            "wda_project",
            "WebDriverAgent project",
            path.display().to_string(),
        )),
        None => checks.push(DoctorCheck::fail(
            "wda_project",
            "WebDriverAgent project",
            "WebDriverAgent.xcodeproj not found",
            "Install the driver: appium driver install xcuitest",
            FixKind::Agent,
        )),
    }

    // 5. rzn-phone config file.
    if RznConfig::exists() {
        let cfg = RznConfig::load();
        let org = cfg
            .signing
            .xcode_org_id
            .clone()
            .unwrap_or_else(|| "(no team set)".to_string());
        checks.push(DoctorCheck::ok(
            "config",
            "rzn-phone config",
            format!(
                "{} -> signing.xcode_org_id = {}",
                config_path().display(),
                org
            ),
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "config",
            "rzn-phone config",
            "no config file yet (first run auto-detects and writes one)",
            "rzn-phone setup",
            FixKind::Agent,
        ));
    }

    checks
}

/// Human/agent steps that cannot be auto-checked (on-device, GUI). Always shown.
pub fn manual_device_steps() -> Vec<RemediationStep> {
    vec![
        RemediationStep {
            kind: FixKind::Manual,
            text: "Plug in the iPhone, unlock it, and tap \"Trust This Computer\".".to_string(),
            command: None,
        },
        RemediationStep {
            kind: FixKind::Manual,
            text: "After WDA installs the first time, on the iPhone open Settings > General > VPN & Device Management and trust the developer profile.".to_string(),
            command: None,
        },
        RemediationStep {
            kind: FixKind::Manual,
            text: "Keep the device unlocked and awake during the first run while WDA builds.".to_string(),
            command: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// WDA / signing failure classifier (agent-actionable)
// ---------------------------------------------------------------------------

/// Turn a raw Appium/xcodebuild WDA error into structured, actionable
/// remediation. Returns `None` when the error is not signing/WDA related.
pub fn classify_wda_failure(error_text: &str) -> Option<WdaRemediation> {
    let lower = error_text.to_lowercase();
    let looks_like_wda = lower.contains("webdriveragent")
        || lower.contains("xcodebuild failed")
        || lower.contains("code 65")
        || lower.contains("provisioning")
        || lower.contains("signing");
    if !looks_like_wda {
        return None;
    }

    let detected = detect_signing();
    let team_hint = detected
        .team_id
        .clone()
        .unwrap_or_else(|| "<TEAM_ID>".to_string());

    // No Apple ID account in Xcode for the requested team.
    if lower.contains("no account for team") || lower.contains("no signing certificate") {
        return Some(WdaRemediation {
            cause: "no-xcode-account".to_string(),
            summary: "WDA can't be signed because the Apple ID for this team is not signed into Xcode."
                .to_string(),
            steps: vec![
                RemediationStep {
                    kind: FixKind::Manual,
                    text: "Open Xcode > Settings > Accounts > + and sign in with the Apple ID that owns the team (a free Apple ID works). A coding agent cannot do this GUI step."
                        .to_string(),
                    command: None,
                },
                RemediationStep {
                    kind: FixKind::Agent,
                    text: "Confirm the team is now visible.".to_string(),
                    command: Some("rzn-phone doctor".to_string()),
                },
                RemediationStep {
                    kind: FixKind::Agent,
                    text: "Re-detect and persist the team, then retry.".to_string(),
                    command: Some("rzn-phone setup --force".to_string()),
                },
            ],
            docs: DOCS_URL.to_string(),
        });
    }

    // Bundle id / profile couldn't be created (often the free-team app-id cap).
    if lower.contains("no profiles for") || lower.contains("failed to register bundle identifier") {
        return Some(WdaRemediation {
            cause: "provisioning-profile".to_string(),
            summary: "Xcode could not create a provisioning profile for the WDA bundle id."
                .to_string(),
            steps: vec![
                RemediationStep {
                    kind: FixKind::Agent,
                    text: "Enable on-demand profile creation and set the team, then retry."
                        .to_string(),
                    command: Some(format!(
                        "rzn-phone config set signing.xcode_org_id {team_hint} && rzn-phone config set signing.allow_provisioning_updates true"
                    )),
                },
                RemediationStep {
                    kind: FixKind::Agent,
                    text: "If you hit the free-account 10-app-ids/week limit, use a unique WDA bundle id."
                        .to_string(),
                    command: Some(
                        "rzn-phone config set signing.updated_wda_bundle_id com.<you>.WebDriverAgentRunner"
                            .to_string(),
                    ),
                },
                RemediationStep {
                    kind: FixKind::Manual,
                    text: "Make sure the iPhone is plugged in, unlocked, and trusted so Xcode can register it."
                        .to_string(),
                    command: None,
                },
            ],
            docs: DOCS_URL.to_string(),
        });
    }

    // Generic xcodebuild/WDA launch failure.
    Some(WdaRemediation {
        cause: "wda-build-failed".to_string(),
        summary: "WebDriverAgent failed to build or launch on the device.".to_string(),
        steps: vec![
            RemediationStep {
                kind: FixKind::Agent,
                text: "Run the readiness check; fix anything marked [agent], do the [manual] items, then retry."
                    .to_string(),
                command: Some("rzn-phone doctor".to_string()),
            },
            RemediationStep {
                kind: FixKind::Agent,
                text: "Capture the full Xcode build log for diagnosis (paste this into a coding agent)."
                    .to_string(),
                command: Some(format!(
                    "cd {} && xcodebuild -project WebDriverAgent.xcodeproj -scheme WebDriverAgentRunner -destination 'id=<UDID>' -allowProvisioningUpdates DEVELOPMENT_TEAM={team_hint} build-for-testing",
                    find_wda_project()
                        .and_then(|p| p.parent().map(|d| d.display().to_string()))
                        .unwrap_or_else(|| "<appium-webdriveragent-dir>".to_string())
                )),
            },
            RemediationStep {
                kind: FixKind::Manual,
                text: "On the iPhone, trust the developer profile: Settings > General > VPN & Device Management."
                    .to_string(),
                command: None,
            },
        ],
        docs: DOCS_URL.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }

    #[test]
    fn state_dir_prefers_explicit_override() {
        let _guard = ENV_LOCK.lock().expect("environment lock");
        let old = env::var_os("RZN_PHONE_STATE_DIR");
        let expected = env::temp_dir().join("rzn-phone-test-state");
        env::set_var("RZN_PHONE_STATE_DIR", &expected);

        assert_eq!(state_dir(), expected);

        restore_env("RZN_PHONE_STATE_DIR", old);
    }

    #[test]
    fn state_dir_defaults_to_xdg_state_home() {
        let _guard = ENV_LOCK.lock().expect("environment lock");
        let old_state = env::var_os("RZN_PHONE_STATE_DIR");
        let old_xdg = env::var_os("XDG_STATE_HOME");
        let expected = env::temp_dir().join("rzn-phone-test-xdg-state");
        env::remove_var("RZN_PHONE_STATE_DIR");
        env::set_var("XDG_STATE_HOME", &expected);

        assert_eq!(state_dir(), expected.join("rzn-phone"));

        restore_env("RZN_PHONE_STATE_DIR", old_state);
        restore_env("XDG_STATE_HOME", old_xdg);
    }

    #[test]
    fn parses_xcode_teams_paid_first() {
        let text = r#"{
    "free@icloud.com" =     (
                {
            isFreeProvisioningTeam = 1;
            teamID = AAAAAAAAAA;
            teamName = "Free Person";
        }
    );
    "paid@icloud.com" =     (
                {
            isFreeProvisioningTeam = 0;
            teamID = 3CQX5AJP6M;
            teamName = "Saravanan Pitchaimani";
        }
    );
}"#;
        let mut teams = parse_xcode_teams(text);
        teams.sort_by_key(|t| t.is_free);
        let picked = pick_team(&teams).expect("team");
        assert_eq!(picked.team_id, "3CQX5AJP6M");
        assert!(!picked.is_free);
        assert_eq!(picked.team_name.as_deref(), Some("Saravanan Pitchaimani"));
    }

    #[test]
    fn falls_back_to_free_team_when_no_paid() {
        let teams = vec![Team {
            team_id: "FREEFREE12".to_string(),
            team_name: Some("Free Person".to_string()),
            is_free: true,
        }];
        let picked = pick_team(&teams).expect("team");
        assert_eq!(picked.team_id, "FREEFREE12");
        assert!(picked.is_free);
    }

    #[test]
    fn parses_apple_development_identity() {
        let text = "  1) ABC \"Developer ID Application: X (3CQX5AJP6M)\"\n  2) DEF \"Apple Development: Saravanan Pitchaimani (7A99W929U5)\"\n";
        let ids = parse_apple_development_identities(text);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].0, "7A99W929U5");
        assert_eq!(ids[0].1.as_deref(), Some("Saravanan Pitchaimani"));
    }

    #[test]
    fn resolve_signing_prefers_args_over_config() {
        let args = json!({"signing": {"xcodeOrgId": "FROMARGZ12"}});
        let resolved = resolve_signing(&args);
        assert_eq!(resolved.xcode_org_id.as_deref(), Some("FROMARGZ12"));
    }

    #[test]
    fn classifies_no_account_failure() {
        let err = "xcodebuild failed with code 65 ... No Account for Team \"7A99W929U5\"";
        let r = classify_wda_failure(err).expect("classified");
        assert_eq!(r.cause, "no-xcode-account");
        assert!(r.steps.iter().any(|s| s.kind == FixKind::Manual));
    }

    #[test]
    fn ignores_unrelated_errors() {
        assert!(classify_wda_failure("element not found: #rso").is_none());
    }
}
