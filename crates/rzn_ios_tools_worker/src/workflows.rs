use anyhow::{bail, Result};
use serde_json::Value;
use std::{collections::HashMap, env, fs, path::PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

pub fn list_workflows() -> Vec<WorkflowInfo> {
    let mut out = list_file_workflows();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn run_workflow(_name: &str) -> Result<Value> {
    bail!("workflow implementations are data-only; ensure the JSON workflow has steps")
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct WorkflowInputDefinition {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileWorkflowDefinition {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: HashMap<String, WorkflowInputDefinition>,
    #[serde(default)]
    pub steps: Option<Vec<Value>>,
    #[serde(default)]
    pub output: Option<Value>,
}

pub fn merge_input_defaults(def: &FileWorkflowDefinition, vars: &mut Value) -> Result<()> {
    let Some(vars_obj) = vars.as_object_mut() else {
        bail!("workflow vars must be an object");
    };

    for (name, input) in &def.inputs {
        let missing =
            vars_obj.get(name).is_none() || vars_obj.get(name).is_some_and(Value::is_null);
        if !missing {
            continue;
        }

        if let Some(default) = &input.default {
            vars_obj.insert(name.clone(), default.clone());
            continue;
        }

        if input.required {
            bail!("workflow input '{name}' is required");
        }
    }

    Ok(())
}

pub fn load_file_workflow(name: &str) -> Option<FileWorkflowDefinition> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }

    for dir in workflow_search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(def) = serde_json::from_str::<FileWorkflowDefinition>(&raw) else {
                continue;
            };
            if def.name == want {
                return Some(def);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{merge_input_defaults, FileWorkflowDefinition};
    use serde_json::json;

    #[test]
    fn merge_input_defaults_injects_missing_values() {
        let def: FileWorkflowDefinition = serde_json::from_value(json!({
            "name": "test.workflow",
            "version": "1.0.0",
            "inputs": {
                "flag": { "type": "boolean", "default": false },
                "maxScrolls": { "type": "integer", "default": 8 }
            },
            "steps": []
        }))
        .expect("workflow definition");

        let mut vars = json!({});
        merge_input_defaults(&def, &mut vars).expect("defaults merged");

        assert_eq!(vars.get("flag"), Some(&json!(false)));
        assert_eq!(vars.get("maxScrolls"), Some(&json!(8)));
    }

    #[test]
    fn merge_input_defaults_rejects_missing_required_inputs() {
        let def: FileWorkflowDefinition = serde_json::from_value(json!({
            "name": "test.workflow",
            "version": "1.0.0",
            "inputs": {
                "review_title": { "type": "string", "required": true }
            },
            "steps": []
        }))
        .expect("workflow definition");

        let mut vars = json!({});
        let err = merge_input_defaults(&def, &mut vars).expect_err("required input error");
        assert!(err.to_string().contains("review_title"));
    }
}

fn list_file_workflows() -> Vec<WorkflowInfo> {
    let mut out = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for dir in workflow_search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(def) = serde_json::from_str::<FileWorkflowDefinition>(&raw) else {
                continue;
            };
            if def.name.trim().is_empty() {
                continue;
            }
            if seen.contains_key(&def.name) {
                continue;
            }
            seen.insert(def.name.clone(), ());
            out.push(WorkflowInfo {
                name: def.name,
                version: def.version,
                description: if def.description.trim().is_empty() {
                    "Workflow loaded from JSON pack.".to_string()
                } else {
                    def.description
                },
            });
        }
    }

    out
}

fn workflow_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(plugin_dir) = env::var("RZN_PLUGIN_DIR") {
        let root = PathBuf::from(plugin_dir);
        dirs.push(root.join("resources").join("workflows"));
    }
    if let Ok(plugin_root) = env::var("CLAUDE_PLUGIN_ROOT") {
        let root = PathBuf::from(plugin_root);
        dirs.push(root.join("resources").join("workflows"));
    }

    // Dev fallback (repo root as cwd in claude_plugin/.mcp.json).
    dirs.push(PathBuf::from(
        "crates/rzn_ios_tools_worker/resources/workflows",
    ));

    if let Ok(extra) = env::var("RZN_IOS_WORKFLOW_DIRS") {
        for raw in extra.split(':') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            dirs.push(PathBuf::from(trimmed));
        }
    }

    dirs.into_iter()
        .filter(|dir| dir.exists() && dir.is_dir())
        .fold(Vec::new(), |mut dedup, entry| {
            if !dedup.contains(&entry) {
                dedup.push(entry);
            }
            dedup
        })
}
