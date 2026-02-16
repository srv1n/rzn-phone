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
pub struct FileWorkflowDefinition {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub steps: Option<Vec<Value>>,
    #[serde(default)]
    pub output: Option<Value>,
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
    dirs.push(PathBuf::from("crates/rzn_ios_tools_worker/resources/workflows"));

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
