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
            let env_payload = call_tool(&state, "ios.env.doctor", json!({})).await?;
            render_doctor(&env_payload, args.json)?;
        }
        CommandKind::Setup(args) => {
            handle_setup(&args).await?;
        }
        CommandKind::Config { command } => {
            handle_config(command)?;
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
            // First run with no config and no env signing: auto-detect the
            // Xcode team and persist a starter config so testers are zero-touch.
            maybe_autoconfigure_signing(args.json);
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

use rzn_phone_worker::config::{self, RznConfig};

/// `rzn-phone setup` — detect (or accept) a signing team, write config.json,
/// and verify by building/launching WDA so the device-trust prompt happens now
/// instead of mid-run. Output is agent-actionable on failure.
async fn handle_setup(args: &SetupArgs) -> Result<()> {
    prepare_pretty_output(args.pretty);

    if RznConfig::exists() && !args.force {
        let payload = json!({
            "ok": true,
            "alreadyConfigured": true,
            "configPath": config::config_path(),
            "hint": "pass --force to re-detect, `rzn-phone config show` to inspect, `rzn-phone doctor` to verify",
        });
        print_value(&payload, true, None)?;
        return Ok(());
    }

    let detected = match args.team_id.clone() {
        Some(team) => config::DetectedSigning {
            team_id: Some(team),
            source: "explicit".to_string(),
            ..Default::default()
        },
        None => config::detect_signing(),
    };
    let team_id = detected.team_id.clone();

    let mut cfg = RznConfig::load();
    cfg.signing.xcode_org_id = team_id.clone();
    cfg.signing.allow_provisioning_updates = Some(true);
    if cfg.run.disconnect_on_finish.is_none() {
        cfg.run.disconnect_on_finish = Some(false);
    }
    cfg.meta = Some(json!({"generatedBy": "rzn-phone setup", "schema": 1}));
    let config_path = cfg.save()?;

    // Optionally verify by warming WDA on a connected device.
    let mut verify = Value::Null;
    if !args.no_verify && team_id.is_some() {
        match resolve_run_udid(args.udid.clone()).await {
            Ok(udid) => {
                env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
                env::set_var("RZN_IOS_REUSE_ACTIVE_SESSION", "1");
                if !args.json && io::stderr().is_terminal() {
                    eprintln!("Verifying signing by building/launching WDA on {udid} (first time can take a few minutes; keep the device unlocked)...");
                }
                let mut create = build_session_json(&udid);
                if let Some(obj) = create.as_object_mut() {
                    obj.insert("kind".to_string(), json!("safari_web"));
                    obj.insert("replaceExisting".to_string(), json!(false));
                }
                let state = AppState::new();
                let payload =
                    match tools::handle_tool_call(&state, "ios.session.create", create).await {
                        Ok(p) => p,
                        Err(err) => tools::tool_error_from_anyhow(&err, "ios.session.create"),
                    };
                let is_error = payload
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let structured = payload.get("structuredContent").cloned().unwrap_or(payload);
                verify = json!({
                    "ran": true,
                    "ok": !is_error,
                    "udid": udid,
                    "result": structured,
                });
            }
            Err(err) => {
                verify = json!({
                    "ran": false,
                    "ok": false,
                    "reason": format!("{err:#}"),
                    "hint": "connect + unlock an iPhone and tap Trust, then rerun `rzn-phone setup --force`, or skip with --no-verify",
                });
            }
        }
    }

    let verify_ok = verify.get("ok").and_then(Value::as_bool);
    let payload = json!({
        "ok": team_id.is_some() && verify_ok != Some(false),
        "configPath": config_path,
        "teamId": team_id,
        "isFree": detected.is_free,
        "detected": detected,
        "verify": verify,
        "onDevice": config::manual_device_steps(),
        "next": setup_next_hint(team_id.is_some(), detected.is_free, verify_ok),
    });
    print_value(&payload, true, None)?;
    Ok(())
}

fn setup_next_hint(have_team: bool, is_free: Option<bool>, verify_ok: Option<bool>) -> String {
    if !have_team {
        return "No signing team. Sign into Xcode > Settings > Accounts (a free Apple ID works), then rerun `rzn-phone setup --force`. Or pass --team-id <ID>.".to_string();
    }
    match verify_ok {
        Some(false) => {
            "WDA verification failed — see verify.result.remediation, or run `rzn-phone doctor`."
                .to_string()
        }
        Some(true) | None => {
            let mut hint = "Ready. Try: rzn-phone run safari google_search --args-json '{\"query\":\"hello\"}'".to_string();
            if is_free == Some(true) {
                hint.push_str(" (free team: WDA re-signs automatically about once a week)");
            }
            hint
        }
    }
}

/// Render `rzn-phone doctor`: prerequisites + signing readiness + on-device
/// steps, tagged manual vs agent-fixable so it can be pasted into a coding agent.
fn render_doctor(env_payload: &Value, json_output: bool) -> Result<()> {
    let structured = env_payload
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| env_payload.clone());
    let prereqs = structured
        .get("checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let signing = config::signing_doctor();
    let on_device = config::manual_device_steps();

    let prereqs_ok = prereqs
        .iter()
        .all(|c| c.get("ok").and_then(Value::as_bool).unwrap_or(false));
    let signing_fail = signing
        .iter()
        .any(|c| matches!(c.status, config::CheckStatus::Fail));
    let overall_ok = prereqs_ok && !signing_fail;

    if json_output {
        print_value(
            &json!({
                "ok": overall_ok,
                "prerequisites": prereqs,
                "signing": signing,
                "onDevice": on_device,
                "remediationEnv": structured.get("remediation"),
            }),
            true,
            None,
        )?;
        return Ok(());
    }

    let mut out = String::new();
    out.push_str("rzn-phone doctor\n\n");

    out.push_str("Prerequisites\n");
    for c in &prereqs {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("?");
        let ok = c.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let mark = if ok { "ok  " } else { "FAIL" };
        out.push_str(&format!("  [{mark}] {name}\n"));
    }
    if !prereqs_ok {
        if let Some(rem) = structured.get("remediation").and_then(Value::as_array) {
            out.push_str("    fixes:\n");
            for r in rem {
                if let Some(s) = r.as_str() {
                    out.push_str(&format!("      - {s}\n"));
                }
            }
        }
    }

    out.push_str("\nSigning\n");
    for c in &signing {
        let mark = match c.status {
            config::CheckStatus::Ok => "ok  ",
            config::CheckStatus::Warn => "WARN",
            config::CheckStatus::Fail => "FAIL",
        };
        out.push_str(&format!("  [{}] {}: {}\n", mark, c.label, c.detail));
        if let Some(fix) = &c.fix {
            let tag = match c.fix_kind {
                Some(config::FixKind::Manual) => "manual",
                Some(config::FixKind::Agent) => "agent",
                None => "fix",
            };
            out.push_str(&format!("        ({tag}) {fix}\n"));
        }
    }

    out.push_str("\nOn-device (manual — a coding agent cannot do these)\n");
    for s in &on_device {
        out.push_str(&format!("  - {}\n", s.text));
    }

    out.push_str(&format!(
        "\n{}\n",
        if overall_ok {
            "Ready. Run: rzn-phone run safari google_search --args-json '{\"query\":\"hello\"}'"
        } else {
            "Resolve the FAIL items above (do the (manual) ones yourself; (agent) ones can be run for you), then rerun `rzn-phone doctor`."
        }
    ));

    print!("{out}");
    Ok(())
}

/// `rzn-phone config <show|path|get|set>`.
fn handle_config(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show(args) => {
            prepare_pretty_output(args.pretty);
            let cfg = RznConfig::load();
            print_value(
                &json!({
                    "configPath": config::config_path(),
                    "exists": RznConfig::exists(),
                    "config": serde_json::to_value(&cfg)?,
                }),
                true,
                None,
            )?;
        }
        ConfigCommand::Path(args) => {
            prepare_pretty_output(args.pretty);
            print_value(
                &json!({
                    "config": config::config_path(),
                    "configDir": config::config_dir(),
                    "stateDir": config::state_dir(),
                    "legacyStateDir": config::legacy_state_dir(),
                    "dataDir": config::data_dir(),
                }),
                true,
                None,
            )?;
        }
        ConfigCommand::Get { key } => {
            let cfg = RznConfig::load();
            let value = config_get(&cfg, &key)?;
            print_value(&json!({ "key": key, "value": value }), true, None)?;
        }
        ConfigCommand::Set { key, value } => {
            let mut cfg = RznConfig::load();
            config_set(&mut cfg, &key, &value)?;
            let path = cfg.save()?;
            print_value(
                &json!({ "ok": true, "key": key, "value": value, "configPath": path }),
                true,
                None,
            )?;
        }
    }
    Ok(())
}

