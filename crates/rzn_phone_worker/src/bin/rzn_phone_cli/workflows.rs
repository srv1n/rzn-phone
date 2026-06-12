enum RunTarget {
    Workflow(String),
    SystemNamespace(String),
}

fn workflow_payload(family_filter: Option<&str>) -> Value {
    let workflows = workflows::list_workflows(None, family_filter);
    let systems = workflows::group_workflows_by_system(&workflows);
    serde_json::to_value(json!({
        "systemCount": systems.len(),
        "workflowCount": workflows.len(),
        "systems": systems,
        "workflows": workflows,
    }))
    .unwrap_or_else(|_| json!({}))
}

fn read_json_input(raw: &str) -> Result<Value> {
    let text = if raw.is_empty() {
        "{}".to_string()
    } else if let Some(path) = raw.strip_prefix('@') {
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path))?
    } else {
        raw.to_string()
    };
    serde_json::from_str(&text).with_context(|| "invalid JSON input".to_string())
}

fn canonicalize_workflow_ref(value: &str) -> String {
    let raw = value.trim().replace('\\', "/");
    if let Some((system, workflow)) = raw.split_once('/') {
        return format!(
            "{}/{}",
            system.trim_matches(['/', '.']).trim(),
            workflow.trim_matches(['/', '.']).trim()
        );
    }
    if let Some((system, workflow)) = raw.split_once('.') {
        return format!(
            "{}/{}",
            system.trim_matches(['/', '.']).trim(),
            workflow.trim_matches(['/', '.']).trim()
        );
    }
    raw.trim_matches(['/', '.']).to_string()
}

fn normalize_workflow_ref(first: &str, second: Option<&str>) -> Result<String> {
    let joined = if let Some(second) = second {
        format!("{}/{}", first, second)
    } else {
        first.to_string()
    };
    let normalized = canonicalize_workflow_ref(&joined);
    if normalized.contains('/') {
        Ok(normalized)
    } else {
        bail!(
            "workflow ref '{}' is missing a system/workflow shape",
            joined
        )
    }
}

fn resolve_run_target(first: &str, second: Option<&str>) -> Result<RunTarget> {
    if second.is_some() {
        return Ok(RunTarget::Workflow(normalize_workflow_ref(first, second)?));
    }
    if first.contains('/') || first.contains('.') {
        return Ok(RunTarget::Workflow(normalize_workflow_ref(first, None)?));
    }

    let requested = normalize_text(first);
    if requested.is_empty() {
        bail!("workflow ref is empty");
    }

    let systems = workflows::group_workflows_by_system(&workflows::list_workflows(None, None));
    if let Some(system) = systems
        .iter()
        .find(|system| system.id.eq_ignore_ascii_case(&requested))
    {
        return Ok(RunTarget::SystemNamespace(system.id.clone()));
    }

    let suggestions = closest_matches(&requested, systems.into_iter().map(|system| system.id));
    if suggestions.is_empty() {
        bail!(
            "workflow ref '{}' is missing a system/workflow shape",
            first
        );
    }
    bail!(
        "workflow ref '{}' is missing a system/workflow shape\nDid you mean one of these systems?\n  {}",
        first,
        suggestions.join("\n  ")
    );
}

fn find_workflow(reference: &str) -> Result<workflows::WorkflowInfo> {
    let wanted = canonicalize_workflow_ref(reference);
    let all = workflows::list_workflows(None, None);
    all.iter()
        .find(|item| item.id == wanted || canonicalize_workflow_ref(&item.name) == wanted)
        .cloned()
        .ok_or_else(|| unknown_workflow_error(reference, &wanted, &all))
}

