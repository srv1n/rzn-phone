fn want_pretty(explicit_pretty: bool) -> bool {
    explicit_pretty || (io::stdout().is_terminal() && !plain_mode_forced())
}

fn plain_mode_forced() -> bool {
    env::var("RZN_CLI_PLAIN")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn rich_mode_forced() -> bool {
    env::var("RZN_CLI_FORCE_RICH")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn prepare_pretty_output(explicit_pretty: bool) {
    if explicit_pretty && io::stdout().is_terminal() && !plain_mode_forced() {
        env::set_var("RZN_CLI_FORCE_RICH", "1");
    }
}

fn cli_stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

fn cli_rich_output_enabled() -> bool {
    if plain_mode_forced() || !cli_stdout_is_tty() {
        return false;
    }

    if rich_mode_forced() {
        return true;
    }

    env::var("TERM")
        .map(|term| term.trim() != "dumb")
        .unwrap_or(true)
}

fn cli_table_width(indent: usize) -> u16 {
    let width = terminal_size()
        .map(|(Width(width), _)| width as usize)
        .filter(|width| *width >= 60)
        .unwrap_or(100);
    width
        .saturating_sub(indent)
        .clamp(60, 140)
        .min(u16::MAX as usize) as u16
}

fn render_primary_heading(prefix: &str, suffix: impl AsRef<str>) -> String {
    let suffix = suffix.as_ref();
    if cli_rich_output_enabled() {
        format!(
            "{}{}",
            prefix.bold().bright_white(),
            suffix.bold().bright_white()
        )
    } else {
        format!("{}{}", prefix, suffix)
    }
}

fn render_section_heading(title: &str) -> String {
    if cli_rich_output_enabled() {
        title.bold().bright_cyan().to_string()
    } else {
        title.to_string()
    }
}

fn render_meta_line(text: &str) -> String {
    if cli_rich_output_enabled() {
        text.dimmed().to_string()
    } else {
        text.to_string()
    }
}

fn indent_block(block: &str, indent: usize) -> String {
    let prefix = " ".repeat(indent);
    block
        .lines()
        .map(|line| format!("{}{}", prefix, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn styled_header_cell(text: &str) -> Cell {
    let cell = Cell::new(text);
    if cli_rich_output_enabled() {
        cell.fg(Color::Cyan).add_attribute(Attribute::Bold)
    } else {
        cell
    }
}

fn styled_body_cell(text: impl AsRef<str>) -> Cell {
    Cell::new(text.as_ref())
}

fn styled_status_cell(text: &str) -> Cell {
    let cell = Cell::new(text);
    if !cli_rich_output_enabled() {
        return cell;
    }

    match text {
        "required" => cell.fg(Color::Yellow).add_attribute(Attribute::Bold),
        "optional" => cell.fg(Color::Green),
        "write" => cell.fg(Color::Red).add_attribute(Attribute::Bold),
        "read" => cell.fg(Color::Green),
        "available" => cell.fg(Color::Green).add_attribute(Attribute::Bold),
        "offline" => cell.fg(Color::Yellow),
        "live" => cell.fg(Color::Red).add_attribute(Attribute::Bold),
        "dry-run" => cell.fg(Color::Green),
        other => Cell::new(other),
    }
}

fn render_cli_table(headers: Vec<Cell>, rows: Vec<Vec<Cell>>, indent: usize) -> String {
    let mut table = Table::new();
    if cli_rich_output_enabled() {
        table
            .load_preset(UTF8_FULL_CONDENSED)
            .apply_modifier(UTF8_ROUND_CORNERS);
    } else {
        table.load_preset(ASCII_FULL).force_no_tty();
    }
    table
        .set_content_arrangement(ContentArrangement::DynamicFullWidth)
        .set_width(cli_table_width(indent))
        .set_header(headers);
    for row in rows {
        table.add_row(row);
    }
    indent_block(&table.to_string(), indent)
}
fn print_workflow_list(payload: &Value, args: &ListArgs) -> Result<()> {
    let filtered = filtered_workflow_payload(payload, args)?;
    let favorites = load_favorites()?.into_iter().collect::<HashSet<_>>();
    let workflow_count = filtered
        .get("workflowCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let system_count = filtered
        .get("systemCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    println!("{}", render_primary_heading("Workflow Catalog", ""));
    let mut meta = vec![
        format!("systems: {}", system_count),
        format!("workflows: {}", workflow_count),
    ];
    if let Some(system) = filtered.get("_resolvedSystem").and_then(Value::as_str) {
        meta.push(format!("system: {}", system));
    }
    if let Some(search) = filtered.get("_resolvedSearch").and_then(Value::as_str) {
        meta.push(format!("search: {}", search));
    }
    if args.favorites {
        meta.push("filter: favorites".to_string());
    }
    println!("{}", render_meta_line(&meta.join(" | ")));
    if let Some(note) = filtered.get("_systemNote").and_then(Value::as_str) {
        println!("{}", render_meta_line(note));
    }
    if workflow_count == 0 {
        println!("No workflows found.");
        return Ok(());
    }

    let detailed = filtered
        .get("_resolvedSearch")
        .and_then(Value::as_str)
        .is_some()
        || filtered
            .get("_resolvedSystem")
            .and_then(Value::as_str)
            .is_some()
        || args.has_input.is_some()
        || args.surface.is_some()
        || args.favorites
        || workflow_count <= 12;

    for system in filtered
        .get("systems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        println!();
        println!(
            "{}",
            render_section_heading(&format!(
                "{} ({})",
                system.get("id").and_then(Value::as_str).unwrap_or("misc"),
                system
                    .get("workflow_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ))
        );
        let workflows = system
            .get("workflows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if args.compact {
            let names = workflows
                .iter()
                .filter_map(|item| item.get("workflow").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ");
            println!("  {}", names);
            continue;
        }
        let rows = workflows
            .iter()
            .map(|workflow| {
                let id = workflow.get("id").and_then(Value::as_str).unwrap_or("?");
                let short = workflow_short_name(id);
                let capability = workflow
                    .get("capability")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let mode = if capability
                    .get("mutating")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "write"
                } else {
                    "read"
                };
                let mut row = vec![
                    styled_body_cell(format!(
                        "{} {}",
                        if favorites.contains(id) { "*" } else { " " },
                        short
                    )),
                    styled_body_cell(
                        capability
                            .get("family")
                            .and_then(Value::as_str)
                            .unwrap_or("other"),
                    ),
                    styled_body_cell(
                        capability
                            .get("surface")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                    ),
                    styled_status_cell(mode),
                ];
                if detailed {
                    row.push(styled_body_cell(workflow_contract_preview(workflow)));
                }
                row.push(styled_body_cell(normalize_text(
                    workflow
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )));
                row
            })
            .collect::<Vec<_>>();
        let mut headers = vec![
            styled_header_cell("workflow"),
            styled_header_cell("family"),
            styled_header_cell("surface"),
            styled_header_cell("mode"),
        ];
        if detailed {
            headers.push(styled_header_cell("inputs"));
        }
        headers.push(styled_header_cell("description"));
        println!("{}", render_cli_table(headers, rows, 2));
    }
    if detailed {
        println!();
        println!("{}", render_section_heading("Next"));
        println!("  Use `rzn-phone show <workflow>` for full input docs and runnable examples.");
    }
    if !favorites.is_empty() {
        println!();
        println!("* favorite");
    }
    Ok(())
}

fn workflow_contract_preview(workflow: &Value) -> String {
    let grouped = workflow_input_groups(workflow);
    let mut parts = Vec::new();
    let mut core = grouped
        .get("core")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, spec)| {
            spec.get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    core.sort_by_key(|name| input_preview_priority(name));
    if !core.is_empty() {
        let preview = core.iter().take(3).cloned().collect::<Vec<_>>();
        let mut label = format!("needs {}", preview.join(", "));
        if core.len() > 3 {
            label.push_str(&format!(" +{}", core.len() - 3));
        }
        parts.push(label);
    }

    let mut advanced = grouped
        .get("advanced")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    advanced.sort_by_key(|name| input_preview_priority(name));
    if !advanced.is_empty() {
        parts.push(format!(
            "opt {}",
            advanced.into_iter().take(2).collect::<Vec<_>>().join(", ")
        ));
    }

    let mut safety = grouped
        .get("safety")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    safety.sort_by_key(|name| input_preview_priority(name));
    if !safety.is_empty() {
        parts.push(format!(
            "gate {} + --commit 1",
            safety.into_iter().take(2).collect::<Vec<_>>().join(", ")
        ));
    }

    if parts.is_empty() {
        "no workflow args".to_string()
    } else {
        parts.join(" | ")
    }
}

fn input_preview_priority(name: &str) -> (u8, String) {
    let lower = name.to_ascii_lowercase();
    let priority = if matches!(
        lower.as_str(),
        "query"
            | "search_query"
            | "message_text"
            | "comment_text"
            | "reply_text"
            | "post_text"
            | "updated_text"
            | "target_app_name"
            | "username"
    ) {
        0
    } else if matches!(
        lower.as_str(),
        "limit"
            | "country"
            | "locale"
            | "submit_mode"
            | "typing_mode"
            | "result_index"
            | "post_index"
            | "thread_index"
            | "reply_index"
    ) {
        1
    } else if lower.starts_with("execute_") || lower == "submit" {
        2
    } else if lower.starts_with("capture") {
        4
    } else {
        3
    };
    (priority, lower)
}

fn workflow_input_groups(workflow: &Value) -> BTreeMap<String, Vec<(String, Value)>> {
    let mut grouped = BTreeMap::<String, Vec<(String, Value)>>::new();
    for group in INPUT_GROUP_ORDER {
        grouped.insert(group.to_string(), Vec::new());
    }
    if let Some(inputs) = workflow.get("inputs").and_then(Value::as_object) {
        let mut names = inputs.keys().cloned().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let spec = inputs.get(&name).cloned().unwrap_or_else(|| json!({}));
            let group = infer_input_group(&name, &spec);
            grouped.entry(group).or_default().push((name, spec));
        }
    }
    grouped
}

fn infer_input_group(name: &str, spec: &Value) -> String {
    if let Some(group) = spec.get("group").and_then(Value::as_str) {
        if INPUT_GROUP_ORDER.contains(&group) {
            return group.to_string();
        }
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("execute_") || lower == "submit" {
        return "safety".to_string();
    }
    if lower == "maxnodes"
        || [
            "predicate",
            "selector",
            "xpath",
            "bundle_id",
            "bundleid",
            "accessibility",
        ]
        .iter()
        .any(|token| lower.contains(token))
    {
        return "internal".to_string();
    }
    if spec
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return "core".to_string();
    }
    if matches!(
        lower.as_str(),
        "limit"
            | "country"
            | "locale"
            | "submit_mode"
            | "typing_mode"
            | "review_sort"
            | "capturescreenshot"
            | "capturepagesource"
    ) || lower.starts_with("max")
        || lower.starts_with("min")
        || lower.starts_with("capture")
    {
        return "advanced".to_string();
    }
    "core".to_string()
}

fn workflow_presentation(payload: &Value) -> Option<String> {
    let cli = payload
        .get("_presentation")
        .and_then(|presentation| presentation.get("cli"))?;
    if cli.get("type").and_then(Value::as_str) != Some("result_list") {
        return None;
    }
    let items = cli.get("items").and_then(Value::as_array)?;
    let title = cli
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Results");
    let title_field = cli
        .get("titleField")
        .and_then(Value::as_str)
        .unwrap_or("title");
    let url_field = cli.get("urlField").and_then(Value::as_str).unwrap_or("url");
    let snippet_field = cli
        .get("snippetField")
        .and_then(Value::as_str)
        .unwrap_or("snippet");
    let mut lines = vec![title.to_string()];
    if items.is_empty() {
        lines.push("No results found.".to_string());
    } else {
        for (idx, item) in items.iter().enumerate() {
            let item_title = item
                .get(title_field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("(untitled)");
            lines.push(format!("{}. {}", idx + 1, item_title));
            if let Some(url) = item
                .get(url_field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push(format!("   {}", url));
            }
            if let Some(snippet) = item
                .get(snippet_field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push(format!("   {}", snippet));
            }
        }
    }
    if let Some(footer) = cli
        .get("footer")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(footer.to_string());
    }
    Some(lines.join("\n") + "\n")
}

fn print_tool_list(tools: &[Value]) -> Result<()> {
    println!("{}", render_primary_heading("Tools", ""));
    println!("{}", render_meta_line(&format!("count: {}", tools.len())));
    if tools.is_empty() {
        println!("No tools found.");
        return Ok(());
    }
    let mut grouped = BTreeMap::<String, Vec<&Value>>::new();
    for tool in tools {
        grouped
            .entry(
                tool.get("capabilityFamily")
                    .and_then(Value::as_str)
                    .unwrap_or("other")
                    .to_string(),
            )
            .or_default()
            .push(tool);
    }
    for (family, tools) in grouped {
        println!();
        println!(
            "{}",
            render_section_heading(&format!("{} ({})", family, tools.len()))
        );
        let rows = tools
            .into_iter()
            .map(|tool| {
                vec![
                    styled_body_cell(tool.get("name").and_then(Value::as_str).unwrap_or("")),
                    styled_body_cell(
                        scalar_value(tool.get("capabilityTier")).unwrap_or_else(|| "?".to_string()),
                    ),
                    styled_body_cell(normalize_text(
                        tool.get("description")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    )),
                ]
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            render_cli_table(
                vec![
                    styled_header_cell("tool"),
                    styled_header_cell("tier"),
                    styled_header_cell("description")
                ],
                rows,
                2
            )
        );
    }
    Ok(())
}

fn print_tool_show(tool: &Value) -> Result<()> {
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    println!("{}", render_primary_heading(&name, ""));
    println!(
        "{}",
        render_meta_line(&format!(
            "family: {} | tier: {}",
            tool.get("capabilityFamily")
                .and_then(Value::as_str)
                .unwrap_or("other"),
            scalar_value(tool.get("capabilityTier")).unwrap_or_else(|| "?".to_string())
        ))
    );
    println!();
    println!(
        "{}",
        normalize_text(
            tool.get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
        )
    );
    println!();
    println!("{}", render_section_heading("Inputs"));
    let props = tool
        .get("inputSchema")
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let required = tool
        .get("inputSchema")
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect::<HashSet<_>>();
    if props.is_empty() {
        println!("  none");
        return Ok(());
    }
    let mut names = props.keys().cloned().collect::<Vec<_>>();
    names.sort();
    let rows = names
        .into_iter()
        .map(|name| {
            let spec = props.get(&name).cloned().unwrap_or_else(|| json!({}));
            vec![
                styled_body_cell(&name),
                styled_status_cell(if required.contains(&name) {
                    "required"
                } else {
                    "optional"
                }),
                styled_body_cell(spec.get("type").and_then(Value::as_str).unwrap_or("any")),
                styled_body_cell(
                    spec.get("description")
                        .and_then(Value::as_str)
                        .map(normalize_text)
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![
                styled_header_cell("input"),
                styled_header_cell("required"),
                styled_header_cell("shape"),
                styled_header_cell("description")
            ],
            rows,
            2
        )
    );
    Ok(())
}

fn print_capabilities(payload: &Value) -> Result<()> {
    println!("{}", render_primary_heading("Capability Families", ""));
    let tool_counts = payload
        .get("toolFamilies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            Some((
                item.get("family")?.as_str()?.to_string(),
                item.get("tools")?.as_array()?.len(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let workflow_counts = payload
        .get("workflowFamilies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            Some((
                item.get("family")?.as_str()?.to_string(),
                item.get("workflows")?.as_array()?.len(),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    let rows = payload
        .get("families")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|family| {
            let id = family.get("id").and_then(Value::as_str).unwrap_or_default();
            let examples = family
                .get("examples")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "-".to_string());
            vec![
                styled_body_cell(id),
                styled_body_cell(
                    scalar_value(family.get("tier")).unwrap_or_else(|| "?".to_string()),
                ),
                styled_body_cell(tool_counts.get(id).copied().unwrap_or(0).to_string()),
                styled_body_cell(workflow_counts.get(id).copied().unwrap_or(0).to_string()),
                styled_body_cell(normalize_text(
                    family
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )),
                styled_body_cell(examples),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![
                styled_header_cell("family"),
                styled_header_cell("tier"),
                styled_header_cell("tools"),
                styled_header_cell("workflows"),
                styled_header_cell("description"),
                styled_header_cell("examples")
            ],
            rows,
            2
        )
    );
    Ok(())
}

fn print_devices(payload: &Value) -> Result<()> {
    let devices = payload
        .get("devices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("{}", render_primary_heading("Devices", ""));
    println!("{}", render_meta_line(&format!("count: {}", devices.len())));
    if devices.is_empty() {
        println!("No physical devices found.");
        return Ok(());
    }
    let rows = devices
        .into_iter()
        .map(|device| {
            vec![
                styled_body_cell(device.get("name").and_then(Value::as_str).unwrap_or("")),
                styled_body_cell(
                    device
                        .get("platform_version")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                ),
                styled_status_cell(
                    if device
                        .get("is_available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        "available"
                    } else {
                        "offline"
                    },
                ),
                styled_body_cell(device.get("udid").and_then(Value::as_str).unwrap_or("")),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![
                styled_header_cell("device"),
                styled_header_cell("ios"),
                styled_header_cell("status"),
                styled_header_cell("udid")
            ],
            rows,
            2
        )
    );
    Ok(())
}

fn print_recent(entries: &[HistoryEntry]) -> Result<()> {
    let favorites = load_favorites()?.into_iter().collect::<HashSet<_>>();
    println!("{}", render_primary_heading("Recent Runs", ""));
    println!("{}", render_meta_line(&format!("count: {}", entries.len())));
    if entries.is_empty() {
        println!("No recent workflow runs.");
        return Ok(());
    }
    let rows = entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let mode = if entry.commit { "live" } else { "dry-run" };
            let fav = if favorites.contains(&entry.workflow_ref) {
                "*"
            } else {
                ""
            };
            let when = entry
                .ts
                .replace('T', " ")
                .chars()
                .take(16)
                .collect::<String>();
            let args = if entry.args_json != json!({}) && !entry.args_json.is_null() {
                serde_json::to_string(&entry.args_json).unwrap_or_else(|_| "{}".to_string())
            } else {
                "-".to_string()
            };
            vec![
                styled_body_cell((idx + 1).to_string()),
                styled_body_cell(fav),
                styled_body_cell(&entry.workflow_ref),
                styled_status_cell(mode),
                styled_body_cell(when),
                styled_body_cell(entry.udid.chars().take(8).collect::<String>()),
                styled_body_cell(args),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![
                styled_header_cell("#"),
                styled_header_cell("fav"),
                styled_header_cell("workflow"),
                styled_header_cell("mode"),
                styled_header_cell("when"),
                styled_header_cell("device"),
                styled_header_cell("args")
            ],
            rows,
            2
        )
    );
    if !favorites.is_empty() {
        println!();
        println!("* favorite");
    }
    Ok(())
}

fn print_favorites(favorites: &[String]) -> Result<()> {
    println!("{}", render_primary_heading("Favorites", ""));
    println!(
        "{}",
        render_meta_line(&format!("count: {}", favorites.len()))
    );
    if favorites.is_empty() {
        println!("No favorite workflows.");
    } else {
        let rows = favorites
            .iter()
            .map(|favorite| vec![styled_body_cell(favorite)])
            .collect::<Vec<_>>();
        println!(
            "{}",
            render_cli_table(vec![styled_header_cell("workflow")], rows, 2)
        );
    }
    Ok(())
}

fn print_skill_result(title: &str, payload: &Value) -> Result<()> {
    let skill = payload
        .get("skill")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let scope = payload
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let version = payload
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let results = payload
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    println!("{}", render_primary_heading(title, ""));
    println!(
        "{}",
        render_meta_line(&format!(
            "skill: {} | scope: {} | version: {} | clients: {}",
            skill,
            scope,
            version,
            results.len()
        ))
    );
    if results.is_empty() {
        println!("No client targets.");
        return Ok(());
    }

    let rows = results
        .into_iter()
        .map(|item| {
            let state = item
                .get("action")
                .or_else(|| item.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("-");
            vec![
                styled_body_cell(
                    item.get("clientLabel")
                        .and_then(Value::as_str)
                        .unwrap_or("?"),
                ),
                styled_status_cell(state),
                styled_body_cell(item.get("path").and_then(Value::as_str).unwrap_or("-")),
                styled_body_cell(item.get("source").and_then(Value::as_str).unwrap_or("-")),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![
                styled_header_cell("client"),
                styled_header_cell("status"),
                styled_header_cell("path"),
                styled_header_cell("source")
            ],
            rows,
            2
        )
    );
    Ok(())
}

fn print_bundled_skills(payload: &Value) -> Result<()> {
    let skills = payload
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("{}", render_primary_heading("Bundled Skills", ""));
    println!(
        "{}",
        render_meta_line(&format!(
            "count: {} | dir: {}",
            skills.len(),
            payload
                .get("skillsDir")
                .and_then(Value::as_str)
                .unwrap_or("-")
        ))
    );
    if skills.is_empty() {
        println!("No bundled skills found.");
        return Ok(());
    }
    let rows = skills
        .into_iter()
        .map(|skill| {
            vec![
                styled_body_cell(skill.get("name").and_then(Value::as_str).unwrap_or("?")),
                styled_body_cell(skill.get("path").and_then(Value::as_str).unwrap_or("-")),
            ]
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        render_cli_table(
            vec![styled_header_cell("skill"), styled_header_cell("path")],
            rows,
            2
        )
    );
    Ok(())
}

fn print_value(value: &Value, _as_json: bool, pretty: Option<String>) -> Result<()> {
    if let Some(pretty) = pretty {
        print!("{}", pretty);
        return Ok(());
    }
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
