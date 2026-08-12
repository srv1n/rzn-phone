use serde_json::{json, Value};

use super::{action, phone_data, policy, script, session, ui, utility, web, workflow};

pub fn list_tool_definitions() -> Vec<Value> {
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
    #[test]
    fn definitions_have_unique_names() {
        let mut names = std::collections::BTreeSet::new();
        for definition in list_tool_definitions() {
            let name = definition
                .get("name")
                .and_then(Value::as_str)
                .expect("definition name")
                .to_owned();
            assert!(names.insert(name.clone()), "duplicate tool name: {name}");
        }
    }
}
