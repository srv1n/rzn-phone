const INPUT_GROUP_ORDER: [&str; 4] = ["core", "safety", "advanced", "internal"];

fn parse_bool_flag(value: &str) -> std::result::Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Ok(false),
        _ => Err(format!("expected one of 0, 1, true, or false; got '{value}'")),
    }
}

#[derive(Parser)]
#[command(name = "rzn-phone", disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand)]
enum CommandKind {
    /// Run the MCP worker over stdio.
    Worker,
    /// Check local iOS automation prerequisites.
    Doctor(JsonOutputArgs),
    /// List connected physical iPhones.
    Devices(JsonOutputArgs),
    /// Show cached runtime/session status.
    Status,
    /// Stop Appium, WDA, or the active app/session.
    Shutdown(ShutdownArgs),
    /// Show runtime and workflow-pack versions.
    Version,
    /// Show installed runtime paths and update source.
    Info,
    /// Run a packaged workflow on a connected iPhone.
    Run(RunArgs),
    /// List packaged workflows.
    List(ListArgs),
    /// Show a workflow or direct tool.
    Show(ShowArgs),
    /// Inspect capability families for agent planning.
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    /// Inspect or call direct MCP tools.
    Tool {
        #[command(subcommand)]
        command: ToolCommand,
    },
    /// Alias for `tool list`.
    Tools(ToolsAliasArgs),
    /// Show recent workflow runs.
    Recent(RecentArgs),
    /// Manage local run history.
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Rerun a recent workflow entry by number.
    Rerun {
        /// One-based index from `rzn-phone recent`.
        index: usize,
    },
    /// Manage favorite workflows.
    Favorite {
        #[command(subcommand)]
        command: FavoriteCommand,
    },
    /// List favorite workflows.
    Favorites(JsonOutputArgs),
    /// Print shell completion script.
    Completion(CompletionArgs),
    #[command(alias = "skills")]
    /// Install or manage bundled agent skills.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Manage installed workflow packs.
    Workflows {
        #[command(subcommand)]
        command: WorkflowsCommand,
    },
    /// Draft or inspect workflow failure reports.
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    /// Workflow command aliases.
    Workflow {
        #[command(subcommand)]
        command: WorkflowAliasCommand,
    },
    /// Show paths to installed examples.
    Examples {
        #[command(subcommand)]
        command: ExamplesCommand,
    },
    #[command(hide = true, name = "__complete-values")]
    CompleteValues {
        entity: String,
    },
}

#[derive(Subcommand)]
enum CapabilityCommand {
    List(JsonOutputArgs),
}

#[derive(Subcommand)]
enum ToolCommand {
    List(ToolListArgs),
    Show(ToolShowArgs),
    Call(ToolCallArgs),
}

#[derive(Args)]
struct ToolsAliasArgs {
    #[arg(long)]
    direct: bool,
    #[arg(long)]
    search: Option<String>,
    #[arg(long)]
    family: Option<String>,
    #[arg(long)]
    tier: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Subcommand)]
enum FavoriteCommand {
    Add { reference: String },
    Remove { reference: String },
    List(JsonOutputArgs),
}

#[derive(Subcommand)]
enum WorkflowAliasCommand {
    List(ListArgs),
    Show(ShowArgs),
}

#[derive(Subcommand)]
enum WorkflowsCommand {
    Update(WorkflowsUpdateArgs),
    Path,
}

#[derive(Subcommand)]
enum SkillCommand {
    Install(SkillInstallArgs),
    Update(SkillInstallArgs),
    Remove(SkillRemoveArgs),
    Status(SkillStatusArgs),
    List(SkillListArgs),
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum ReportCommand {
    #[command(name = "workflow-broken")]
    WorkflowBroken(WorkflowBrokenReportArgs),
    Queue(ReportQueueArgs),
}

#[derive(Args)]
struct WorkflowBrokenReportArgs {
    #[arg(long)]
    surface: Option<String>,
    #[arg(long)]
    flow: Option<String>,
    #[arg(long = "flow-version")]
    flow_version: Option<String>,
    #[arg(long = "failed-stage")]
    failed_stage: Option<String>,
    #[arg(long)]
    system: Option<String>,
    #[arg(long)]
    workflow: Option<String>,
    #[arg(long = "version")]
    workflow_version: Option<String>,
    #[arg(long = "step")]
    failed_step: Option<String>,
    #[arg(long)]
    error: String,
    #[arg(long = "app-version")]
    app_version: String,
    #[arg(long)]
    platform: String,
    #[arg(long)]
    note: Option<String>,
    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,
}

#[derive(Args)]
struct ReportQueueArgs {
    #[arg(long, default_value = "list")]
    action: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Subcommand)]
