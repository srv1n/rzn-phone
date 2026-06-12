use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::env;

pub const SOURCE: &str = "rzn-phone";
pub const PRODUCT: &str = "rzn-phone";
pub const FLOW_KIND: &str = "phone_automation";
pub const SUBMISSION_MODE: &str = "host_auto";
pub const HOST_EVENT_TYPE: &str = "rzn.flow_failure_report.draft";
pub const VALUE_PROPOSITION: &str =
    "Reporting this helps RZN group phone automation failures and fix the flow faster.";
pub const PRIVACY_STATEMENT: &str = "This draft excludes message text, contact names, phone numbers, screenshots, OCR text, page text, URLs, device logs, trace ids, accessibility trees, and workflow inputs.";

const MAX_NOTE_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureArtifactPolicy {
    Off,
    Minimal,
    Full,
}

impl FailureArtifactPolicy {
    pub fn from_env() -> Self {
        env::var("RZN_IOS_FAILURE_ARTIFACTS")
            .ok()
            .as_deref()
            .map(Self::parse)
            .unwrap_or(Self::Minimal)
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "false" | "off" | "none" => Self::Off,
            "full" | "debug" | "screenshots" | "screenshot" => Self::Full,
            _ => Self::Minimal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Full => "full",
        }
    }

    pub fn captures_screenshot(self) -> bool {
        matches!(self, Self::Full)
    }

    pub fn captures_ui_source(self) -> bool {
        matches!(self, Self::Full)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowFailureReportDraft {
    pub schema_version: u8,
    pub submission_mode: String,
    pub source: String,
    pub product: String,
    pub flow_kind: String,
    pub surface: String,
    pub flow: String,
    pub flow_version: String,
    pub failed_stage: String,
    pub error: String,
    pub app_version: String,
    pub platform: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowFailureContext {
    pub surface: String,
    pub flow: String,
    pub flow_version: String,
    pub app_version: String,
    pub platform: String,
}

#[derive(Debug, Default)]
pub struct FlowFailureReportEmitter {
    emitted_keys: HashSet<String>,
}

impl FlowFailureReportEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit_once(&mut self, draft: &FlowFailureReportDraft) -> Option<Value> {
        if self.emitted_keys.insert(draft.dedupe_key()) {
            Some(draft.host_event())
        } else {
            None
        }
    }
}

impl FlowFailureReportDraft {
    pub fn new(
        context: FlowFailureContext,
        failed_stage: impl Into<String>,
        error: impl Into<String>,
        note: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            schema_version: 1,
            submission_mode: SUBMISSION_MODE.to_string(),
            source: SOURCE.to_string(),
            product: PRODUCT.to_string(),
            flow_kind: FLOW_KIND.to_string(),
            surface: stable_id_or_unknown(&context.surface),
            flow: stable_id_or_unknown(&context.flow),
            flow_version: stable_id_or_unknown(&context.flow_version),
            failed_stage: stable_id_or_unknown(&failed_stage.into()),
            error: normalize_error_code(&error.into()),
            app_version: stable_id_or_unknown(&context.app_version),
            platform: normalize_platform(&context.platform),
            note: normalize_note(note)?,
        })
    }

    pub fn host_event(&self) -> Value {
        json!({
            "type": HOST_EVENT_TYPE,
            "draft": self
        })
    }

    pub fn dedupe_key(&self) -> String {
        [
            self.source.as_str(),
            self.product.as_str(),
            self.flow_kind.as_str(),
            self.surface.as_str(),
            self.flow.as_str(),
            self.flow_version.as_str(),
            self.failed_stage.as_str(),
            self.error.as_str(),
            self.app_version.as_str(),
            self.platform.as_str(),
        ]
        .join("|")
    }
}

pub fn classify_failure(
    context: FlowFailureContext,
    raw_stage: Option<&str>,
    raw_error_code: Option<&str>,
    raw_error_message: Option<&str>,
    note: Option<String>,
) -> Result<FlowFailureReportDraft> {
    FlowFailureReportDraft::new(
        context,
        normalize_failed_stage(raw_stage),
        classify_error(raw_error_code, raw_error_message),
        note,
    )
}

