use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::agent::AgentArgs;
use crate::v1::V1Args;

#[derive(Parser)]
#[command(
    name = "saturation",
    bin_name = "saturation",
    about = "CLI for the Saturation public API, production-finance data, and agent tools",
    long_about = "Saturation CLI provides programmatic access to your production-finance workspace.\n\n\
        Two namespaces share one API token and one config (~/.saturation/config.json):\n  \
        • the resource grammar (budget, transactions, library, documents, search, ...)\n    \
          is a typed client generated from the public OpenAPI 3.1 (`/v1`)\n  \
        • `agent` exposes the workspace tool registry for AI-agent workflows\n\n\
        Get started:\n  \
        saturation login                 # Sign in through Saturation OAuth\n  \
        saturation v1 projects list      # Start exploring\n  \
        saturation schema                # Machine-readable command + tool discovery",
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

    /// Manage personal API-token authentication.
    Auth(AuthArgs),

    /// Public API resource grammar (`/v1`) — budget, transactions, library, documents, search, webhooks, ...
    #[command(name = "v1", flatten_help = true)]
    V1(Box<V1Args>),

    /// Agent-runtime tools — discover and invoke the workspace tool registry.
    Agent(AgentArgs),

    /// Machine-readable discovery — `/v1` surface (OpenAPI) + agent tools (registry).
    Schema(SchemaArgs),
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

// ─── Auth ────────────────────────────────────────────────────────────────────

#[derive(clap::Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Save a token created under Settings > Developers > API.
    #[command(long_about = "Save a personal API token for authenticated commands.\n\
            Create the token under Settings > Developers > API. JWTs minted by\n\
            the sandbox are also accepted for local testing.\n\n\
            Example:\n  \
            saturation auth token \"$SATURATION_API_TOKEN\"")]
    Token {
        /// Personal API token or JWT access token.
        token: String,
        /// User email (extracted from JWT if omitted).
        #[arg(long)]
        email: Option<String>,
        /// User ID (extracted from JWT sub if omitted).
        #[arg(long)]
        user_id: Option<String>,
    },
    /// Clear stored credentials.
    Logout,
    /// Show current authentication status.
    Status,
}

// ─── Schema ──────────────────────────────────────────────────────────────────

#[derive(clap::Args)]
#[command(
    about = "Machine-readable discovery for agents",
    long_about = "Output structured discovery for both namespaces:\n  \
        • `/v1` — the resource surface, sourced from the bundled OpenAPI 3.1\n  \
        • `agent` — the public MCP tool catalog\n\n\
        Examples:\n  \
        saturation schema             # both surfaces\n  \
        saturation schema --v1        # just the /v1 resource list (offline)\n  \
        saturation schema --agent     # just the live agent tool registry"
)]
pub struct SchemaArgs {
    /// Emit only the `/v1` resource surface (from the bundled OpenAPI).
    #[arg(long)]
    pub v1: bool,
    /// Emit only the live MCP tool catalog.
    #[arg(long)]
    pub agent: bool,
}
