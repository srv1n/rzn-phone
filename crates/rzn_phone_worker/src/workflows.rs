#![allow(clippy::items_after_test_module)]

use anyhow::{bail, Result};
use serde_json::Value;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowInfo {
    pub id: String,
    pub system: String,
    pub workflow: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub required_variables: Vec<String>,
    pub inputs: HashMap<String, WorkflowInputDefinition>,
    pub capability: Option<WorkflowCapabilityDefinition>,
    pub notes: Vec<String>,
    pub help: Option<WorkflowHelpDefinition>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowSystemInfo {
    pub id: String,
    pub workflow_count: usize,
    pub workflows: Vec<WorkflowInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowLoadDiagnostic {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
struct WorkflowCatalog {
    entries: Vec<WorkflowCatalogEntry>,
    diagnostics: Vec<WorkflowLoadDiagnostic>,
}

#[derive(Debug, Clone)]
struct WorkflowCatalogEntry {
    def: FileWorkflowDefinition,
    info: WorkflowInfo,
}

#[derive(Debug, Clone)]
struct WorkflowCatalogCache {
    signature: WorkflowCatalogSignature,
    catalog: WorkflowCatalog,
}

type WorkflowCatalogSignature = Vec<WorkflowDirSignature>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkflowDirSignature {
    path: PathBuf,
    files: Vec<WorkflowFileSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkflowFileSignature {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

static WORKFLOW_CATALOG_CACHE: once_cell::sync::Lazy<
    std::sync::Mutex<Option<WorkflowCatalogCache>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowReference {
    pub system: String,
    pub workflow: String,
}

impl WorkflowReference {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().replace('\\', "/");
        if trimmed.is_empty() {
            return None;
        }

        let (system, workflow) = if let Some((system, workflow)) = trimmed.split_once('/') {
            (system.trim(), workflow.trim())
        } else if let Some((system, workflow)) = trimmed.split_once('.') {
            (system.trim(), workflow.trim())
        } else {
            return None;
        };

        Self::from_parts(system, workflow)
    }

    fn from_parts(system: &str, workflow: &str) -> Option<Self> {
        let system = system.trim().trim_matches(['/', '.']).to_string();
        let workflow = workflow.trim().trim_matches(['/', '.']).to_string();
        if system.is_empty() || workflow.is_empty() {
            return None;
        }

        Some(Self { system, workflow })
    }

    pub fn canonical_id(&self) -> String {
        format!("{}/{}", self.system, self.workflow)
    }

    pub fn legacy_name(&self) -> String {
        format!("{}.{}", self.system, self.workflow)
    }
}

pub fn compose_workflow_reference(system: &str, workflow: &str) -> Option<String> {
    WorkflowReference::from_parts(system, workflow).map(|reference| reference.canonical_id())
}

pub fn list_workflows(
    system_filter: Option<&str>,
    family_filter: Option<&str>,
) -> Vec<WorkflowInfo> {
    let (mut out, diagnostics) = list_file_workflows(system_filter, family_filter);
    for diagnostic in diagnostics {
        eprintln!(
            "rzn-phone: skipped workflow file {}: {}",
            diagnostic.path, diagnostic.reason
        );
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn list_workflow_diagnostics() -> Vec<WorkflowLoadDiagnostic> {
    let (_, diagnostics) = list_file_workflows(None, None);
    diagnostics
}

pub fn group_workflows_by_system(workflows: &[WorkflowInfo]) -> Vec<WorkflowSystemInfo> {
    let mut grouped = HashMap::<String, Vec<WorkflowInfo>>::new();

    for workflow in workflows {
        grouped
            .entry(workflow.system.clone())
            .or_default()
            .push(workflow.clone());
    }

    let mut systems = grouped
        .into_iter()
        .map(|(id, mut workflows)| {
            workflows.sort_by(|a, b| a.id.cmp(&b.id));
            WorkflowSystemInfo {
                id,
                workflow_count: workflows.len(),
                workflows,
            }
        })
        .collect::<Vec<_>>();

    systems.sort_by(|a, b| a.id.cmp(&b.id));
    systems
}

pub async fn run_workflow(_name: &str) -> Result<Value> {
    bail!("workflow implementations are data-only; ensure the JSON workflow has steps")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowInputDefinition {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub example: Option<Value>,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowCapabilityDefinition {
    pub family: String,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub mutating: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowExampleDefinition {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub args: HashMap<String, Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowParameterHelpDefinition {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub example: Option<Value>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub structure: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowHelpDefinition {
    #[serde(default)]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub returns: Option<String>,
    #[serde(default)]
    pub parameters: HashMap<String, WorkflowParameterHelpDefinition>,
    #[serde(default)]
    pub examples: Vec<WorkflowExampleDefinition>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileWorkflowDefinition {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required_variables: Vec<String>,
    #[serde(default)]
    pub inputs: HashMap<String, WorkflowInputDefinition>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub steps: Option<Vec<Value>>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub presentation: Option<Value>,
    #[serde(default)]
    pub capability: Option<WorkflowCapabilityDefinition>,
    #[serde(default)]
    pub help: Option<WorkflowHelpDefinition>,
}

pub fn merge_input_defaults(def: &FileWorkflowDefinition, vars: &mut Value) -> Result<()> {
    let Some(vars_obj) = vars.as_object_mut() else {
        bail!("workflow vars must be an object");
    };
    let required_variables = normalized_required_variables(&def.required_variables, &def.inputs);

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

        if input.required || required_variables.iter().any(|item| item == name) {
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

    let catalog = cached_file_workflow_catalog();
    for entry in catalog.entries {
        if workflow_matches(&entry.def.name, want) {
            return Some(entry.def);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{
        compose_workflow_reference, group_workflows_by_system, merge_input_defaults,
        workflow_info_from_definition, workflow_matches, FileWorkflowDefinition, WorkflowInfo,
        WorkflowReference,
    };
    use once_cell::sync::Lazy;
    use serde_json::json;
    use std::sync::Mutex as StdMutex;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{collections::HashMap, env, fs, path::PathBuf};

    static ENV_LOCK: Lazy<StdMutex<()>> = Lazy::new(|| StdMutex::new(()));

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

    #[test]
    fn workflow_reference_parses_dot_and_slash_forms() {
        let slash = WorkflowReference::parse("google_maps/open_place").expect("slash ref");
        let dot = WorkflowReference::parse("google_maps.open_place").expect("dot ref");

        assert_eq!(slash, dot);
        assert_eq!(slash.canonical_id(), "google_maps/open_place");
        assert_eq!(slash.legacy_name(), "google_maps.open_place");
    }

    #[test]
    fn compose_workflow_reference_normalizes_parts() {
        assert_eq!(
            compose_workflow_reference(" google_maps ", "/open_place/"),
            Some("google_maps/open_place".to_string())
        );
    }

    #[test]
    fn workflow_matches_accepts_canonical_slash_ref() {
        assert!(workflow_matches(
            "safari.google_search",
            "safari/google_search"
        ));
    }

    #[test]
    fn workflow_info_carries_capability_metadata() {
        let def: FileWorkflowDefinition = serde_json::from_value(json!({
            "name": "safari.google_search",
            "version": "1.0.0",
            "capability": {
                "family": "extract",
                "intent": "search_results",
                "surface": "web",
                "mutating": false
            }
        }))
        .expect("workflow definition");

        let info = workflow_info_from_definition(def);
        let capability = info.capability.expect("capability");
        assert_eq!(capability.family, "extract");
        assert_eq!(capability.intent.as_deref(), Some("search_results"));
        assert_eq!(capability.surface.as_deref(), Some("web"));
        assert_eq!(capability.mutating, Some(false));
    }

    #[test]
    fn workflow_info_carries_help_metadata() {
        let def: FileWorkflowDefinition = serde_json::from_value(json!({
            "name": "safari.google_search",
            "version": "1.0.0",
            "description": "Search Google in Safari.",
            "required_variables": ["query"],
            "inputs": {
                "query": {
                    "type": "string"
                }
            },
            "notes": ["Read-only."],
            "help": {
                "when_to_use": "Use this when you want quick Google results from the phone.",
                "returns": "Ordered organic results.",
                "parameters": {
                    "query": {
                        "description": "What to search for.",
                        "example": "best headphones 2026",
                        "group": "core",
                        "structure": "Plain search string."
                    }
                },
                "examples": [
                    {
                        "label": "Basic search",
                        "args": {
                            "query": "best headphones 2026"
                        }
                    }
                ]
            }
        }))
        .expect("workflow definition");

        let info = workflow_info_from_definition(def);
        let query = info.inputs.get("query").expect("query input");
        assert_eq!(query.description.as_deref(), Some("What to search for."));
        assert_eq!(query.group.as_deref(), Some("core"));
        assert_eq!(info.required_variables, vec!["query".to_string()]);
        assert!(query.required);
        assert_eq!(info.notes, vec!["Read-only.".to_string()]);
        let help = info.help.expect("help");
        assert_eq!(
            help.when_to_use.as_deref(),
            Some("Use this when you want quick Google results from the phone.")
        );
        assert_eq!(help.returns.as_deref(), Some("Ordered organic results."));
        assert_eq!(
            help.parameters
                .get("query")
                .and_then(|item| item.structure.as_deref()),
            Some("Plain search string.")
        );
        assert_eq!(help.examples.len(), 1);
    }

    #[test]
    fn group_workflows_by_system_returns_sorted_groups() {
        let workflows = vec![
            WorkflowInfo {
                id: "reddit/open_post".to_string(),
                system: "reddit".to_string(),
                workflow: "open_post".to_string(),
                name: "reddit.open_post".to_string(),
                version: "1.0.0".to_string(),
                description: "Open a Reddit post".to_string(),
                required_variables: Vec::new(),
                inputs: HashMap::new(),
                capability: None,
                notes: Vec::new(),
                help: None,
            },
            WorkflowInfo {
                id: "appstore/search_results".to_string(),
                system: "appstore".to_string(),
                workflow: "search_results".to_string(),
                name: "appstore.search_results".to_string(),
                version: "1.0.0".to_string(),
                description: "Search App Store".to_string(),
                required_variables: Vec::new(),
                inputs: HashMap::new(),
                capability: None,
                notes: Vec::new(),
                help: None,
            },
            WorkflowInfo {
                id: "reddit/comment_post".to_string(),
                system: "reddit".to_string(),
                workflow: "comment_post".to_string(),
                name: "reddit.comment_post".to_string(),
                version: "1.0.0".to_string(),
                description: "Comment on a Reddit post".to_string(),
                required_variables: Vec::new(),
                inputs: HashMap::new(),
                capability: None,
                notes: Vec::new(),
                help: None,
            },
        ];

        let systems = group_workflows_by_system(&workflows);
        assert_eq!(systems.len(), 2);
        assert_eq!(systems[0].id, "appstore");
        assert_eq!(systems[0].workflow_count, 1);
        assert_eq!(systems[1].id, "reddit");
        assert_eq!(systems[1].workflow_count, 2);
        assert_eq!(systems[1].workflows[0].id, "reddit/comment_post");
        assert_eq!(systems[1].workflows[1].id, "reddit/open_post");
    }

    #[test]
    fn shipped_workflows_declare_complete_capability_metadata() {
        let workflow_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/workflows");
        let allowed_families = [
            "observe", "navigate", "extract", "interact", "verify", "session", "workflow",
            "utility",
        ];
        let mut missing_capability = Vec::new();
        let mut missing_intent = Vec::new();
        let mut missing_surface = Vec::new();
        let mut missing_mutating = Vec::new();
        let mut invalid_family = Vec::new();

        for entry in fs::read_dir(&workflow_dir).expect("workflow dir") {
            let path = entry.expect("workflow entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let raw = fs::read_to_string(&path).expect("workflow file");
            let def: FileWorkflowDefinition =
                serde_json::from_str(&raw).expect("workflow definition");
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>");

            let Some(capability) = def.capability else {
                missing_capability.push(file_name.to_string());
                continue;
            };

            if !allowed_families.contains(&capability.family.as_str()) {
                invalid_family.push(format!("{file_name}:{}", capability.family));
            }
            if capability
                .intent
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                missing_intent.push(file_name.to_string());
            }
            if capability
                .surface
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                missing_surface.push(file_name.to_string());
            }
            if capability.mutating.is_none() {
                missing_mutating.push(file_name.to_string());
            }
        }

        assert!(
            missing_capability.is_empty(),
            "missing capability metadata: {missing_capability:?}"
        );
        assert!(
            invalid_family.is_empty(),
            "invalid capability families: {invalid_family:?}"
        );
        assert!(
            missing_intent.is_empty(),
            "missing capability.intent: {missing_intent:?}"
        );
        assert!(
            missing_surface.is_empty(),
            "missing capability.surface: {missing_surface:?}"
        );
        assert!(
            missing_mutating.is_empty(),
            "missing capability.mutating: {missing_mutating:?}"
        );
    }

    #[test]
    fn runtime_guardrails_local_workflow_dirs_take_override_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_extra = env::var_os("RZN_IOS_WORKFLOW_DIRS");
        let old_plugin = env::var_os("RZN_PLUGIN_DIR");
        let old_plugin_root = env::var_os("CLAUDE_PLUGIN_ROOT");
        let root = env::temp_dir().join(format!(
            "rzn-phone-workflows-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let local = root.join("local");
        let plugin = root.join("plugin").join("resources").join("workflows");
        fs::create_dir_all(&local).expect("local dir");
        fs::create_dir_all(&plugin).expect("plugin dir");
        fs::write(
            local.join("demo_open.json"),
            json!({
                "name": "demo.open",
                "version": "local",
                "description": "Local override"
            })
            .to_string(),
        )
        .expect("local workflow");
        fs::write(
            plugin.join("demo_open.json"),
            json!({
                "name": "demo.open",
                "version": "plugin",
                "description": "Plugin copy"
            })
            .to_string(),
        )
        .expect("plugin workflow");
        env::set_var("RZN_IOS_WORKFLOW_DIRS", &local);
        env::set_var("RZN_PLUGIN_DIR", root.join("plugin"));
        env::remove_var("CLAUDE_PLUGIN_ROOT");

        let workflows = super::list_workflows(Some("demo"), None);
        let workflow = workflows
            .iter()
            .find(|workflow| workflow.id == "demo/open")
            .expect("workflow");
        assert_eq!(workflow.version, "local");

        restore_env("RZN_IOS_WORKFLOW_DIRS", old_extra);
        restore_env("RZN_PLUGIN_DIR", old_plugin);
        restore_env("CLAUDE_PLUGIN_ROOT", old_plugin_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_guardrails_workflow_diagnostics_surface_invalid_files() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_extra = env::var_os("RZN_IOS_WORKFLOW_DIRS");
        let old_plugin = env::var_os("RZN_PLUGIN_DIR");
        let old_plugin_root = env::var_os("CLAUDE_PLUGIN_ROOT");
        let root = env::temp_dir().join(format!(
            "rzn-phone-workflow-diagnostics-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("diagnostics dir");
        let bad_path = root.join("bad.json");
        fs::write(&bad_path, "{not json").expect("bad workflow");
        env::set_var("RZN_IOS_WORKFLOW_DIRS", &root);
        env::remove_var("RZN_PLUGIN_DIR");
        env::remove_var("CLAUDE_PLUGIN_ROOT");

        let diagnostics = super::list_workflow_diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.path
            == bad_path.display().to_string()
            && diagnostic.reason.contains("parse failed")));

        restore_env("RZN_IOS_WORKFLOW_DIRS", old_extra);
        restore_env("RZN_PLUGIN_DIR", old_plugin);
        restore_env("CLAUDE_PLUGIN_ROOT", old_plugin_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workflow_catalog_cache_invalidates_when_directory_contents_change() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_extra = env::var_os("RZN_IOS_WORKFLOW_DIRS");
        let old_plugin = env::var_os("RZN_PLUGIN_DIR");
        let old_plugin_root = env::var_os("CLAUDE_PLUGIN_ROOT");
        let root = env::temp_dir().join(format!(
            "rzn-phone-workflow-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("workflow dir");
        env::set_var("RZN_IOS_WORKFLOW_DIRS", &root);
        env::remove_var("RZN_PLUGIN_DIR");
        env::remove_var("CLAUDE_PLUGIN_ROOT");

        fs::write(
            root.join("demo_first.json"),
            json!({
                "name": "demo.first",
                "version": "1.0.0",
                "description": "First"
            })
            .to_string(),
        )
        .expect("first workflow");
        let first = super::list_workflows(Some("demo"), None);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "demo/first");

        fs::write(
            root.join("demo_second.json"),
            json!({
                "name": "demo.second",
                "version": "1.0.0",
                "description": "Second"
            })
            .to_string(),
        )
        .expect("second workflow");
        let second = super::list_workflows(Some("demo"), None);
        let ids = second
            .iter()
            .map(|workflow| workflow.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["demo/first", "demo/second"]);
        assert!(super::load_file_workflow("demo/second").is_some());

        restore_env("RZN_IOS_WORKFLOW_DIRS", old_extra);
        restore_env("RZN_PLUGIN_DIR", old_plugin);
        restore_env("CLAUDE_PLUGIN_ROOT", old_plugin_root);
        let _ = fs::remove_dir_all(root);
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}

fn workflow_info_from_definition(def: FileWorkflowDefinition) -> WorkflowInfo {
    let mut inputs = def.inputs;
    if let Some(help) = def.help.as_ref() {
        merge_help_parameters(&mut inputs, help);
    }
    let required_variables = normalized_required_variables(&def.required_variables, &inputs);
    for name in &required_variables {
        if let Some(input) = inputs.get_mut(name) {
            input.required = true;
        }
    }
    let reference = WorkflowReference::parse(&def.name).unwrap_or_else(|| WorkflowReference {
        system: "misc".to_string(),
        workflow: def.name.clone(),
    });

    WorkflowInfo {
        id: reference.canonical_id(),
        system: reference.system,
        workflow: reference.workflow,
        name: def.name,
        version: def.version,
        description: if def.description.trim().is_empty() {
            "Workflow loaded from JSON pack.".to_string()
        } else {
            def.description
        },
        required_variables,
        inputs,
        capability: def.capability,
        notes: def.notes,
        help: def.help,
    }
}

fn normalized_required_variables(
    declared: &[String],
    inputs: &HashMap<String, WorkflowInputDefinition>,
) -> Vec<String> {
    let mut required = if declared.is_empty() {
        inputs
            .iter()
            .filter_map(|(name, input)| input.required.then_some(name.clone()))
            .collect::<Vec<_>>()
    } else {
        declared
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty() && inputs.contains_key(name))
            .collect::<Vec<_>>()
    };
    required.sort();
    required.dedup();
    required
}

fn merge_help_parameters(
    inputs: &mut HashMap<String, WorkflowInputDefinition>,
    help: &WorkflowHelpDefinition,
) {
    for (name, metadata) in &help.parameters {
        let Some(input) = inputs.get_mut(name) else {
            continue;
        };
        if input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            input.description = metadata.description.clone();
        }
        if input.example.is_none() {
            input.example = metadata.example.clone();
        }
        if input
            .group
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            input.group = metadata.group.clone();
        }
    }
}

fn workflow_matches(candidate: &str, want: &str) -> bool {
    if candidate.trim() == want.trim() {
        return true;
    }

    match (
        WorkflowReference::parse(candidate),
        WorkflowReference::parse(want),
    ) {
        (Some(candidate_ref), Some(want_ref)) => {
            candidate_ref.canonical_id() == want_ref.canonical_id()
        }
        _ => false,
    }
}

fn list_file_workflows(
    system_filter: Option<&str>,
    family_filter: Option<&str>,
) -> (Vec<WorkflowInfo>, Vec<WorkflowLoadDiagnostic>) {
    let catalog = cached_file_workflow_catalog();
    filter_file_workflow_catalog(&catalog, system_filter, family_filter)
}

fn cached_file_workflow_catalog() -> WorkflowCatalog {
    let dirs = workflow_search_dirs();
    let signature = workflow_catalog_signature(&dirs);
    {
        let guard = WORKFLOW_CATALOG_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cache) = guard.as_ref() {
            if cache.signature == signature {
                return cache.catalog.clone();
            }
        }
    }

    let catalog = load_file_workflow_catalog(&dirs);
    let mut guard = WORKFLOW_CATALOG_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(WorkflowCatalogCache {
        signature,
        catalog: catalog.clone(),
    });
    catalog
}

fn filter_file_workflow_catalog(
    catalog: &WorkflowCatalog,
    system_filter: Option<&str>,
    family_filter: Option<&str>,
) -> (Vec<WorkflowInfo>, Vec<WorkflowLoadDiagnostic>) {
    let mut out = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    let system_filter = system_filter
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let family_filter = family_filter
        .map(str::trim)
        .filter(|value| !value.is_empty());

    for entry in &catalog.entries {
        let info = &entry.info;
        if let Some(filter) = system_filter {
            if info.system != filter {
                continue;
            }
        }
        if let Some(filter) = family_filter {
            let Some(capability) = info.capability.as_ref() else {
                continue;
            };
            if !capability.family.eq_ignore_ascii_case(filter) {
                continue;
            }
        }
        if seen.contains_key(&info.id) {
            continue;
        }
        seen.insert(info.id.clone(), ());
        out.push(info.clone());
    }

    (out, catalog.diagnostics.clone())
}

fn load_file_workflow_catalog(dirs: &[PathBuf]) -> WorkflowCatalog {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for dir in dirs {
        for path in workflow_json_paths(dir) {
            let raw = match fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(err) => {
                    diagnostics.push(WorkflowLoadDiagnostic {
                        path: path.display().to_string(),
                        reason: format!("read failed: {err}"),
                    });
                    continue;
                }
            };
            let def = match serde_json::from_str::<FileWorkflowDefinition>(&raw) {
                Ok(def) => def,
                Err(err) => {
                    diagnostics.push(WorkflowLoadDiagnostic {
                        path: path.display().to_string(),
                        reason: format!("parse failed: {err}"),
                    });
                    continue;
                }
            };
            if def.name.trim().is_empty() {
                diagnostics.push(WorkflowLoadDiagnostic {
                    path: path.display().to_string(),
                    reason: "missing workflow name".to_string(),
                });
                continue;
            }
            let info = workflow_info_from_definition(def.clone());
            entries.push(WorkflowCatalogEntry { def, info });
        }
    }

    WorkflowCatalog {
        entries,
        diagnostics,
    }
}

fn workflow_catalog_signature(dirs: &[PathBuf]) -> WorkflowCatalogSignature {
    dirs.iter()
        .map(|dir| WorkflowDirSignature {
            path: dir.clone(),
            files: workflow_json_paths(dir)
                .into_iter()
                .filter_map(|path| {
                    let metadata = fs::metadata(&path).ok()?;
                    Some(WorkflowFileSignature {
                        path,
                        len: metadata.len(),
                        modified: metadata.modified().ok(),
                    })
                })
                .collect(),
        })
        .collect()
}

fn workflow_json_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn workflow_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(extra) = env::var("RZN_IOS_WORKFLOW_DIRS") {
        for raw in extra.split(':') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            dirs.push(PathBuf::from(trimmed));
        }
    }

    if let Ok(plugin_dir) = env::var("RZN_PLUGIN_DIR") {
        let root = PathBuf::from(plugin_dir);
        dirs.push(root.join("resources").join("workflows"));
    }
    if let Ok(plugin_root) = env::var("CLAUDE_PLUGIN_ROOT") {
        let root = PathBuf::from(plugin_root);
        dirs.push(root.join("resources").join("workflows"));
    }

    // Dev fallback (repo root as cwd in claude_plugin/.mcp.json).
    dirs.push(PathBuf::from("crates/rzn_phone_worker/resources/workflows"));

    dirs.into_iter()
        .filter(|dir| dir.exists() && dir.is_dir())
        .fold(Vec::new(), |mut dedup, entry| {
            if !dedup.contains(&entry) {
                dedup.push(entry);
            }
            dedup
        })
}
