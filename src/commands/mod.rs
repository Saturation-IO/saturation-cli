pub mod auth;

use std::path::Path;

use anyhow::{Context, Result};

use crate::agent::client::AgentClient;
use crate::cli::{Cli, Command};
use crate::config::{Config, TokenInfo};
use crate::output::Output;
use crate::schema;
use crate::v1;

pub async fn execute(cli: Cli) -> Result<()> {
    let output = Output::new(cli.format.clone(), cli.quiet);

    match cli.command {
        // ── Auth / schema: no live API client required ────────────────────────
        Command::Login(args) => auth::login(args, &output).await,
        Command::Logout => auth::logout(&output),
        Command::Auth(args) => auth::execute(args, &output).await,
        Command::Schema(args) => schema::execute(args, &output).await,

        // ── /v1 namespace: typed client generated from the OpenAPI ─────────────
        Command::V1(args) => {
            let mut config = load_config_with_overrides(cli.token_file.as_deref())?;
            // Transparently refresh an expired (refreshable) token before the call.
            auth::ensure_fresh_token(&mut config).await?;
            let token = config.require_token()?;
            let base_url = cli
                .api_base_url
                .as_deref()
                .unwrap_or(config.v1_server_url());
            let client = v1::Client::new(base_url, token.access_token.clone());
            v1::commands::execute(*args, &client, &output).await
        }

        // ── agent namespace: registry-driven tool dispatch ─────────────────────
        Command::Agent(args) => {
            let mut config = load_config_with_overrides(cli.token_file.as_deref())?;
            auth::ensure_fresh_token(&mut config).await?;
            let workspace_override = args.workspace.clone();
            let client = AgentClient::from_config(&config, workspace_override.as_deref())?;
            crate::agent::execute(args, &client, &output).await
        }
    }
}

/// Load config, then apply the `--token-file` injection.
///
/// `--token-file` is the headless-auth path: read a bearer token from disk on
/// every invocation (so a desktop-rotated short-TTL token stays fresh without a
/// respawn), and store it as a non-refreshable `TokenInfo`.
fn load_config_with_overrides(token_file: Option<&Path>) -> Result<Config> {
    let mut config = Config::load()?;
    if let Some(path) = token_file {
        let token = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read token file: {}", path.display()))?
            .trim()
            .to_string();
        config.token = Some(TokenInfo {
            access_token: token,
            refresh_token: String::new(),
            expires_at: String::new(),
        });
    }
    Ok(config)
}