enum ExamplesCommand {
    Path,
}

#[derive(Args, Clone)]
struct JsonOutputArgs {
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args, Clone)]
struct ListArgs {
    /// Optional exact system id, workflow ref, or search text.
    system_or_query: Option<String>,
    #[arg(long, help = "Filter workflows by planning family.")]
    family: Option<String>,
    #[arg(long, help = "Search workflow names, descriptions, notes, and inputs.")]
    search: Option<String>,
    #[arg(long, help = "Filter workflows by app/system surface.")]
    surface: Option<String>,
    #[arg(long = "has-input", help = "Show workflows that expose this input name.")]
    has_input: Option<String>,
    #[arg(
        long,
        default_missing_value = "true",
        num_args = 0..=1,
        help = "Filter mutating workflows; accepts bare flag, 0/1, true/false."
    )]
    mutating: Option<bool>,
    #[arg(long, help = "Show only favorite workflows.")]
    favorites: bool,
    #[arg(long, help = "Use compact one-line output.")]
    compact: bool,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args, Clone)]
struct ShowArgs {
    /// Workflow ref, system id, or direct tool name.
    first: String,
    /// Optional workflow name when using `system workflow`.
    second: Option<String>,
    #[arg(long, help = "Print the full example set for workflows.")]
    example: bool,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args)]
struct RunArgs {
    /// Workflow ref or system id.
    first: String,
    /// Optional workflow name when using `system workflow`.
    second: Option<String>,
    #[arg(long, help = "Physical device UDID. Auto-selected when exactly one device is connected.")]
    udid: Option<String>,
    #[arg(long = "args-json", default_value = "{}", help = "Workflow arguments as JSON or @file.")]
    args_json: String,
    #[arg(
        long,
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Allow commit-gated workflow steps; accepts bare flag, 0/1, true/false."
    )]
    commit: bool,
    #[arg(long = "dry-run", default_value_t = false, help = "Force commit=false for this run.")]
    dry_run: bool,
    #[arg(
        long = "disconnect-on-finish",
        action = clap::ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Disconnect the WebDriver session after the workflow; accepts 0/1 or true/false."
    )]
    disconnect_on_finish: bool,
    #[arg(
        long = "stop-appium-on-finish",
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Stop Appium after the workflow; accepts 0/1 or true/false."
    )]
    stop_appium_on_finish: bool,
    #[arg(
        long = "background-on-exit",
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Background the app after the workflow; accepts 0/1 or true/false."
    )]
    background_on_exit: bool,
    #[arg(
        long = "lock-device-on-exit",
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Lock the device after the workflow; accepts 0/1 or true/false."
    )]
    lock_device_on_exit: bool,
    #[arg(
        long = "fast",
        action = clap::ArgAction::Set,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Enable smart runtime reuse for this run; accepts bare flag, 0/1, true/false."
    )]
    fast: Option<bool>,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
}

#[derive(Args)]
struct ToolListArgs {
    #[arg(long, help = "Hide workflow/script wrapper tools and show direct device tools.")]
    direct: bool,
    #[arg(long, help = "Search tool names, descriptions, families, and inputs.")]
    search: Option<String>,
    #[arg(long, help = "Filter tools by capability family.")]
    family: Option<String>,
    #[arg(long, help = "Filter tools by capability tier.")]
    tier: Option<String>,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args)]
struct ToolShowArgs {
    /// Direct tool name, such as ios.ui.observe_compact.
    name: String,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args)]
struct ToolCallArgs {
    /// Direct tool name to call.
    name: String,
    #[arg(long = "args-json", default_value = "{}", help = "Tool arguments as JSON or @file.")]
    args_json: String,
}

#[derive(Args)]
struct RecentArgs {
    #[arg(long, default_value_t = 10, help = "Maximum recent entries to show.")]
    limit: usize,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Subcommand)]
enum HistoryCommand {
    Clear,
    Redact,
    Path,
}