pub fn draft_from_value(value: &Value, note: Option<String>) -> Result<FlowFailureReportDraft> {
    if value.get("schema_version").is_some() || value.get("schemaVersion").is_some() {
        let mut draft = draft_from_explicit_fields(value)?;
        apply_note_if_missing(&mut draft, note)?;
        return Ok(draft);
    }

    let context = FlowFailureContext {
        surface: optional_string(value, "surface")
            .or_else(|| optional_string(value, "system"))
            .unwrap_or_else(|| "ios".to_string()),
        flow: required_string(value, "flow").or_else(|_| required_string(value, "workflow"))?,
        flow_version: required_string(value, "flow_version")
            .or_else(|_| required_string(value, "flowVersion"))
            .or_else(|_| required_string(value, "workflow_version"))
            .or_else(|_| required_string(value, "workflowVersion"))?,
        app_version: required_string(value, "app_version")
            .or_else(|_| required_string(value, "appVersion"))?,
        platform: optional_string(value, "platform").unwrap_or_else(|| "ios".to_string()),
    };

    classify_failure(
        context,
        optional_string(value, "failed_stage")
            .or_else(|| optional_string(value, "failedStage"))
            .or_else(|| optional_string(value, "failed_step"))
            .or_else(|| optional_string(value, "failedStep"))
            .as_deref(),
        optional_string(value, "error").as_deref(),
        optional_string(value, "message").as_deref(),
        note,
    )
}

pub fn apply_note_if_missing(
    draft: &mut FlowFailureReportDraft,
    note: Option<String>,
) -> Result<()> {
    if draft.note.is_none() {
        draft.note = normalize_note(note)?;
    }
    Ok(())
}

pub fn review_payload(draft: &FlowFailureReportDraft) -> Value {
    let mut fields = vec![
        json!({"label": "Source", "key": "source", "value": draft.source}),
        json!({"label": "Product", "key": "product", "value": draft.product}),
        json!({"label": "Flow kind", "key": "flow_kind", "value": draft.flow_kind}),
        json!({"label": "Surface", "key": "surface", "value": draft.surface}),
        json!({"label": "Flow", "key": "flow", "value": draft.flow}),
        json!({"label": "Flow version", "key": "flow_version", "value": draft.flow_version}),
        json!({"label": "Failed stage", "key": "failed_stage", "value": draft.failed_stage}),
        json!({"label": "Error", "key": "error", "value": draft.error}),
        json!({"label": "App version", "key": "app_version", "value": draft.app_version}),
        json!({"label": "Platform", "key": "platform", "value": draft.platform}),
    ];
    if let Some(note) = &draft.note {
        fields.push(json!({"label": "Optional note", "key": "note", "value": note}));
    }

    json!({
        "title": "Phone automation failure report",
        "valueProposition": VALUE_PROPOSITION,
        "fields": fields,
        "privacyStatement": PRIVACY_STATEMENT,
        "payload": draft,
        "hostEvent": draft.host_event(),
        "manualReportCommand": manual_report_command(draft)
    })
}

pub fn manual_report_command(draft: &FlowFailureReportDraft) -> String {
    let mut parts = vec![
        "rzn-phone".to_string(),
        "report".to_string(),
        "workflow-broken".to_string(),
        "--surface".to_string(),
        shell_quote(&draft.surface),
        "--flow".to_string(),
        shell_quote(&draft.flow),
        "--flow-version".to_string(),
        shell_quote(&draft.flow_version),
        "--failed-stage".to_string(),
        shell_quote(&draft.failed_stage),
        "--error".to_string(),
        shell_quote(&draft.error),
        "--app-version".to_string(),
        shell_quote(&draft.app_version),
        "--platform".to_string(),
        shell_quote(&draft.platform),
        "--dry-run".to_string(),
    ];
    if let Some(note) = &draft.note {
        parts.push("--note".to_string());
        parts.push(shell_quote(note));
    }
    parts.join(" ")
}

