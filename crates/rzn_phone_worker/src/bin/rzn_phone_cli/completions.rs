fn exec_self(args: &[String]) -> Result<()> {
    let exe = env::current_exe()?;
    let status = Command::new(exe).args(args).status()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn scalar_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn completion_script(shell: &str) -> Result<String> {
    let command_name = "rzn-phone";
    match shell {
        "bash" => Ok(format!(
            r#"# bash completion for {command_name}
_{name}_complete() {{
  local cur prev words cword
  _init_completion || return
  case "$prev" in
    --family)
      COMPREPLY=( $(compgen -W "$({command_name} __complete-values families)" -- "$cur") )
      return
      ;;
  esac
  if [[ $cword -eq 1 ]]; then
    COMPREPLY=( $(compgen -W "$({command_name} __complete-values commands)" -- "$cur") )
    return
  fi
  case "${{words[1]}}" in
    run|show)
      COMPREPLY=( $(compgen -W "$({command_name} __complete-values workflows)" -- "$cur") )
      ;;
    tool)
      COMPREPLY=( $(compgen -W "$({command_name} __complete-values tools)" -- "$cur") )
      ;;
    favorite)
      COMPREPLY=( $(compgen -W "$({command_name} __complete-values favorites)" -- "$cur") )
      ;;
  esac
}}
complete -F _{name}_complete {command_name}
"#,
            name = command_name.replace('-', "_")
        )),
        "zsh" => Ok(format!(
            r#"#compdef {command_name}

_{name}_commands() {{
  local -a cmds
  cmds=(
    'worker:Run the MCP worker over stdio'
    'doctor:Check local prerequisites'
    'setup:Detect signing identity and write starter config'
    'config:View or edit persistent configuration'
    'devices:List connected physical iPhones'
    'status:Show runtime status'
    'shutdown:Shutdown active runtime'
    'version:Show runtime and workflow-pack versions'
    'info:Show install metadata'
    'run:Run a workflow'
    'list:List workflows grouped by system'
    'show:Show a workflow or tool'
    'capability:Inspect capability families'
    'tool:Inspect or call a tool'
    'recent:Show recent workflow runs'
    'history:Manage local run history'
    'rerun:Rerun a previous workflow'
    'favorite:Manage favorite workflows'
    'completion:Print shell completion script'
    'skill:Install agent skill symlinks'
    'workflows:Manage workflow packs'
    'report:Draft or inspect workflow failure reports'
    'examples:Show paths to installed examples'
  )
  _describe 'command' cmds
}}

_{name}() {{
  if (( CURRENT == 2 )); then
    _{name}_commands
    return
  fi
  case "$words[2]" in
    run|show)
      local -a refs
      refs=("${{(@f)$({command_name} __complete-values workflows)}}")
      _describe 'workflow' refs
      ;;
    tool)
      local -a tools
      tools=("${{(@f)$({command_name} __complete-values tools)}}")
      _describe 'tool' tools
      ;;
    *)
      _arguments '*::arg: '
      ;;
  esac
}}

_{name} "$@"
"#,
            name = command_name.replace('-', "_")
        )),
        other => bail!("unsupported shell: {}", other),
    }
}

fn complete_values(entity: &str) -> Result<Vec<String>> {
    let values = match entity {
        "commands" => vec![
            "worker",
            "doctor",
            "setup",
            "config",
            "devices",
            "status",
            "shutdown",
            "version",
            "info",
            "run",
            "list",
            "show",
            "capability",
            "tool",
            "recent",
            "history",
            "rerun",
            "favorite",
            "completion",
            "skill",
            "workflows",
            "report",
            "examples",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect(),
        "workflows" => workflows::list_workflows(None, None)
            .into_iter()
            .map(|item| item.id)
            .collect(),
        "systems" => workflows::group_workflows_by_system(&workflows::list_workflows(None, None))
            .into_iter()
            .map(|item| item.id)
            .collect(),
        "tools" => tools::list_tool_definitions()
            .into_iter()
            .filter_map(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect(),
        "families" => {
            let state = AppState::new();
            let rt = tokio::runtime::Runtime::new()?;
            let payload = rt.block_on(call_tool(&state, "ios.capability.list", json!({})))?;
            payload
                .get("families")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        }
        "favorites" => load_favorites()?,
        _ => bail!("unknown completion entity: {}", entity),
    };
    Ok(values)
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn completion_commands_match_the_canonical_surface() {
        let commands = complete_values("commands").expect("commands");
        for command in [
            "worker",
            "setup",
            "config",
            "capability",
            "completion",
            "report",
            "examples",
        ] {
            assert!(commands.iter().any(|item| item == command), "missing {command}");
        }
        for alias in ["tools", "workflow", "favorites", "skills"] {
            assert!(!commands.iter().any(|item| item == alias), "removed {alias}");
        }
    }

    #[test]
    fn generated_shell_completion_omits_removed_aliases() {
        for shell in ["bash", "zsh"] {
            let script = completion_script(shell).expect("completion script");
            for alias in ["tools", "workflow", "favorites", "skills"] {
                assert!(!script.contains(&format!("'{alias}:")), "removed {alias} in {shell}");
            }
        }
    }
}