#[derive(Args)]
struct CompletionArgs {
    /// Shell name: bash or zsh.
    shell: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SkillScope {
    Global,
    Project,
}

#[derive(Args, Clone)]
struct SkillInstallArgs {
    #[arg(long, default_value = "rzn-phone-automation", help = "Bundled skill id to install.")]
    skill: String,
    #[arg(long, value_enum, default_value = "project", help = "Install globally or into a project.")]
    scope: SkillScope,
    #[arg(long = "project-dir", help = "Project directory for project-scoped installs.")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all", help = "Comma-separated clients: codex, claude, or all.")]
    clients: String,
    #[arg(long, help = "Overwrite existing links/files.")]
    force: bool,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillRemoveArgs {
    #[arg(long, default_value = "rzn-phone-automation", help = "Bundled skill id to remove.")]
    skill: String,
    #[arg(long, value_enum, default_value = "project", help = "Remove global or project-scoped links.")]
    scope: SkillScope,
    #[arg(long = "project-dir", help = "Project directory for project-scoped removes.")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all", help = "Comma-separated clients: codex, claude, or all.")]
    clients: String,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillStatusArgs {
    #[arg(long, default_value = "rzn-phone-automation", help = "Bundled skill id to inspect.")]
    skill: String,
    #[arg(long, value_enum, default_value = "project", help = "Inspect global or project-scoped links.")]
    scope: SkillScope,
    #[arg(long = "project-dir", help = "Project directory for project-scoped status.")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all", help = "Comma-separated clients: codex, claude, or all.")]
    clients: String,
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillListArgs {
    #[arg(long, help = "Emit raw JSON.")]
    json: bool,
    #[arg(long, help = "Force rich terminal rendering.")]
    pretty: bool,
}

#[derive(Args)]
struct ShutdownArgs {
    #[arg(
        long = "stop-appium",
        action = clap::ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Stop Appium while shutting down; accepts 0/1 or true/false."
    )]
    stop_appium: bool,
    #[arg(
        long = "background-on-exit",
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Background the current app during shutdown; accepts 0/1 or true/false."
    )]
    background_on_exit: bool,
    #[arg(
        long = "lock-device-on-exit",
        action = clap::ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = parse_bool_flag,
        help = "Lock the device during shutdown; accepts 0/1 or true/false."
    )]
    lock_device_on_exit: bool,
}

#[derive(Args)]
struct WorkflowsUpdateArgs {
    #[arg(long, help = "Workflow-pack source directory, file:// URL, or HTTPS base URL.")]
    source: Option<String>,
    #[arg(long, help = "Workflow-pack version to fetch from the source.")]
    version: Option<String>,
}

#[cfg(test)]
mod cli_arg_tests {
    use super::*;

    fn parse_run_args(argv: &[&str]) -> RunArgs {
        match Cli::try_parse_from(argv).expect("parse cli").command {
            CommandKind::Run(args) => args,
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_boolean_flags_accept_bare_and_explicit_values() {
        let args = parse_run_args(&[
            "rzn-phone",
            "run",
            "safari",
            "google_search",
            "--commit",
            "--disconnect-on-finish",
            "0",
            "--stop-appium-on-finish=true",
            "--background-on-exit",
            "1",
            "--lock-device-on-exit=false",
            "--fast",
            "0",
        ]);

        assert!(args.commit);
        assert!(!args.disconnect_on_finish);
        assert!(args.stop_appium_on_finish);
        assert!(args.background_on_exit);
        assert!(!args.lock_device_on_exit);
        assert_eq!(args.fast, Some(false));
    }

    #[test]
    fn run_boolean_flags_accept_documented_zero_one_examples() {
        let args = parse_run_args(&[
            "rzn-phone",
            "run",
            "reddit/comment_post",
            "--args-json",
            "{\"execute_comment\":false}",
            "--commit",
            "0",
            "--disconnect-on-finish",
            "0",
            "--fast",
            "1",
        ]);

        assert!(!args.commit);
        assert!(!args.dry_run);
        assert!(!args.disconnect_on_finish);
        assert_eq!(args.fast, Some(true));
    }

    #[test]
    fn dry_run_remains_safe_commit_alias() {
        let args = parse_run_args(&[
            "rzn-phone",
            "run",
            "linkedin/create_post",
            "--commit=1",
            "--dry-run",
        ]);

        assert!(args.commit);
        assert!(args.dry_run);
    }

    #[test]
    fn shutdown_boolean_flags_can_be_disabled() {
        match Cli::try_parse_from([
            "rzn-phone",
            "shutdown",
            "--stop-appium",
            "0",
            "--background-on-exit=1",
            "--lock-device-on-exit=false",
        ])
        .expect("parse cli")
        .command
        {
            CommandKind::Shutdown(args) => {
                assert!(!args.stop_appium);
                assert!(args.background_on_exit);
                assert!(!args.lock_device_on_exit);
            }
            _ => panic!("expected shutdown command"),
        }
    }
}