/// First-run hook: if there is no config file and no env signing override,
/// auto-detect the Xcode team and persist a starter config so the very first
/// run is configured without any manual step.
fn maybe_autoconfigure_signing(json_output: bool) {
    if RznConfig::exists() || env::var("IOS_XCODE_ORG_ID").is_ok() {
        return;
    }
    let detected = config::detect_signing();
    let Some(team) = detected.team_id.clone() else {
        return;
    };
    let mut cfg = RznConfig::default();
    cfg.signing.xcode_org_id = Some(team.clone());
    cfg.signing.allow_provisioning_updates = Some(true);
    cfg.run.disconnect_on_finish = Some(false);
    cfg.meta = Some(json!({"generatedBy": "rzn-phone first-run autoconfigure", "schema": 1}));
    if let Ok(path) = cfg.save() {
        if !json_output && io::stderr().is_terminal() {
            eprintln!(
                "Configured signing team {} ({}). Saved to {}.",
                team,
                detected.source,
                path.display()
            );
        }
    }
}

fn config_get(cfg: &RznConfig, key: &str) -> Result<Value> {
    let full = serde_json::to_value(cfg)?;
    let mut node = &full;
    for part in key.split('.') {
        node = node
            .get(part)
            .ok_or_else(|| anyhow!("unknown config key '{}'", key))?;
    }
    Ok(node.clone())
}

