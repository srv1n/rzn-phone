const INPUT_GROUP_ORDER: [&str; 4] = ["core", "safety", "advanced", "internal"];

#[derive(Parser)]
#[command(name = "rzn-phone", disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand)]
enum CommandKind {
    Worker,
    Doctor(JsonOutputArgs),
    Devices(JsonOutputArgs),
    Status,
    Shutdown(ShutdownArgs),
    Version,
    Info,
    Run(RunArgs),
    List(ListArgs),
    Show(ShowArgs),
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    Tool {
        #[command(subcommand)]
        command: ToolCommand,
    },
    Tools(ToolsAliasArgs),
    Recent(RecentArgs),
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    Rerun {
        index: usize,
    },
    Favorite {
        #[command(subcommand)]
        command: FavoriteCommand,
    },
    Favorites(JsonOutputArgs),
    Completion(CompletionArgs),
    #[command(alias = "skills")]
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    Workflows {
        #[command(subcommand)]
        command: WorkflowsCommand,
    },
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    Workflow {
        #[command(subcommand)]
        command: WorkflowAliasCommand,
    },
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
    system_or_query: Option<String>,
    #[arg(long)]
    family: Option<String>,
    #[arg(long)]
    search: Option<String>,
    #[arg(long)]
    surface: Option<String>,
    #[arg(long = "has-input")]
    has_input: Option<String>,
    #[arg(long, default_missing_value = "true", num_args = 0..=1)]
    mutating: Option<bool>,
    #[arg(long)]
    favorites: bool,
    #[arg(long)]
    compact: bool,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args, Clone)]
struct ShowArgs {
    first: String,
    second: Option<String>,
    #[arg(long)]
    example: bool,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
struct RunArgs {
    first: String,
    second: Option<String>,
    #[arg(long)]
    udid: Option<String>,
    #[arg(long = "args-json", default_value = "{}")]
    args_json: String,
    #[arg(long, default_value_t = false)]
    commit: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,
    #[arg(long = "disconnect-on-finish", default_value_t = true)]
    disconnect_on_finish: bool,
    #[arg(long = "stop-appium-on-finish", default_value_t = false)]
    stop_appium_on_finish: bool,
    #[arg(long = "background-on-exit", default_value_t = false)]
    background_on_exit: bool,
    #[arg(long = "lock-device-on-exit", default_value_t = false)]
    lock_device_on_exit: bool,
    #[arg(long = "fast", default_missing_value = "true", num_args = 0..=1)]
    fast: Option<bool>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ToolListArgs {
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

#[derive(Args)]
struct ToolShowArgs {
    name: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
struct ToolCallArgs {
    name: String,
    #[arg(long = "args-json", default_value = "{}")]
    args_json: String,
}

#[derive(Args)]
struct RecentArgs {
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long)]
    json: bool,
    #[arg(long)]
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
    shell: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SkillScope {
    Global,
    Project,
}

#[derive(Args, Clone)]
struct SkillInstallArgs {
    #[arg(long, default_value = "rzn-phone-automation")]
    skill: String,
    #[arg(long, value_enum, default_value = "project")]
    scope: SkillScope,
    #[arg(long = "project-dir")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all")]
    clients: String,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillRemoveArgs {
    #[arg(long, default_value = "rzn-phone-automation")]
    skill: String,
    #[arg(long, value_enum, default_value = "project")]
    scope: SkillScope,
    #[arg(long = "project-dir")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all")]
    clients: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillStatusArgs {
    #[arg(long, default_value = "rzn-phone-automation")]
    skill: String,
    #[arg(long, value_enum, default_value = "project")]
    scope: SkillScope,
    #[arg(long = "project-dir")]
    project_dir: Option<PathBuf>,
    #[arg(long, default_value = "all")]
    clients: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args, Clone)]
struct SkillListArgs {
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
struct ShutdownArgs {
    #[arg(long = "stop-appium", default_value_t = true)]
    stop_appium: bool,
    #[arg(long = "background-on-exit", default_value_t = false)]
    background_on_exit: bool,
    #[arg(long = "lock-device-on-exit", default_value_t = false)]
    lock_device_on_exit: bool,
}

#[derive(Args)]
struct WorkflowsUpdateArgs {
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    version: Option<String>,
}
