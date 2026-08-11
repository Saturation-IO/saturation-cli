use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::v1::V1Command;

#[derive(Parser)]
#[command(
    name = "saturation",
    bin_name = "saturation",
    about = "CLI for the Saturation public API and production-finance data",
    long_about = "Saturation CLI provides programmatic access to your production-finance workspace.\n\n\
        Commands map to the public Saturation API. API versioning and transport are handled\n  \
        by the CLI, so commands stay focused on the work you want to do.\n\n\
        Get started:\n  \
        saturation login                 # Sign in through Saturation OAuth\n  \
        saturation projects list         # Start exploring\n  \
        saturation search \"camera rental\" # Search your workspace\n  \
        saturation schema                # Machine-readable API discovery",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output format
    #[arg(long, global = true, default_value = "json")]
    pub format: OutputFormat,

    /// Suppress headers and metadata, emit only data
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Override the canonical public API base URL.
    #[arg(long = "api-base-url", global = true, env = "SATURATION_API_BASE_URL")]
    pub api_base_url: Option<String>,

    /// Path to a file holding a bearer token, re-read on every invocation.
    /// Bypasses the stored config token for headless and sandboxed callers.
    #[arg(long = "token-file", global = true, env = "SATURATION_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,

    /// Project slug or id for project-scoped resources.
    #[arg(long, global = true)]
    pub project: Option<String>,

    /// Idempotency-Key required by create operations and optional elsewhere.
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    Json,
    Table,
    Csv,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sign in through Saturation in your browser.
    Login(LoginArgs),

    /// Clear stored credentials.
    Logout,

    /// Machine-readable discovery from the bundled public OpenAPI contract.
    Schema,

    /// Public API resource commands.
    #[command(flatten)]
    Api(Box<V1Command>),
}

#[derive(clap::Args)]
pub struct LoginArgs {
    /// OAuth authority. Override only for local testing.
    #[arg(
        long,
        env = "SATURATION_OAUTH_ISSUER",
        default_value = "https://connect.saturation.io",
        hide = true
    )]
    pub issuer: String,

    /// Print the sign-in URL instead of opening a browser.
    #[arg(long)]
    pub no_browser: bool,
}