fn config_set(cfg: &mut RznConfig, key: &str, value: &str) -> Result<()> {
    let s = || Some(value.trim().to_string());
    let b = || match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(anyhow!("expected a boolean for '{}', got '{}'", key, other)),
    };
    let u = || {
        value
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("expected an integer for '{}', got '{}'", key, value))
    };
    match key {
        "signing.xcode_org_id" => cfg.signing.xcode_org_id = s(),
        "signing.xcode_signing_id" => cfg.signing.xcode_signing_id = s(),
        "signing.updated_wda_bundle_id" => cfg.signing.updated_wda_bundle_id = s(),
        "signing.allow_provisioning_updates" => cfg.signing.allow_provisioning_updates = Some(b()?),
        "signing.allow_provisioning_device_registration" => {
            cfg.signing.allow_provisioning_device_registration = Some(b()?)
        }
        "signing.show_xcode_log" => cfg.signing.show_xcode_log = Some(b()?),
        "session.create_timeout_ms" => cfg.session.create_timeout_ms = Some(u()?),
        "session.wda_launch_timeout_ms" => cfg.session.wda_launch_timeout_ms = Some(u()?),
        "session.wda_connection_timeout_ms" => cfg.session.wda_connection_timeout_ms = Some(u()?),
        "run.disconnect_on_finish" => cfg.run.disconnect_on_finish = Some(b()?),
        "run.fast" => cfg.run.fast = Some(b()?),
        other => bail!(
            "unknown config key '{}'. Known keys: signing.xcode_org_id, signing.xcode_signing_id, \
             signing.updated_wda_bundle_id, signing.allow_provisioning_updates, \
             signing.allow_provisioning_device_registration, signing.show_xcode_log, \
             session.create_timeout_ms, session.wda_launch_timeout_ms, \
             session.wda_connection_timeout_ms, run.disconnect_on_finish, run.fast",
            other
        ),
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