fn unknown_workflow_error(
    reference: &str,
    normalized: &str,
    workflows: &[workflows::WorkflowInfo],
) -> anyhow::Error {
    let suggestions = closest_matches(
        normalized,
        workflows
            .iter()
            .flat_map(|item| [item.id.clone(), canonicalize_workflow_ref(&item.name)]),
    );
    if suggestions.is_empty() {
        anyhow!("unknown workflow '{}'", reference)
    } else {
        anyhow!(
            "unknown workflow '{}'\nDid you mean:\n  {}",
            reference,
            suggestions.join("\n  ")
        )
    }
}
fn closest_matches<I>(query: &str, candidates: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut scored = candidates
        .into_iter()
        .filter_map(|candidate| {
            let lower = candidate.to_ascii_lowercase();
            let score = if lower == query {
                1.0
            } else if lower.starts_with(&query) {
                0.98
            } else if lower.contains(&query) {
                0.92
            } else {
                jaro_winkler(&lower, &query)
            };
            if score < 0.84 {
                return None;
            }
            Some((score, candidate))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.dedup_by(|a, b| a.1 == b.1);
    scored
        .into_iter()
        .take(3)
        .map(|(_, candidate)| candidate)
        .collect()
}

fn filtered_workflow_payload(payload: &Value, args: &ListArgs) -> Result<Value> {
    let source_workflows = payload
        .get("workflows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("workflow payload is missing workflows[]"))?;
    let system_query = args.system_or_query.clone().unwrap_or_default();
    let (system_filter, search_filter, system_note) =
        resolve_system_filter(payload, &system_query, args.search.clone());
    let favorites = load_favorites()?.into_iter().collect::<HashSet<_>>();
    let mut workflows = Vec::new();

    for item in source_workflows {
        let Some(workflow) = item.as_object() else {
            continue;
        };
        if let Some(system) = system_filter.as_deref() {
            if workflow
                .get("system")
                .and_then(Value::as_str)
                .unwrap_or_default()
                != system
            {
                continue;
            }
        }
        let capability = workflow
            .get("capability")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(surface) = args.surface.as_deref() {
            let capability_surface = capability
                .get("surface")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !capability_surface.eq_ignore_ascii_case(surface) {
                continue;
            }
        }
        if let Some(mutating) = args.mutating {
            if capability
                .get("mutating")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                != mutating
            {
                continue;
            }
        }
        if args.favorites
            && !favorites.contains(
                workflow
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        {
            continue;
        }
        if let Some(has_input) = args.has_input.as_deref() {
            if workflow
                .get("inputs")
                .and_then(Value::as_object)
                .map(|inputs| inputs.contains_key(has_input))
                != Some(true)
            {
                continue;
            }
        }
        if let Some(search) = search_filter.as_deref() {
            let haystack = workflow_search_haystack(item);
            if !haystack.contains(&search.to_ascii_lowercase()) {
                continue;
            }
        }
        workflows.push(item.clone());
    }

    workflows.sort_by_key(|item| {
        item.get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    let mut systems_map = BTreeMap::<String, Vec<Value>>::new();
    for workflow in &workflows {
        let system = workflow
            .get("system")
            .and_then(Value::as_str)
            .unwrap_or("misc")
            .to_string();
        systems_map
            .entry(system)
            .or_default()
            .push(workflow.clone());
    }
    let systems = systems_map
        .into_iter()
        .map(|(id, workflows)| json!({"id": id, "workflow_count": workflows.len(), "workflows": workflows}))
        .collect::<Vec<_>>();

    Ok(json!({
        "systemCount": systems.len(),
        "workflowCount": workflows.len(),
        "systems": systems,
        "workflows": workflows,
        "_resolvedSystem": system_filter,
        "_resolvedSearch": search_filter,
        "_systemNote": system_note
    }))
}

fn resolve_system_filter(
    payload: &Value,
    raw_system: &str,
    search: Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    let system = normalize_text(raw_system);
    let search_text = search
        .map(|value| normalize_text(&value))
        .filter(|value| !value.is_empty());
    if system.is_empty() {
        return (None, search_text, None);
    }

    let systems = payload
        .get("systems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in systems {
        if item
            .get("id")
            .and_then(Value::as_str)
            .map(|value| value.eq_ignore_ascii_case(&system))
            .unwrap_or(false)
        {
            return (
                item.get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                search_text,
                None,
            );
        }
    }

    if let Some(search_text) = search_text {
        (
            None,
            Some(format!("{} {}", system, search_text)),
            Some(format!("Positional query fallback: {}", system)),
        )
    } else {
        (
            None,
            Some(system.clone()),
            Some(format!("Positional query fallback: {}", system)),
        )
    }
}

fn workflow_search_haystack(workflow: &Value) -> String {
    let capability = workflow
        .get("capability")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let inputs = workflow.get("inputs").cloned().unwrap_or_else(|| json!({}));
    let help = workflow.get("help").cloned().unwrap_or_else(|| json!({}));
    let mut values = Vec::new();
    for candidate in [
        workflow.get("id"),
        workflow.get("name"),
        workflow.get("system"),
        workflow.get("workflow"),
        workflow.get("description"),
        capability.get("family"),
        capability.get("intent"),
        capability.get("surface"),
        help.get("when_to_use"),
        help.get("returns"),
    ] {
        if let Some(text) = candidate.and_then(Value::as_str) {
            values.push(text.to_ascii_lowercase());
        }
    }
    if let Some(obj) = inputs.as_object() {
        values.push(
            obj.keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase(),
        );
        for spec in obj.values() {
            if let Some(description) = spec.get("description").and_then(Value::as_str) {
                values.push(description.to_ascii_lowercase());
            }
        }
    }
    values.join("\n")
}

fn normalize_text(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn workflow_short_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

fn workflow_title_suffix(workflow: &Value) -> String {
    let reference = workflow.get("id").and_then(Value::as_str).unwrap_or("?");
    let short = workflow_short_name(reference);
    let label = workflow
        .get("workflow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    if label.is_empty() || canonicalize_workflow_ref(label) == canonicalize_workflow_ref(short) {
        String::new()
    } else {
        format!(" - {}", label)
    }
}

fn print_workflow_show(workflow: &Value, expanded_examples: bool) -> Result<()> {
    print!(
        "{}",
        render_workflow_help(workflow, expanded_examples, &[])?
    );
    Ok(())
}

fn render_workflow_help(
    workflow: &Value,
    expanded_examples: bool,
    missing_required: &[String],
) -> Result<String> {
    let reference = workflow.get("id").and_then(Value::as_str).unwrap_or("?");
    let suffix = workflow_title_suffix(workflow);
    let mut lines = vec![render_primary_heading(reference, suffix)];
    if !missing_required.is_empty() {
        lines.push(String::new());
        lines.push(render_section_heading("Missing Required Params"));
        lines.push(format!("  {}", missing_required.join(", ")));
        lines.push(
            "This workflow is one step away from success. Add the missing params and rerun."
                .to_string(),
        );
        lines.push(String::new());
    }

    let capability = workflow
        .get("capability")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let rows = [
        (
            "ID",
            workflow
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        (
            "Version",
            workflow
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        (
            "Family",
            capability
                .get("family")
                .and_then(Value::as_str)
                .unwrap_or("other")
                .to_string(),
        ),
        (
            "Intent",
            capability
                .get("intent")
                .and_then(Value::as_str)
                .unwrap_or("n/a")
                .to_string(),
        ),
        (
            "Surface",
            capability
                .get("surface")
                .and_then(Value::as_str)
                .unwrap_or("n/a")
                .to_string(),
        ),
        (
            "Mode",
            if capability
                .get("mutating")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "write".to_string()
            } else {
                "read".to_string()
            },
        ),
    ];
    lines.push(render_meta_line(
        &rows
            .iter()
            .map(|(key, value)| format!("{}: {}", key.to_ascii_lowercase(), value))
            .collect::<Vec<_>>()
            .join(" | "),
    ));
    lines.push(String::new());
    lines.push(normalize_text(
        workflow
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ));

    if let Some(use_it_when) = workflow
        .get("help")
        .and_then(|help| help.get("when_to_use"))
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())
    {
        lines.push(String::new());
        lines.push(render_section_heading("Use It When"));
        lines.push(format!("  {}", use_it_when));
    }

    let examples = workflow_examples(workflow, expanded_examples);
    if let Some(quick) = examples.first() {
        lines.push(String::new());
        lines.push(render_section_heading("Quick Start"));
        if let Some(description) = quick.get("description").and_then(Value::as_str) {
            if !description.trim().is_empty() {
                lines.push(format!("  {}", description.trim()));
            }
        }
        lines.push(format!(
            "  {}",
            example_command(
                workflow,
                quick.get("args").cloned().unwrap_or_else(|| json!({}))
            )?
        ));
    }

    let required_params = required_param_names(workflow);
    if !required_params.is_empty() {
        lines.push(String::new());
        lines.push(render_section_heading("Required Params"));
        let rows = required_params
            .iter()
            .map(|name| vec![styled_body_cell(name), styled_status_cell("required")])
            .collect::<Vec<_>>();
        lines.push(render_cli_table(
            vec![styled_header_cell("parameter"), styled_header_cell("input")],
            rows,
            2,
        ));
    }

    let optional_params = optional_param_names(workflow);
    if !optional_params.is_empty() {
        lines.push(String::new());
        lines.push(render_section_heading("Optional Params"));
        let rows = optional_params
            .iter()
            .map(|name| vec![styled_body_cell(name), styled_status_cell("optional")])
            .collect::<Vec<_>>();
        lines.push(render_cli_table(
            vec![styled_header_cell("parameter"), styled_header_cell("input")],
            rows,
            2,
        ));
    }

    for group in INPUT_GROUP_ORDER {
        let items = workflow_input_groups(workflow)
            .get(group)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        lines.push(String::new());
        lines.push(
            match group {
                "core" => "Core Inputs",
                "safety" => "Safety Gates",
                "advanced" => "Advanced Inputs",
                "internal" => "Internal Knobs",
                _ => "Inputs",
            }
            .to_string(),
        );
        let rows = items
            .into_iter()
            .map(|(name, spec)| {
                vec![
                    styled_body_cell(&name),
                    styled_body_cell(build_input_traits(&spec)),
                    styled_body_cell(
                        input_description(&name, &spec, workflow)
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                    styled_body_cell(
                        input_example(&name, &spec)
                            .map(|value| {
                                serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
                            })
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                ]
            })
            .collect::<Vec<_>>();
        lines.push(render_cli_table(
            vec![
                styled_header_cell("parameter"),
                styled_header_cell("traits"),
                styled_header_cell("description"),
                styled_header_cell("example"),
            ],
            rows,
            2,
        ));
    }

    if let Some(returns) = workflow
        .get("help")
        .and_then(|help| help.get("returns"))
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())
    {
        lines.push(String::new());
        lines.push(render_section_heading("Returns"));
        lines.push(format!("  {}", returns));
    }

    let notes = workflow
        .get("notes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(normalize_text))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if !notes.is_empty() {
        lines.push(String::new());
        lines.push(render_section_heading("Notes"));
        for note in notes {
            lines.push(format!("  - {}", note));
        }
    }

    if expanded_examples && examples.len() > 1 {
        lines.push(String::new());
        lines.push(render_section_heading("Examples"));
        let rows = examples
            .into_iter()
            .skip(1)
            .map(|example| {
                let purpose = example
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| example.get("label").and_then(Value::as_str))
                    .unwrap_or("Example");
                let command = example_command(
                    workflow,
                    example.get("args").cloned().unwrap_or_else(|| json!({})),
                )
                .unwrap_or_else(|_| "rzn-phone run <workflow> --args-json '{...}'".to_string());
                vec![styled_body_cell(purpose), styled_body_cell(command)]
            })
            .collect::<Vec<_>>();
        lines.push(render_cli_table(
            vec![styled_header_cell("purpose"), styled_header_cell("command")],
            rows,
            2,
        ));
    }

    Ok(lines.join("\n") + "\n")
}

fn build_input_traits(spec: &Value) -> String {
    let mut parts = vec![spec
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string")
        .to_string()];
    if spec
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        parts.push("required".to_string());
    }
    if let Some(default) = spec.get("default") {
        if !default.is_null() {
            parts.push(format!(
                "default={}",
                serde_json::to_string(default).unwrap_or_else(|_| "null".to_string())
            ));
        }
    }
    parts.join("  ")
}

fn input_description(name: &str, spec: &Value, workflow: &Value) -> Option<String> {
    let description = spec
        .get("description")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty());
    let structure = workflow
        .get("help")
        .and_then(|help| help.get("parameters"))
        .and_then(|parameters| parameters.get(name))
        .and_then(|parameter| parameter.get("structure"))
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty());
    if let Some(description) = description {
        if let Some(structure) = structure {
            return Some(format!("{} Shape: {}", description, structure));
        }
        return Some(description);
    }
    if let Some(structure) = structure {
        return Some(format!("Shape: {}", structure));
    }
    let lower = name.to_ascii_lowercase();
    if lower == "query" || lower == "search_query" {
        return Some("Search text to submit.".to_string());
    }
    if [
        "post_text",
        "comment_text",
        "reply_text",
        "message_text",
        "updated_text",
    ]
    .contains(&lower.as_str())
    {
        return Some("Text payload to draft.".to_string());
    }
    if lower == "target_app_name" {
        return Some("Exact app name to rank or locate.".to_string());
    }
    if ["result_index", "post_index", "thread_index", "reply_index"].contains(&lower.as_str()) {
        return Some("Zero-based target index.".to_string());
    }
    if lower == "limit" {
        return Some("Maximum number of items to return.".to_string());
    }
    if lower == "country" || lower == "locale" {
        return Some("Optional storefront or locale override.".to_string());
    }
    if lower.starts_with("execute_") {
        let mut suffix = String::new();
        if workflow
            .get("capability")
            .and_then(|cap| cap.get("mutating"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            suffix = " and still requires --commit 1".to_string();
        }
        return Some(format!(
            "Set true to actually {}{}.",
            lower.trim_start_matches("execute_").replace('_', " "),
            suffix
        ));
    }
    if lower == "submit" {
        return Some("Set true to actually submit the draft.".to_string());
    }
    None
}

fn required_param_names(workflow: &Value) -> Vec<String> {
    let mut names = workflow
        .get("required_variables")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            workflow
                .get("inputs")
                .and_then(Value::as_object)
                .map(|inputs| {
                    inputs
                        .iter()
                        .filter_map(|(name, spec)| {
                            spec.get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                                .then_some(name.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });
    names.sort();
    names.dedup();
    names
}

fn optional_param_names(workflow: &Value) -> Vec<String> {
    let required = required_param_names(workflow)
        .into_iter()
        .collect::<HashSet<_>>();
    let mut names = workflow
        .get("inputs")
        .and_then(Value::as_object)
        .map(|inputs| {
            inputs
                .keys()
                .filter(|name| !required.contains(*name))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn missing_required_params(workflow: &Value, args: &Value) -> Vec<String> {
    let Some(args_obj) = args.as_object() else {
        return required_param_names(workflow);
    };
    let mut missing = required_param_names(workflow)
        .into_iter()
        .filter(|name| {
            args_obj.get(name).is_none() || args_obj.get(name).is_some_and(Value::is_null)
        })
        .collect::<Vec<_>>();
    missing.sort();
    missing
}

fn input_example(name: &str, spec: &Value) -> Option<Value> {
    if let Some(example) = spec.get("example") {
        return Some(example.clone());
    }
    let example = match name {
        "query" => json!("best headphones 2026"),
        "search_query" => json!("voice notes"),
        "target_app_name" => json!("Voicenotes AI Notes & Meetings"),
        "message_text" => json!("Quick check-in on the launch plan."),
        "comment_text" => json!("Useful breakdown. The offline sync detail is the real win."),
        "reply_text" => {
            json!("That constraint makes sense. What does the fallback path look like?")
        }
        "post_text" => json!("Shipping the workflow help overhaul this week."),
        "updated_text" => json!("Updated draft with clearer onboarding copy."),
        "username" => json!("openai"),
        "country" => json!("us"),
        "locale" => json!("en_US"),
        "submit_mode" => json!("suggestion"),
        "typing_mode" => json!("full"),
        "limit" => json!(5),
        _ => match spec.get("type").and_then(Value::as_str).unwrap_or("string") {
            "integer" | "number" => json!(1),
            "boolean" => json!(false),
            "array" => json!([format!("<{}_item>", name)]),
            "object" => json!({}),
            _ => json!(format!("<{}>", name)),
        },
    };
    Some(example)
}

fn workflow_examples(workflow: &Value, expanded: bool) -> Vec<Value> {
    let examples = workflow
        .get("help")
        .and_then(|help| help.get("examples"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| item.is_object())
        .collect::<Vec<_>>();
    if !examples.is_empty() {
        return if expanded {
            examples
        } else {
            vec![examples[0].clone()]
        };
    }
    let quick_args = build_example_args(workflow, false, false);
    let mut fallback = vec![json!({
        "label": "Quick Start",
        "args": quick_args
    })];
    if expanded
        && workflow
            .get("capability")
            .and_then(|cap| cap.get("mutating"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        let mut live = build_example_args(workflow, false, true);
        if let Some(obj) = live.as_object_mut() {
            for (key, value) in obj.iter_mut() {
                if key.starts_with("execute_") || key == "submit" {
                    *value = json!(true);
                }
            }
        }
        fallback.push(json!({
            "label": "Live Run",
            "description": "Same workflow, but with the safety gate enabled.",
            "args": live
        }));
    }
    fallback
}

fn build_example_args(workflow: &Value, include_advanced: bool, include_safety: bool) -> Value {
    let grouped = workflow_input_groups(workflow);
    let mut args = Map::new();
    for group in INPUT_GROUP_ORDER {
        if group == "advanced" && !include_advanced {
            continue;
        }
        if group == "safety" && !include_safety {
            continue;
        }
        if group == "internal" {
            continue;
        }
        for (name, spec) in grouped.get(group).cloned().unwrap_or_default() {
            let default = spec.get("default");
            if group != "safety"
                && default.is_some()
                && default != Some(&Value::Null)
                && !spec
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                continue;
            }
            if let Some(example) = input_example(&name, &spec) {
                args.insert(name, example);
            }
        }
    }
    Value::Object(args)
}

fn example_command(workflow: &Value, args: Value) -> Result<String> {
    let json_args = serde_json::to_string(&args)?;
    let mut command = format!(
        "rzn-phone run {} --udid <udid> --args-json '{}'",
        workflow.get("id").and_then(Value::as_str).unwrap_or("?"),
        json_args
    );
    let mutating = workflow
        .get("capability")
        .and_then(|cap| cap.get("mutating"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if mutating {
        let live = args
            .as_object()
            .map(|obj| {
                obj.iter().any(|(key, value)| {
                    (key.starts_with("execute_") || key == "submit")
                        && value.as_bool().unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if live {
            command.push_str(" --commit 1");
        } else {
            command.push_str(" --dry-run");
        }
    }
    Ok(command)
}

fn render_system_run_help(
    system_id: &str,
    workflows: &[workflows::WorkflowInfo],
) -> Result<String> {
    let mut lines = vec![
        render_primary_heading(&format!("System '{}'", system_id), " is a namespace"),
        "This is a workflow namespace, not a runnable workflow.".to_string(),
        String::new(),
        render_section_heading("Workflows"),
    ];
    let rows = workflows
        .iter()
        .map(|workflow| {
            let value = serde_json::to_value(workflow).unwrap_or_else(|_| json!({}));
            vec![
                styled_body_cell(workflow_short_name(&workflow.id)),
                styled_body_cell(workflow_contract_preview(&value)),
                styled_body_cell(normalize_text(&workflow.description)),
            ]
        })
        .collect::<Vec<_>>();
    lines.push(render_cli_table(
        vec![
            styled_header_cell("workflow"),
            styled_header_cell("inputs"),
            styled_header_cell("description"),
        ],
        rows,
        2,
    ));
    lines.push(String::new());
    lines.push(render_section_heading("Next"));
    lines.push(format!("  rzn-phone list {}", system_id));
    if let Some(first) = workflows.first() {
        let workflow_value = serde_json::to_value(first)?;
        lines.push(format!("  rzn-phone show {}", first.id));
        if let Some(example) = workflow_examples(&workflow_value, false).first() {
            lines.push(format!(
                "  {}",
                example_command(
                    &workflow_value,
                    example.get("args").cloned().unwrap_or_else(|| json!({}))
                )?
            ));
        } else {
            lines.push(format!(
                "  rzn-phone run {}/<workflow> --udid <udid> --args-json '{{...}}'",
                system_id
            ));
        }
    }
    Ok(lines.join("\n") + "\n")
}

fn exit_with_help_output(pretty: &str, structured: &Value, wants_json: bool) -> ! {
    if wants_json {
        println!(
            "{}",
            serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string())
        );
    } else {
        eprint!("{}", pretty);
    }
    std::process::exit(2);
}
