async fn call_tool(state: &AppState, tool: &str, arguments: Value) -> Result<Value> {
    let payload = match tools::handle_tool_call(state, tool, arguments).await {
        Ok(result) => result,
        Err(err) => tools::tool_error_from_anyhow(&err, tool),
    };

    if payload
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let structured = payload
            .get("structuredContent")
            .cloned()
            .unwrap_or(payload.clone());
        bail!(
            "{}",
            serde_json::to_string_pretty(&structured).unwrap_or_else(|_| structured.to_string())
        );
    }

    Ok(payload.get("structuredContent").cloned().unwrap_or(payload))
}

async fn handle_workflow_broken_report(args: WorkflowBrokenReportArgs) -> Result<()> {
    let flow = args.flow.ok_or_else(|| anyhow!("--flow is required"))?;
    let flow_version = args
        .flow_version
        .ok_or_else(|| anyhow!("--flow-version is required"))?;
    let failed_stage = args
        .failed_stage
        .ok_or_else(|| anyhow!("--failed-stage is required"))?;
    let summary = json!({
        "surface": args.surface.unwrap_or_else(|| args.platform.clone()),
        "flow": flow,
        "flow_version": flow_version,
        "failed_stage": failed_stage,
        "error": args.error,
        "app_version": args.app_version,
        "platform": args.platform
    });
    let draft = workflow_failure_report::draft_from_value(&summary, args.note.clone())?;

    print_value(&workflow_failure_report::review_payload(&draft), true, None)?;
    println!("No report was submitted. The RZN host should submit the draft with auth context.");
    Ok(())
}
fn find_tool(name: &str) -> Result<Value> {
    let tools = tools::list_tool_definitions();
    tools
        .iter()
        .find(|item| item.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
        .ok_or_else(|| unknown_tool_error(name, &tools))
}

fn unknown_tool_error(name: &str, tools: &[Value]) -> anyhow::Error {
    let suggestions = closest_matches(
        name,
        tools.iter().filter_map(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        }),
    );
    if suggestions.is_empty() {
        anyhow!("unknown tool '{}'", name)
    } else {
        anyhow!(
            "unknown tool '{}'\nDid you mean:\n  {}",
            name,
            suggestions.join("\n  ")
        )
    }
}
fn filtered_tools(args: &ToolListArgs) -> Result<Vec<Value>> {
    let mut tools = tools::list_tool_definitions();
    tools.retain(|tool| {
        if args.direct {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            if matches!(
                name,
                "ios.capability.list" | "ios.workflow.list" | "ios.workflow.run" | "ios.script.run"
            ) {
                return false;
            }
        }
        if let Some(family) = args.family.as_deref() {
            let tool_family = tool
                .get("capabilityFamily")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !tool_family.eq_ignore_ascii_case(family) {
                return false;
            }
        }
        if let Some(tier) = args.tier.as_deref() {
            if scalar_value(tool.get("capabilityTier"))
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_default()
                != tier.to_ascii_lowercase()
            {
                return false;
            }
        }
        if let Some(search) = args.search.as_deref() {
            let search = search.to_ascii_lowercase();
            let haystack = format!(
                "{}\n{}\n{}\n{}\n{}",
                tool.get("name").and_then(Value::as_str).unwrap_or_default(),
                tool.get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                tool.get("capabilityFamily")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                tool.get("capabilityTier")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                tool.get("inputSchema")
                    .and_then(|schema| schema.get("properties"))
                    .and_then(Value::as_object)
                    .map(|props| props.keys().cloned().collect::<Vec<_>>().join(" "))
                    .unwrap_or_default()
            )
            .to_ascii_lowercase();
            if !haystack.contains(&search) {
                return false;
            }
        }
        true
    });
    tools.sort_by_key(|tool| {
        tool.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    Ok(tools)
}
