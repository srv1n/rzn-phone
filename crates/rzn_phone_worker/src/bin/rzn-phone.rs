#![allow(clippy::items_after_test_module)]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS,
    presets::{ASCII_FULL, UTF8_FULL_CONDENSED},
    Attribute, Cell, Color, ContentArrangement, Table,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Map, Value};
use strsim::jaro_winkler;
use terminal_size::{terminal_size, Width};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use rzn_phone_worker::state::AppState;
use rzn_phone_worker::{tools, workflow_failure_report, workflows};
include!("rzn_phone_cli/args.rs");
include!("rzn_phone_cli/output.rs");
include!("rzn_phone_cli/runtime.rs");
include!("rzn_phone_cli/install.rs");
include!("rzn_phone_cli/worker_client.rs");
include!("rzn_phone_cli/workflows.rs");
include!("rzn_phone_cli/history.rs");
include!("rzn_phone_cli/favorites.rs");
include!("rzn_phone_cli/completions.rs");
include!("rzn_phone_cli/update.rs");

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let runtime = runtime_paths()?;
    env::set_var("RZN_PLUGIN_DIR", &runtime.plugin_root);
    env::set_var("CLAUDE_PLUGIN_ROOT", &runtime.plugin_root);

    match cli.command {
        CommandKind::Worker => {
            rzn_phone_worker::run_worker_stdio().await?;
        }
        CommandKind::Doctor(args) => {
            prepare_pretty_output(args.pretty);
            let state = AppState::new();
            let payload = call_tool(&state, "ios.env.doctor", json!({})).await?;
            print_value(&payload, args.json || !want_pretty(args.pretty), None)?;
        }
        CommandKind::Devices(args) => {
            prepare_pretty_output(args.pretty);
            let state = AppState::new();
            let payload = call_tool(
                &state,
                "ios.device.list",
                json!({"includeSimulators": false}),
            )
            .await?;
            if args.json {
                print_value(&payload, true, None)?;
            } else {
                print_devices(&payload)?;
            }
        }
        CommandKind::Status => {
            maybe_cleanup_stale_runtime_cache(&runtime).await?;
            env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
            let state = AppState::new();
            let payload = call_tool(&state, "rzn.worker.health", json!({})).await?;
            print_value(&payload, true, None)?;
        }
        CommandKind::Shutdown(args) => {
            env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
            let state = AppState::new();
            let payload = call_tool(
                &state,
                "rzn.worker.shutdown",
                json!({
                    "stopAppium": args.stop_appium,
                    "shutdownWDA": true,
                    "backgroundApp": args.background_on_exit,
                    "lockDevice": args.lock_device_on_exit
                }),
            )
            .await?;
            print_value(&payload, true, None)?;
        }
        CommandKind::Version => {
            print_value(
                &json!({
                    "runtimeVersion": runtime_version(&runtime)?,
                    "workflowPackVersion": workflow_pack_version(&runtime)?
                }),
                true,
                None,
            )?;
        }
        CommandKind::Info => {
            print_value(
                &json!({
                    "root": runtime.root,
                    "pluginRoot": runtime.plugin_root,
                    "runtimeVersion": runtime_version(&runtime)?,
                    "workflowPackVersion": workflow_pack_version(&runtime)?,
                    "updateSource": default_update_source(&runtime),
                    "worker": runtime.worker,
                    "workflowDir": runtime.workflow_dir,
                    "examplesDir": runtime.examples_dir,
                    "skillsDir": runtime.skills_dir,
                }),
                true,
                None,
            )?;
        }
        CommandKind::Run(args) => {
            let args_json = read_json_input(&args.args_json)?;
            let workflow_ref = match resolve_run_target(&args.first, args.second.as_deref())? {
                RunTarget::Workflow(workflow_ref) => workflow_ref,
                RunTarget::SystemNamespace(system_id) => {
                    let workflows = workflows::list_workflows(Some(&system_id), None);
                    let pretty = render_system_run_help(&system_id, &workflows)?;
                    let structured = json!({
                        "ok": false,
                        "error": format!("system '{}' is a namespace, not a runnable workflow", system_id),
                        "errorCode": "WORKFLOW_NAMESPACE",
                        "system": system_id,
                        "workflows": workflows
                    });
                    exit_with_help_output(&pretty, &structured, args.json);
                }
            };
            let workflow = find_workflow(&workflow_ref)?;
            let workflow_value = serde_json::to_value(&workflow)?;
            let missing_required = missing_required_params(&workflow_value, &args_json);
            if !missing_required.is_empty() {
                let pretty = render_workflow_help(&workflow_value, true, &missing_required)?;
                let structured = json!({
                    "ok": false,
                    "error": format!("missing required workflow params: {}", missing_required.join(", ")),
                    "errorCode": "MISSING_REQUIRED_PARAMS",
                    "workflow": workflow,
                    "missing": missing_required,
                    "exampleCommand": workflow_examples(&workflow_value, false)
                        .first()
                        .and_then(|example| example.get("args"))
                        .cloned()
                        .and_then(|example_args| example_command(&workflow_value, example_args).ok())
                });
                exit_with_help_output(&pretty, &structured, args.json);
            }
            let udid = resolve_run_udid(args.udid.clone()).await.with_context(|| {
                format!("unable to resolve device for workflow '{}'", workflow_ref)
            })?;
            let commit = if args.dry_run { false } else { args.commit };
            let smart_cache_active = args.fast.unwrap_or(true)
                && !args.disconnect_on_finish
                && !args.stop_appium_on_finish
                && !args.background_on_exit
                && !args.lock_device_on_exit;

            if smart_cache_active {
                env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
                env::set_var("RZN_IOS_REUSE_ACTIVE_SESSION", "1");
                maybe_cleanup_stale_runtime_cache(&runtime).await?;
            } else {
                env::remove_var("RZN_IOS_PERSIST_RUNTIME");
                env::remove_var("RZN_IOS_REUSE_ACTIVE_SESSION");
            }

            maybe_print_cold_start_notice(smart_cache_active, &udid, args.json);

            let state = AppState::new();
            let payload = call_tool(
                &state,
                "ios.workflow.run",
                json!({
                    "workflow": workflow_ref,
                    "session": build_session_json(&udid),
                    "args": args_json,
                    "commit": commit,
                    "disconnectOnFinish": if smart_cache_active { false } else { args.disconnect_on_finish },
                    "stopAppiumOnFinish": if smart_cache_active { false } else { args.stop_appium_on_finish },
                    "backgroundAppOnFinish": if smart_cache_active { false } else { args.background_on_exit },
                    "lockDeviceOnFinish": if smart_cache_active { false } else { args.lock_device_on_exit },
                }),
            )
            .await?;

            record_recent_run(HistoryEntry {
                ts: iso_now(),
                workflow_ref: workflow_ref.clone(),
                udid,
                args_json: payload_input_args(&payload).unwrap_or_else(|| {
                    read_json_input(&args.args_json).unwrap_or_else(|_| json!({}))
                }),
                commit,
                disconnect_on_finish: args.disconnect_on_finish,
                stop_appium_on_finish: args.stop_appium_on_finish,
                background_on_exit: args.background_on_exit,
                lock_device_on_exit: args.lock_device_on_exit,
                smart_cache: smart_cache_active,
            })?;

            let pretty = if !args.json && io::stdout().is_terminal() {
                workflow_presentation(&payload)
            } else {
                None
            };
            print_value(&payload, args.json || pretty.is_none(), pretty)?;
        }
        CommandKind::List(args) => {
            prepare_pretty_output(args.pretty);
            let payload = workflow_payload(args.family.as_deref());
            if args.json {
                print_value(&filtered_workflow_payload(&payload, &args)?, true, None)?;
            } else {
                print_workflow_list(&payload, &args)?;
            }
        }
        CommandKind::Show(args) => {
            prepare_pretty_output(args.pretty);
            let tool_defs = tools::list_tool_definitions();
            if args.second.is_none()
                && tool_defs.iter().any(|item| {
                    item.get("name").and_then(Value::as_str) == Some(args.first.as_str())
                })
            {
                let tool = tool_defs
                    .into_iter()
                    .find(|item| {
                        item.get("name").and_then(Value::as_str) == Some(args.first.as_str())
                    })
                    .ok_or_else(|| anyhow!("unknown tool '{}'", args.first))?;
                if args.json {
                    print_value(&tool, true, None)?;
                } else {
                    print_tool_show(&tool)?;
                }
            } else {
                let workflow_ref = normalize_workflow_ref(&args.first, args.second.as_deref())?;
                let workflow = find_workflow(&workflow_ref)?;
                if args.json {
                    print_value(&serde_json::to_value(&workflow)?, true, None)?;
                } else {
                    print_workflow_show(&serde_json::to_value(&workflow)?, args.example)?;
                }
            }
        }
        CommandKind::Capability { command } => match command {
            CapabilityCommand::List(args) => {
                prepare_pretty_output(args.pretty);
                let state = AppState::new();
                let payload = call_tool(&state, "ios.capability.list", json!({})).await?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_capabilities(&payload)?;
                }
            }
        },
        CommandKind::Tool { command } => match command {
            ToolCommand::List(args) => {
                prepare_pretty_output(args.pretty);
                let tools = filtered_tools(&args)?;
                if args.json {
                    print_value(&json!({ "tools": tools }), true, None)?;
                } else {
                    print_tool_list(&tools)?;
                }
            }
            ToolCommand::Show(args) => {
                prepare_pretty_output(args.pretty);
                let tool = find_tool(&args.name)?;
                if args.json {
                    print_value(&tool, true, None)?;
                } else {
                    print_tool_show(&tool)?;
                }
            }
            ToolCommand::Call(args) => {
                let state = AppState::new();
                let payload =
                    call_tool(&state, &args.name, read_json_input(&args.args_json)?).await?;
                print_value(&payload, true, None)?;
            }
        },
        CommandKind::Tools(args) => {
            prepare_pretty_output(args.pretty);
            let tools = filtered_tools(&ToolListArgs {
                direct: args.direct,
                search: args.search,
                family: args.family,
                tier: args.tier,
                json: args.json,
                pretty: args.pretty,
            })?;
            if args.json {
                print_value(&json!({ "tools": tools }), true, None)?;
            } else {
                print_tool_list(&tools)?;
            }
        }
        CommandKind::Recent(args) => {
            prepare_pretty_output(args.pretty);
            let entries = load_recent(args.limit)?;
            if args.json {
                print_value(&serde_json::to_value(entries)?, true, None)?;
            } else {
                print_recent(&entries)?;
            }
        }
        CommandKind::History { command } => match command {
            HistoryCommand::Clear => {
                clear_history()?;
                println!("History cleared");
            }
            HistoryCommand::Redact => {
                let count = redact_history_file()?;
                println!("History redacted ({count} entries)");
            }
            HistoryCommand::Path => println!("{}", history_path()?.display()),
        },
        CommandKind::Rerun { index } => {
            let entry = rerun_entry(index)?;
            let command = rerun_command(&entry)?;
            exec_self(&command)?;
        }
        CommandKind::Favorite { command } => match command {
            FavoriteCommand::Add { reference } => {
                let reference = canonicalize_workflow_ref(&reference);
                let mut favorites = load_favorites()?;
                if !favorites.contains(&reference) {
                    favorites.push(reference.clone());
                }
                save_favorites(&favorites)?;
                println!("Favorited {}", reference);
            }
            FavoriteCommand::Remove { reference } => {
                let reference = canonicalize_workflow_ref(&reference);
                let favorites = load_favorites()?
                    .into_iter()
                    .filter(|item| item != &reference)
                    .collect::<Vec<_>>();
                save_favorites(&favorites)?;
                println!("Removed {}", reference);
            }
            FavoriteCommand::List(args) => {
                prepare_pretty_output(args.pretty);
                let favorites = load_favorites()?;
                if args.json {
                    print_value(&serde_json::to_value(favorites)?, true, None)?;
                } else {
                    print_favorites(&favorites)?;
                }
            }
        },
        CommandKind::Favorites(args) => {
            prepare_pretty_output(args.pretty);
            let favorites = load_favorites()?;
            if args.json {
                print_value(&serde_json::to_value(favorites)?, true, None)?;
            } else {
                print_favorites(&favorites)?;
            }
        }
        CommandKind::Completion(args) => {
            print!("{}", completion_script(&args.shell)?);
        }
        CommandKind::Skill { command } => match command {
            SkillCommand::Install(args) => {
                prepare_pretty_output(args.pretty);
                let payload = install_skill_links(&runtime, &args, false)?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_skill_result("Skill Install", &payload)?;
                }
            }
            SkillCommand::Update(mut args) => {
                prepare_pretty_output(args.pretty);
                args.force = true;
                let payload = install_skill_links(&runtime, &args, true)?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_skill_result("Skill Update", &payload)?;
                }
            }
            SkillCommand::Remove(args) => {
                prepare_pretty_output(args.pretty);
                let payload = remove_skill_links(&args)?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_skill_result("Skill Remove", &payload)?;
                }
            }
            SkillCommand::Status(args) => {
                prepare_pretty_output(args.pretty);
                let payload = skill_status_payload(&runtime, &args)?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_skill_result("Skill Status", &payload)?;
                }
            }
            SkillCommand::List(args) => {
                prepare_pretty_output(args.pretty);
                let payload = bundled_skills_payload(&runtime)?;
                if args.json {
                    print_value(&payload, true, None)?;
                } else {
                    print_bundled_skills(&payload)?;
                }
            }
        },
        CommandKind::Workflows { command } => match command {
            WorkflowsCommand::Update(args) => {
                update_workflows(&runtime, args.source, args.version).await?;
            }
            WorkflowsCommand::Path => println!("{}", runtime.workflow_dir.display()),
        },
        CommandKind::Report { command } => match command {
            ReportCommand::WorkflowBroken(args) => {
                handle_workflow_broken_report(args).await?;
            }
            ReportCommand::Queue(args) => {
                prepare_pretty_output(args.pretty);
                let state = AppState::new();
                let payload = call_tool(
                    &state,
                    "rzn.workflow_failure_report.queue",
                    json!({ "action": args.action }),
                )
                .await?;
                print_value(&payload, args.json || !want_pretty(args.pretty), None)?;
            }
        },
        CommandKind::Workflow { command } => match command {
            WorkflowAliasCommand::List(args) => {
                prepare_pretty_output(args.pretty);
                let payload = workflow_payload(args.family.as_deref());
                if args.json {
                    print_value(&filtered_workflow_payload(&payload, &args)?, true, None)?;
                } else {
                    print_workflow_list(&payload, &args)?;
                }
            }
            WorkflowAliasCommand::Show(args) => {
                prepare_pretty_output(args.pretty);
                let workflow_ref = normalize_workflow_ref(&args.first, args.second.as_deref())?;
                let workflow = find_workflow(&workflow_ref)?;
                if args.json {
                    print_value(&serde_json::to_value(&workflow)?, true, None)?;
                } else {
                    print_workflow_show(&serde_json::to_value(&workflow)?, args.example)?;
                }
            }
        },
        CommandKind::Examples { command } => match command {
            ExamplesCommand::Path => println!("{}", runtime.examples_dir.display()),
        },
        CommandKind::CompleteValues { entity } => {
            for value in complete_values(&entity)? {
                println!("{}", value);
            }
        }
    }

    Ok(())
}

