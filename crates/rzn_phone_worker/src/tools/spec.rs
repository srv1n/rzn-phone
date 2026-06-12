use serde_json::Value;

use super::registry;

pub fn list_tool_definitions() -> Vec<Value> {
    registry::list_tool_definitions()
}
