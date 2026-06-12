use serde_json::{json, Value};
use std::env;

use crate::errors::{ToolCallError, ToolErrorCode};

pub const TRUSTED_DIRECT_TOOLS_ENV: &str = "RZN_PHONE_TRUSTED_DIRECT_TOOLS";
pub const PRIVACY_GATES_ENV: &str = "RZN_PHONE_PRIVACY_GATES";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    Low,
    Medium,
    High,
}

impl ToolRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolRisk::Low => "low",
            ToolRisk::Medium => "medium",
            ToolRisk::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyClass {
    Public,
    Messages,
    Otp,
    Calls,
    Notifications,
}

impl PrivacyClass {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyClass::Public => "public",
            PrivacyClass::Messages => "messages",
            PrivacyClass::Otp => "otp",
            PrivacyClass::Calls => "calls",
            PrivacyClass::Notifications => "notifications",
        }
    }

    fn is_private(self) -> bool {
        self != PrivacyClass::Public
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPolicy {
    pub mutating: bool,
    pub risk: ToolRisk,
    pub privacy_class: PrivacyClass,
    pub allowed_direct: bool,
    pub allowed_in_workflow: bool,
}

impl ToolPolicy {
    fn read_only(risk: ToolRisk) -> Self {
        Self {
            mutating: false,
            risk,
            privacy_class: PrivacyClass::Public,
            allowed_direct: true,
            allowed_in_workflow: true,
        }
    }

    fn mutating(risk: ToolRisk) -> Self {
        Self {
            mutating: true,
            risk,
            privacy_class: PrivacyClass::Public,
            allowed_direct: true,
            allowed_in_workflow: true,
        }
    }

    fn private(privacy_class: PrivacyClass) -> Self {
        Self {
            mutating: false,
            risk: ToolRisk::High,
            privacy_class,
            allowed_direct: true,
            allowed_in_workflow: true,
        }
    }

    pub fn metadata(self) -> Value {
        json!({
            "mutating": self.mutating,
            "risk": self.risk.as_str(),
            "privacyClass": self.privacy_class.as_str(),
            "allowedDirect": self.allowed_direct,
            "allowedInWorkflow": self.allowed_in_workflow,
            "directCommitArgument": "commit",
            "privacyGateArgument": "privacyGate",
            "trustedDirectOverrideEnv": TRUSTED_DIRECT_TOOLS_ENV,
            "privacyGatesEnv": PRIVACY_GATES_ENV
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallContext {
    Direct,
    Workflow,
}

pub fn policy_for_tool(name: &str) -> ToolPolicy {
    let mut policy = if is_direct_mutation_tool(name) {
        ToolPolicy::mutating(ToolRisk::Medium)
    } else if name == "ios.web.eval_js" {
        ToolPolicy::read_only(ToolRisk::High)
    } else if name == "phone_messages.find_recent_otp" {
        ToolPolicy::private(PrivacyClass::Otp)
    } else if name.starts_with("phone_messages.") {
        ToolPolicy::private(PrivacyClass::Messages)
    } else if name.starts_with("phone_calls.") {
        ToolPolicy::private(PrivacyClass::Calls)
    } else if name.starts_with("phone_notifications.") {
        ToolPolicy::private(PrivacyClass::Notifications)
    } else if name == "ios.script.run" {
        ToolPolicy::mutating(ToolRisk::High)
    } else if name == "rzn.worker.shutdown"
        || name == "ios.session.delete"
        || name == "ios.wda.shutdown"
    {
        ToolPolicy::mutating(ToolRisk::Low)
    } else {
        ToolPolicy::read_only(ToolRisk::Low)
    };

    if name == "ios.web.eval_js" {
        policy.allowed_direct = false;
        policy.allowed_in_workflow = true;
    }
    if name == "ios.workflow.run" || name == "ios.script.run" {
        policy.allowed_in_workflow = false;
    }

    policy
}

pub fn metadata_for_tool(name: &str) -> Value {
    policy_for_tool(name).metadata()
}

pub fn augment_input_schema(name: &str, mut input_schema: Value) -> Value {
    let policy = policy_for_tool(name);
    let Some(schema) = input_schema.as_object_mut() else {
        return input_schema;
    };
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return input_schema;
    };

    if policy.mutating {
        properties.entry("commit".to_string()).or_insert_with(|| {
            json!({
                "type": "boolean",
                "default": false,
                "description": "Required for direct mutating tool calls unless the runtime is started with RZN_PHONE_TRUSTED_DIRECT_TOOLS=1."
            })
        });
    }

    if policy.privacy_class.is_private() {
        properties
            .entry("privacyGate".to_string())
            .or_insert_with(|| {
                json!({
                    "type": "string",
                    "enum": [policy.privacy_class.as_str()],
                    "description": "Required privacy grant for this private phone-data tool."
                })
            });
        properties
            .entry("privacyGates".to_string())
            .or_insert_with(|| {
                json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Alternative list of granted privacy classes."
                })
            });
    }

    input_schema
}

pub fn enforce_direct_tool_policy(tool_name: &str, arguments: &Value) -> Result<(), ToolCallError> {
    enforce_tool_policy(tool_name, arguments, ToolCallContext::Direct)
}

pub fn enforce_workflow_tool_policy(
    tool_name: &str,
    workflow_vars: &Value,
) -> Result<(), ToolCallError> {
    enforce_tool_policy(tool_name, workflow_vars, ToolCallContext::Workflow)
}

fn enforce_tool_policy(
    tool_name: &str,
    arguments: &Value,
    context: ToolCallContext,
) -> Result<(), ToolCallError> {
    let policy = policy_for_tool(tool_name);
    let trusted_direct = context == ToolCallContext::Direct && trusted_direct_override_enabled();

    if context == ToolCallContext::Direct && !policy.allowed_direct && !trusted_direct {
        return Err(policy_denied(
            tool_name,
            policy,
            "tool is disabled for direct MCP/tool calls; use a workflow or start the runtime in trusted direct-tool mode",
        ));
    }

    if context == ToolCallContext::Workflow && !policy.allowed_in_workflow {
        return Err(policy_denied(
            tool_name,
            policy,
            "tool is not allowed inside workflow/script steps",
        ));
    }

    if context == ToolCallContext::Direct
        && policy.risk == ToolRisk::High
        && !policy.privacy_class.is_private()
        && !trusted_direct
    {
        return Err(policy_denied(
            tool_name,
            policy,
            "high-risk direct tool requires trusted direct-tool mode",
        ));
    }

    if context == ToolCallContext::Direct
        && policy.mutating
        && !direct_commit_requested(arguments)
        && !trusted_direct
    {
        return Err(ToolCallError::new(
            ToolErrorCode::CommitRequired,
            format!("direct mutating tool '{tool_name}' requires commit=true"),
            policy_details(tool_name, policy),
        ));
    }

    if policy.privacy_class.is_private()
        && !privacy_gate_present(arguments, policy.privacy_class)
        && !privacy_gate_env_present(policy.privacy_class)
    {
        return Err(policy_denied(
            tool_name,
            policy,
            format!(
                "tool requires privacyGate='{}' or {} containing '{}'",
                policy.privacy_class.as_str(),
                PRIVACY_GATES_ENV,
                policy.privacy_class.as_str()
            ),
        ));
    }

    Ok(())
}

fn is_direct_mutation_tool(name: &str) -> bool {
    name == "ios.action.tap"
        || name == "ios.action.type"
        || name == "ios.action.typeahead"
        || name == "ios.web.click_css"
        || name == "ios.web.type_css"
        || name == "ios.web.press_key"
        || name == "ios.alert.accept"
        || name == "ios.alert.dismiss"
}

fn direct_commit_requested(arguments: &Value) -> bool {
    arguments
        .get("commit")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn trusted_direct_override_enabled() -> bool {
    env_truthy(TRUSTED_DIRECT_TOOLS_ENV)
}

fn env_truthy(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn privacy_gate_present(arguments: &Value, privacy_class: PrivacyClass) -> bool {
    string_gate_matches(arguments.get("privacyGate"), privacy_class)
        || string_gate_matches(arguments.get("privacyClass"), privacy_class)
        || array_gate_matches(arguments.get("privacyGates"), privacy_class)
}

fn string_gate_matches(value: Option<&Value>, privacy_class: PrivacyClass) -> bool {
    value
        .and_then(Value::as_str)
        .map(|gate| gate.eq_ignore_ascii_case(privacy_class.as_str()))
        .unwrap_or(false)
}

fn array_gate_matches(value: Option<&Value>, privacy_class: PrivacyClass) -> bool {
    value
        .and_then(Value::as_array)
        .map(|gates| {
            gates
                .iter()
                .any(|gate| string_gate_matches(Some(gate), privacy_class))
        })
        .unwrap_or(false)
}

fn privacy_gate_env_present(privacy_class: PrivacyClass) -> bool {
    env::var(PRIVACY_GATES_ENV)
        .map(|value| {
            value
                .split(|ch: char| ch == ',' || ch == ';' || ch.is_ascii_whitespace())
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .any(|part| {
                    part.eq_ignore_ascii_case("all")
                        || part.eq_ignore_ascii_case(privacy_class.as_str())
                })
        })
        .unwrap_or(false)
}

fn policy_denied(tool_name: &str, policy: ToolPolicy, message: impl Into<String>) -> ToolCallError {
    ToolCallError::new(
        ToolErrorCode::PolicyDenied,
        message.into(),
        policy_details(tool_name, policy),
    )
}

fn policy_details(tool_name: &str, policy: ToolPolicy) -> Value {
    json!({
        "tool": tool_name,
        "policy": policy.metadata()
    })
}

pub fn tool_capability_family(name: &str) -> &'static str {
    if name.starts_with("rzn.worker.")
        || name.starts_with("ios.env.")
        || name.starts_with("ios.device.")
        || name.starts_with("ios.appium.")
        || name.starts_with("ios.session.")
        || name.starts_with("ios.wda.")
    {
        "session"
    } else if name.starts_with("ios.ui.observe")
        || name == "ios.ui.source"
        || name == "ios.ui.screenshot"
        || name == "ios.web.screenshot"
        || name == "ios.target.resolve"
    {
        "observe"
    } else if name.starts_with("ios.web.goto")
        || name.starts_with("ios.action.back")
        || name.starts_with("ios.action.scroll")
        || name.starts_with("ios.action.swipe")
        || name.starts_with("ios.action.scroll_until")
        || name.starts_with("ios.app.activate")
    {
        "navigate"
    } else if name.starts_with("ios.ui.extract")
        || name == "ios.ui.find_row"
        || name.starts_with("ios.element.")
        || name == "ios.web.page_source"
        || name == "ios.web.eval_js"
    {
        "extract"
    } else if name.starts_with("ios.action.tap")
        || name.starts_with("ios.action.type")
        || name.starts_with("ios.web.click_css")
        || name.starts_with("ios.web.type_css")
        || name.starts_with("ios.web.press_key")
        || name.starts_with("ios.alert.accept")
        || name.starts_with("ios.alert.dismiss")
    {
        "interact"
    } else if name.starts_with("ios.action.wait")
        || name.starts_with("ios.web.wait_")
        || name.starts_with("ios.alert.wait")
        || name.starts_with("ios.alert.text")
    {
        "verify"
    } else if name.starts_with("ios.workflow.")
        || name.starts_with("rzn.workflow_failure_report.")
        || name.starts_with("ios.script.")
        || name.starts_with("phone_messages.")
        || name.starts_with("phone_calls.")
        || name.starts_with("phone_notifications.")
    {
        "workflow"
    } else if name.starts_with("util.") {
        "utility"
    } else {
        "other"
    }
}

pub fn planner_capability_families() -> Vec<Value> {
    vec![
        json!({
            "id": "observe",
            "tier": 1,
            "description": "Capture a compact understanding of the current page or screen before acting.",
            "examples": ["compact_scene", "ui_bundle", "accessibility_tree"]
        }),
        json!({
            "id": "navigate",
            "tier": 1,
            "description": "Move through apps, pages, tabs, and view hierarchies without mutating remote state.",
            "examples": ["open", "back", "scroll"]
        }),
        json!({
            "id": "extract",
            "tier": 1,
            "description": "Turn the current surface into normalized structured data such as lists, fields, or entity summaries.",
            "examples": ["list", "entity", "field"]
        }),
        json!({
            "id": "interact",
            "tier": 1,
            "description": "Activate controls or provide input such as taps, typing, and key presses.",
            "examples": ["activate", "input", "submit"]
        }),
        json!({
            "id": "verify",
            "tier": 1,
            "description": "Check whether a UI state or transition has happened yet.",
            "examples": ["wait", "state_check", "change_check"]
        }),
        json!({
            "id": "session",
            "tier": 1,
            "description": "Start, stop, or inspect runtime and device automation sessions.",
            "examples": ["start", "end", "health"]
        }),
        json!({
            "id": "workflow",
            "tier": 1,
            "description": "Run packaged multi-step flows or app-level cards built from lower-level primitives.",
            "examples": ["browse_card", "read_card", "engage_card"]
        }),
        json!({
            "id": "utility",
            "tier": 1,
            "description": "Small helper transforms used by workflows; not primary planner verbs.",
            "examples": ["count", "rank", "sleep"]
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mutation_requires_commit() {
        let err = enforce_direct_tool_policy("ios.action.tap", &json!({}))
            .expect_err("tap should require commit");
        assert_eq!(err.code, ToolErrorCode::CommitRequired);
    }

    #[test]
    fn direct_mutation_allows_commit() {
        enforce_direct_tool_policy("ios.action.tap", &json!({"commit": true}))
            .expect("commit should satisfy direct mutation gate");
    }

    #[test]
    fn eval_js_is_disabled_for_direct_calls() {
        let err = enforce_direct_tool_policy("ios.web.eval_js", &json!({"script": "1"}))
            .expect_err("eval_js should be gated");
        assert_eq!(err.code, ToolErrorCode::PolicyDenied);
    }

    #[test]
    fn private_tools_require_matching_privacy_gate() {
        let err = enforce_direct_tool_policy("phone_messages.find_recent_otp", &json!({}))
            .expect_err("otp should require a privacy gate");
        assert_eq!(err.code, ToolErrorCode::PolicyDenied);

        enforce_direct_tool_policy(
            "phone_messages.find_recent_otp",
            &json!({"privacyGate": "otp"}),
        )
        .expect("otp privacy gate should pass");
    }
}