fn rerun_command(entry: &HistoryEntry) -> Result<Vec<String>> {
    Ok(vec![
        "run".to_string(),
        entry.workflow_ref.clone(),
        "--udid".to_string(),
        entry.udid.clone(),
        "--args-json".to_string(),
        serde_json::to_string(&entry.args_json)?,
        format!("--commit={}", entry.commit),
        format!("--disconnect-on-finish={}", entry.disconnect_on_finish),
        format!("--stop-appium-on-finish={}", entry.stop_appium_on_finish),
        format!("--background-on-exit={}", entry.background_on_exit),
        format!("--lock-device-on-exit={}", entry.lock_device_on_exit),
        format!("--fast={}", entry.smart_cache),
    ])
}

#[cfg(test)]
mod cli_rerun_tests {
    use super::*;

    #[test]
    fn rerun_command_uses_inline_boolean_values_that_round_trip() {
        let entry = HistoryEntry {
            ts: "2026-06-12T00:00:00Z".to_string(),
            workflow_ref: "safari/google_search".to_string(),
            udid: "TEST-UDID-RERUN-001".to_string(),
            args_json: json!({"query": "headphones", "limit": 5}),
            commit: false,
            disconnect_on_finish: false,
            stop_appium_on_finish: false,
            background_on_exit: false,
            lock_device_on_exit: false,
            smart_cache: true,
        };

        let command = rerun_command(&entry).expect("rerun command");
        assert_eq!(
            command,
            vec![
                "run",
                "safari/google_search",
                "--udid",
                "TEST-UDID-RERUN-001",
                "--args-json",
                "{\"limit\":5,\"query\":\"headphones\"}",
                "--commit=false",
                "--disconnect-on-finish=false",
                "--stop-appium-on-finish=false",
                "--background-on-exit=false",
                "--lock-device-on-exit=false",
                "--fast=true",
            ]
        );
        assert!(!command.iter().any(|arg| arg == "true" || arg == "false"));

        let mut argv = vec!["rzn-phone".to_string()];
        argv.extend(command);
        match Cli::try_parse_from(argv).expect("round-trip parse").command {
            CommandKind::Run(args) => {
                assert_eq!(args.first, "safari/google_search");
                assert!(!args.commit);
                assert!(!args.disconnect_on_finish);
                assert!(!args.stop_appium_on_finish);
                assert!(!args.background_on_exit);
                assert!(!args.lock_device_on_exit);
                assert_eq!(args.fast, Some(true));
            }
            _ => panic!("expected run command"),
        }
    }
}
