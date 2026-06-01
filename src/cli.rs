use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Parser, Subcommand};

use crate::client::ApiClient;
use crate::config::{template_config, RunConfig, RunConfigInput};
use crate::mock::{run_mock_server, MockMode};
use crate::report::{render_report, ReporterKind};
use crate::testsuite::{build_report, registry_for_profiles, Runner};
use crate::types::{Profile, ReportFormat};
use crate::util::fs::write_text;

#[derive(Debug, Parser)]
#[command(name = "octest")]
#[command(version)]
#[command(about = "Test OpenAI-compatible API compatibility")]
pub struct Cli {
    #[arg(long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub debug: bool,

    #[arg(long, global = true)]
    pub trace: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Run(RunArgs),
    Quick(QuickArgs),
    Models(EndpointArgs),
    Chat(ChatArgs),
    Stream(EndpointArgs),
    Tools(EndpointArgs),
    Schema(EndpointArgs),
    Embeddings(EndpointArgs),
    Report(ReportArgs),
    Init(InitArgs),
    MockServer(MockServerArgs),
}

#[derive(Debug, Args, Clone)]
pub struct RunArgs {
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub base_url: Option<String>,

    #[arg(long)]
    pub api_key: Option<String>,

    #[arg(long)]
    pub api_key_env: Option<String>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub embedding_model: Option<String>,

    #[arg(long, value_enum)]
    pub profile: Vec<Profile>,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long, value_enum)]
    pub format: Option<ReportFormat>,

    #[arg(long)]
    pub timeout_ms: Option<u64>,

    #[arg(long)]
    pub stream_timeout_ms: Option<u64>,

    #[arg(long)]
    pub no_auth: bool,

    #[arg(long)]
    pub costly: bool,

    #[arg(long)]
    pub destructive: bool,

    #[arg(long)]
    pub strict: bool,

    #[arg(long)]
    pub no_redact: bool,

    #[arg(long)]
    pub min_score: Option<u32>,

    #[arg(long)]
    pub max_latency_ms: Option<u64>,

    #[arg(long)]
    pub max_first_token_ms: Option<u64>,

    #[arg(long)]
    pub concurrency: Option<usize>,
}

#[derive(Debug, Args, Clone)]
pub struct QuickArgs {
    #[command(flatten)]
    pub run: RunArgs,
}

#[derive(Debug, Args, Clone)]
pub struct EndpointArgs {
    #[command(flatten)]
    pub run: RunArgs,
}

#[derive(Debug, Args, Clone)]
pub struct ChatArgs {
    #[command(flatten)]
    pub run: RunArgs,

    #[arg(long, default_value = "Reply with exactly: pong")]
    pub message: String,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    pub report: PathBuf,

    #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
    pub format: ReportFormat,

    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct MockServerArgs {
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    #[arg(long, value_enum, default_value_t = MockMode::Compatible)]
    pub mode: MockMode,
}

pub async fn run() -> anyhow::Result<i32> {
    let cli = Cli::parse();
    init_tracing(&cli);

    match cli.command {
        Commands::Run(args) => execute_run(args, Profile::Core, None, true).await,
        Commands::Quick(args) => {
            execute_run(args.run, Profile::Core, Some(QuickSelection::Quick), true).await
        }
        Commands::Models(args) => {
            execute_run(args.run, Profile::Core, Some(QuickSelection::Models), false).await
        }
        Commands::Chat(args) => {
            execute_run(
                args.run,
                Profile::Core,
                Some(QuickSelection::ManualChat(args.message)),
                true,
            )
            .await
        }
        Commands::Stream(args) => {
            execute_run(args.run, Profile::Core, Some(QuickSelection::Stream), true).await
        }
        Commands::Tools(args) => {
            execute_run(args.run, Profile::Agent, Some(QuickSelection::Tools), true).await
        }
        Commands::Schema(args) => {
            execute_run(args.run, Profile::Agent, Some(QuickSelection::Schema), true).await
        }
        Commands::Embeddings(args) => {
            execute_run(
                args.run,
                Profile::Data,
                Some(QuickSelection::Embeddings),
                false,
            )
            .await
        }
        Commands::Report(args) => render_saved_report(args),
        Commands::Init(args) => {
            write_text(&args.path, template_config())?;
            println!("Wrote config template to {}", args.path.display());
            Ok(0)
        }
        Commands::MockServer(args) => run_mock_server(args.port, args.mode).await,
    }
}

#[derive(Debug, Clone)]
enum QuickSelection {
    Quick,
    Models,
    ManualChat(String),
    Stream,
    Tools,
    Schema,
    Embeddings,
}

async fn execute_run(
    args: RunArgs,
    default_profile: Profile,
    selection: Option<QuickSelection>,
    require_chat_model: bool,
) -> anyhow::Result<i32> {
    let config = match RunConfig::load(args_to_input(args), default_profile, require_chat_model) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Config error: {err:#}");
            return Ok(2);
        }
    };

