mod agent;
mod cli;
mod commands;
mod config;
mod output;
mod schema;
mod v1;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    commands::execute(cli).await
}