fn draft_from_explicit_fields(value: &Value) -> Result<FlowFailureReportDraft> {
    let schema_version = value
        .get("schema_version")
        .or_else(|| value.get("schemaVersion"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if schema_version != 1 {
        bail!("schema_version must be 1");
    }

    validate_fixed(value, "submission_mode", SUBMISSION_MODE)?;
    validate_fixed(value, "source", SOURCE)?;
    validate_fixed(value, "product", PRODUCT)?;
    validate_fixed(value, "flow_kind", FLOW_KIND)?;

    FlowFailureReportDraft::new(
        FlowFailureContext {
            surface: required_string(value, "surface")?,
            flow: required_string(value, "flow")?,
            flow_version: required_string(value, "flow_version")
                .or_else(|_| required_string(value, "flowVersion"))?,
            app_version: required_string(value, "app_version")
                .or_else(|_| required_string(value, "appVersion"))?,
            platform: required_string(value, "platform")?,
        },
        required_string(value, "failed_stage")
            .or_else(|_| required_string(value, "failedStage"))?,
        required_string(value, "error")?,
        value
            .get("note")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    )
}

fn validate_fixed(value: &Value, key: &str, expected: &str) -> Result<()> {
    let got = value
        .get(key)
        .or_else(|| value.get(to_camel(key)))
        .and_then(Value::as_str)
        .unwrap_or(expected);
    if got != expected {
        bail!("{key} must be {expected}");
    }
    Ok(())
}

fn classify_error(raw_error_code: Option<&str>, raw_error_message: Option<&str>) -> String {
    let combined = [
        raw_error_code.unwrap_or(""),
        raw_error_message.unwrap_or(""),
    ]
    .join(" ")
    .to_lowercase();

    if combined.contains("element_not_found")
        || combined.contains("no elements found")
        || combined.contains("no matching elements")
        || combined.contains("not found")
    {
        "element_not_found".to_string()
    } else if combined.contains("element_not_clickable")
        || combined.contains("not clickable")
        || combined.contains("not hittable")
    {
        "element_not_clickable".to_string()
    } else if combined.contains("permission")
        || combined.contains("privacy")
        || combined.contains("access denied")
    {
        "permission_denied".to_string()
    } else if combined.contains("app_not_installed")
        || combined.contains("application is not installed")
    {
        "app_not_installed".to_string()
    } else if combined.contains("app_not_foreground") || combined.contains("not foreground") {
        "app_not_foreground".to_string()
    } else if combined.contains("device_disconnected")
        || combined.contains("device disconnected")
        || combined.contains("socket hang up")
        || combined.contains("connection refused")
        || combined.contains("no active session")
    {
        "device_disconnected".to_string()
    } else if combined.contains("simulator_unavailable")
        || combined.contains("simulator unavailable")
        || combined.contains("simulator is not booted")
    {
        "simulator_unavailable".to_string()
    } else if combined.contains("auth_required")
        || combined.contains("authentication required")
        || combined.contains("login required")
        || combined.contains("sign in")
    {
        "auth_required".to_string()
    } else if combined.contains("timeout") || combined.contains("timed out") {
        "timeout".to_string()
    } else if combined.contains("system prompt")
        || combined.contains("alert")
        || combined.contains("automation mode")
        || combined.contains("locked")
    {
        "blocked_by_system_prompt".to_string()
    } else {
        normalize_error_code(raw_error_code.unwrap_or("unknown_failure"))
    }
}

fn normalize_error_code(raw: &str) -> String {
    let code = stable_id_or_unknown(raw).to_lowercase();
    match code.as_str() {
        "element_not_found"
        | "element_not_clickable"
        | "permission_denied"
        | "app_not_installed"
        | "app_not_foreground"
        | "device_disconnected"
        | "simulator_unavailable"
        | "auth_required"
        | "timeout"
        | "blocked_by_system_prompt"
        | "unknown_failure" => code,
        "elementnotfound" | "no_such_element" | "no_such_window" => "element_not_found".to_string(),
        "devicelocked" | "device_locked" => "blocked_by_system_prompt".to_string(),
        "nosession" | "no_session" => "device_disconnected".to_string(),
        "actionfailed" | "action_failed" | "internal" | "invalidparams" | "invalid_params" => {
            "unknown_failure".to_string()
        }
        _ => "unknown_failure".to_string(),
    }
}

fn normalize_failed_stage(raw_stage: Option<&str>) -> String {
    raw_stage
        .map(stable_id_or_unknown)
        .filter(|stage| stage != "unknown")
        .unwrap_or_else(|| "workflow".to_string())
}

fn stable_id_or_unknown(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '/' || ch == '.' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = out.trim_matches(['_', '/', '.', '-']).to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

fn normalize_platform(raw: &str) -> String {
    match stable_id_or_unknown(raw).as_str() {
        "android" => "android".to_string(),
        _ => "ios".to_string(),
    }
}

fn normalize_note(note: Option<String>) -> Result<Option<String>> {
    let Some(note) = note else {
        return Ok(None);
    };
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_NOTE_CHARS {
        bail!("note must be {MAX_NOTE_CHARS} characters or fewer");
    }
    Ok(Some(trimmed.to_string()))
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    optional_string(value, key).ok_or_else(|| anyhow!("{key} is required"))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn to_camel(key: &str) -> String {
    let mut out = String::new();
    let mut upper_next = false;
    for ch in key.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_context() -> FlowFailureContext {
        FlowFailureContext {
            surface: "ios".to_string(),
            flow: "ios/messages-send-v1".to_string(),
            flow_version: "2026-04-24.1".to_string(),
            app_version: "0.2.5".to_string(),
            platform: "ios".to_string(),
        }
    }

    #[test]
    fn snapshot_minimal_payload() {
        let draft = FlowFailureReportDraft::new(
            sample_context(),
            "tap_recipient",
            "element_not_found",
            None,
        )
        .expect("draft");

        assert_eq!(
            serde_json::to_value(draft).expect("json"),
            json!({
                "schema_version": 1,
                "submission_mode": "host_auto",
                "source": "rzn-phone",
                "product": "rzn-phone",
                "flow_kind": "phone_automation",
                "surface": "ios",
                "flow": "ios/messages-send-v1",
                "flow_version": "2026-04-24.1",
                "failed_stage": "tap_recipient",
                "error": "element_not_found",
                "app_version": "0.2.5",
                "platform": "ios"
            })
        );
    }

    #[test]
    fn forbidden_keys_and_values_are_absent() {
        let draft = draft_from_value(
            &json!({
                "surface": "ios",
                "flow": "ios/messages-send-v1",
                "flow_version": "2026-04-24.1",
                "failed_stage": "tap_recipient",
                "error": "ELEMENT_NOT_FOUND",
                "app_version": "0.2.5",
                "platform": "ios",
                "message_text": "secret message",
                "recipient": "Jane Doe",
                "phone_number": "+15555550123",
                "screenshot": "base64",
                "ocr_text": "private OCR",
                "url": "https://example.test/private",
                "run_id": "trace-123",
                "raw_accessibility_tree": "<xml/>",
                "user_inputs": {"body": "secret message"}
            }),
            None,
        )
        .expect("draft");
        let value = serde_json::to_value(draft).expect("json");
        let raw = serde_json::to_string(&value).expect("string");

        for key in [
            "message_text",
            "recipient",
            "phone_number",
            "screenshot",
            "ocr_text",
            "url",
            "run_id",
            "raw_accessibility_tree",
            "user_inputs",
        ] {
            assert!(!value.as_object().expect("object").contains_key(key));
        }
        for private_value in [
            "secret message",
            "Jane Doe",
            "+15555550123",
            "base64",
            "private OCR",
            "https://example.test/private",
            "trace-123",
            "<xml/>",
        ] {
            assert!(!raw.contains(private_value));
        }
    }

    #[test]
    fn raw_private_failures_normalize_to_safe_error_codes() {
        let draft = classify_failure(
            sample_context(),
            Some("Tap Recipient"),
            Some("ELEMENT_NOT_FOUND"),
            Some("No elements found for label Jane Doe at +15555550123"),
            None,
        )
        .expect("draft");

        assert_eq!(draft.failed_stage, "tap_recipient");
        assert_eq!(draft.error, "element_not_found");
        let raw = serde_json::to_string(&draft).expect("json");
        assert!(!raw.contains("Jane Doe"));
        assert!(!raw.contains("+15555550123"));
    }

    #[test]
    fn review_exposes_host_event_not_backend_submission() {
        let draft = FlowFailureReportDraft::new(sample_context(), "tap_recipient", "timeout", None)
            .expect("draft");
        let review = review_payload(&draft);

        assert_eq!(
            review
                .get("hostEvent")
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str),
            Some(HOST_EVENT_TYPE)
        );
        assert!(review.get("manualReportCommand").is_some());
        assert!(review.get("endpoint").is_none());
    }

    #[test]
    fn host_hook_emits_one_sanitized_draft_on_failure() {
        let draft = classify_failure(
            sample_context(),
            Some("tap recipient"),
            Some("ELEMENT_NOT_FOUND"),
            Some("No elements found for Jane Doe +15555550123"),
            None,
        )
        .expect("draft");
        let event = draft.host_event();

        assert_eq!(
            event.get("type").and_then(Value::as_str),
            Some(HOST_EVENT_TYPE)
        );
        assert_eq!(
            event
                .get("draft")
                .and_then(|value| value.get("error"))
                .and_then(Value::as_str),
            Some("element_not_found")
        );
        let raw = serde_json::to_string(&event).expect("json");
        assert!(!raw.contains("Jane Doe"));
        assert!(!raw.contains("+15555550123"));
    }

    #[test]
    fn emitter_suppresses_duplicate_reports_for_same_failed_run() {
        let draft = FlowFailureReportDraft::new(
            sample_context(),
            "tap_recipient",
            "element_not_found",
            None,
        )
        .expect("draft");
        let mut emitter = FlowFailureReportEmitter::new();

        assert!(emitter.emit_once(&draft).is_some());
        assert!(emitter.emit_once(&draft).is_none());
    }

    #[test]
    fn privacy_failure_artifact_policy_defaults_to_minimal_redacted_mode() {
        assert_eq!(
            FailureArtifactPolicy::parse(""),
            FailureArtifactPolicy::Minimal
        );
        assert_eq!(
            FailureArtifactPolicy::parse("minimal"),
            FailureArtifactPolicy::Minimal
        );
        assert_eq!(
            FailureArtifactPolicy::parse("off"),
            FailureArtifactPolicy::Off
        );
        assert_eq!(
            FailureArtifactPolicy::parse("full"),
            FailureArtifactPolicy::Full
        );
        assert!(!FailureArtifactPolicy::Minimal.captures_screenshot());
        assert!(!FailureArtifactPolicy::Minimal.captures_ui_source());
        assert!(FailureArtifactPolicy::Full.captures_screenshot());
        assert!(FailureArtifactPolicy::Full.captures_ui_source());
    }
}