    let client = match ApiClient::new(&config) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Client error: {err:#}");
            return Ok(2);
        }
    };
    tracing::debug!(
        provider = ?config.name,
        api_key_env = %config.api_key_env,
        concurrency = config.concurrency,
        "run configuration loaded"
    );

    let tests = match selection {
        Some(QuickSelection::Quick) => registry_for_profiles(&[Profile::Core], Some("quick")),
        Some(QuickSelection::Models) => registry_for_profiles(&[Profile::Core], Some("models")),
        Some(QuickSelection::ManualChat(message)) => {
            registry_for_profiles(&[Profile::Core], Some(&format!("manual_chat:{message}")))
        }
        Some(QuickSelection::Stream) => registry_for_profiles(&[Profile::Core], Some("stream")),
        Some(QuickSelection::Tools) => registry_for_profiles(&[Profile::Agent], Some("tools")),
        Some(QuickSelection::Schema) => registry_for_profiles(&[Profile::Agent], Some("schema")),
        Some(QuickSelection::Embeddings) => {
            registry_for_profiles(&[Profile::Data], Some("embeddings"))
        }
        None => registry_for_profiles(&config.profiles, None),
    };

    let runner = Runner::new(config.clone(), client);
    let run_output = runner.run(tests).await;
    let report = build_report(&config, run_output);

    let rendered = render_report(ReporterKind::from(config.output.format), &report)?;
    if let Some(path) = &config.output.path {
        let file_content = if matches!(config.output.format, ReportFormat::Terminal) {
            render_report(ReporterKind::Json, &report)?
        } else {
            rendered.clone()
        };
        write_text(path, &file_content)?;
        if matches!(config.output.format, ReportFormat::Terminal) {
            println!("{rendered}");
            println!("Report saved to: {}", path.display());
        }
    } else {
        println!("{rendered}");
    }

    Ok(exit_code_for(&config, &report))
}

fn args_to_input(args: RunArgs) -> RunConfigInput {
    RunConfigInput {
        config_path: args.config,
        base_url: args.base_url,
        api_key: args.api_key,
        api_key_env: args.api_key_env,
        model: args.model,
        embedding_model: args.embedding_model,
        profiles: args.profile,
        output_path: args.output,
        output_format: args.format,
        timeout_ms: args.timeout_ms,
        stream_timeout_ms: args.stream_timeout_ms,
        no_auth: args.no_auth,
        costly: args.costly,
        destructive: args.destructive,
        strict: args.strict,
        redact: Some(!args.no_redact),
        min_score: args.min_score,
        max_latency_ms: args.max_latency_ms,
        max_first_token_ms: args.max_first_token_ms,
        concurrency: args.concurrency,
    }
}

fn render_saved_report(args: ReportArgs) -> anyhow::Result<i32> {
    let text = std::fs::read_to_string(&args.report)
        .with_context(|| format!("failed to read {}", args.report.display()))?;
    let report: crate::types::CompatibilityReport = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", args.report.display()))?;
    let rendered = render_report(ReporterKind::from(args.format), &report)?;
    if let Some(path) = args.output {
        write_text(&path, &rendered)?;
    } else {
        println!("{rendered}");
    }
    Ok(0)
}

fn exit_code_for(config: &RunConfig, report: &crate::types::CompatibilityReport) -> i32 {
    if let Some(min_score) = config.thresholds.min_score {
        if report.score.overall < min_score {
            return 6;
        }
    }

    if report
        .tests
        .iter()
        .any(|test| test.status.is_failure() && test.max_score > 0)
    {
        return 1;
    }

    0
}

fn init_tracing(cli: &Cli) {
    let level = if cli.trace {
        "trace"
    } else if cli.debug {
        "debug"
    } else if cli.verbose {
        "info"
    } else {
        "warn"
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(level)
        .with_target(false)
        .try_init();
}
