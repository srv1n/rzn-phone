use serde_json::{json, Value};

use super::{action, phone_data, policy, script, session, ui, utility, web, workflow};

pub(crate) fn list_tool_definitions() -> Vec<Value> {
    let mut definitions = Vec::new();
    definitions.extend(session::definitions());
    definitions.extend(ui::definitions());
    definitions.extend(action::definitions());
    definitions.extend(web::definitions());
    definitions.extend(workflow::definitions());
    definitions.extend(script::definitions());
    definitions.extend(phone_data::definitions());
    definitions.extend(utility::definitions());
    definitions
}

#[cfg(test)]
pub(crate) fn registered_tool_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    names.extend_from_slice(session::TOOL_NAMES);
    names.extend_from_slice(ui::TOOL_NAMES);
    names.extend_from_slice(action::TOOL_NAMES);
    names.extend_from_slice(web::TOOL_NAMES);
    names.extend_from_slice(workflow::TOOL_NAMES);
    names.extend_from_slice(script::TOOL_NAMES);
    names.extend_from_slice(phone_data::TOOL_NAMES);
    names.extend_from_slice(utility::TOOL_NAMES);
    names
}

pub(crate) fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    let policy = policy::metadata_for_tool(name);
    let input_schema = policy::augment_input_schema(name, input_schema);
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "capabilityFamily": policy::tool_capability_family(name),
        "capabilityTier": 2,
        "mutating": policy.get("mutating").cloned().unwrap_or(Value::Bool(false)),
        "risk": policy.get("risk").cloned().unwrap_or_else(|| json!("low")),
        "privacyClass": policy.get("privacyClass").cloned().unwrap_or_else(|| json!("public")),
        "allowedDirect": policy.get("allowedDirect").cloned().unwrap_or(Value::Bool(true)),
        "allowedInWorkflow": policy.get("allowedInWorkflow").cloned().unwrap_or(Value::Bool(true)),
        "policy": policy
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registry_definitions_match_family_tool_names() {
        let names: BTreeSet<String> = registered_tool_names()
            .into_iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            names.len(),
            registered_tool_names().len(),
            "duplicate registry tool name"
        );

        let definition_names: BTreeSet<String> = list_tool_definitions()
            .into_iter()
            .map(|definition| {
                definition
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("definition name")
                    .to_string()
            })
            .collect();

        assert_eq!(definition_names, names);
    }

    #[test]
    fn registry_matches_dispatch_table() {
        let registry_names: BTreeSet<String> = registered_tool_names()
            .into_iter()
            .map(ToString::to_string)
            .collect();
        let dispatched_names = dispatched_tool_names_from_source();

        assert_eq!(registry_names, dispatched_names);
    }

    fn dispatched_tool_names_from_source() -> BTreeSet<String> {
        let source = include_str!("../tools.rs");
        let start = source
            .find("async fn handle_tool_call_unchecked")
            .expect("dispatch function");
        let end = source[start..]
            .find("fn tool_success")
            .map(|offset| start + offset)
            .expect("end of dispatch area");

        source[start..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                if !line.starts_with('"') {
                    return None;
                }
                let rest = &line[1..];
                let end_quote = rest.find('"')?;
                let name = &rest[..end_quote];
                let after = rest[end_quote + 1..].trim_start();
                after.starts_with("=>").then(|| name.to_string())
            })
            .collect()
    }
}
