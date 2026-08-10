//! clap definitions for the `sat agent` namespace.

use clap::{Args, Subcommand};

#[derive(Args)]
#[command(
    about = "Agent-runtime tools — discover and invoke the workspace tool registry",
    long_about = "The agent surface dispatches through the server's consolidated tool registry:\n  \
        GET  /api/cli/:ws/tools           — discover available tools\n  \
        POST /api/cli/:ws/tool/:toolName  — invoke any tool\n\n\
        Discovery is registry-driven (not a hand-maintained list), so `agent call`\n\
        can invoke anything `agent tools` advertises.\n\n\
        Examples:\n  \
        saturation agent tools\n  \
        saturation agent call query --params '{\"domain\":\"workspace\",\"sql\":\"SELECT id, name FROM projects LIMIT 10\"}'\n  \
        saturation agent query '{\"domain\":\"workspace\",\"sql\":\"SELECT id, name FROM projects LIMIT 10\"}'"
)]
pub struct AgentArgs {
    /// Override the active agent workspace for this agent command.
    #[arg(long, global = true, env = "SATURATION_WORKSPACE_ID")]
    pub workspace: Option<String>,

    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Subcommand)]
pub enum AgentCommand {
    /// List the tools advertised by the workspace registry (`GET /tools`).
    Tools,

    /// Invoke any registered tool by name (`POST /tool/:toolName`).
    Call {
        /// Tool name (as listed by `agent tools`).
        tool: String,
        /// Tool params as inline JSON.
        #[arg(long)]
        params: Option<String>,
        /// Read tool params from a JSON file.
        #[arg(long)]
        file: Option<String>,
    },

    /// Run a v3 workspace query (shortcut for `call query`).
    Query {
        /// Query params as inline JSON.
        dsl: Option<String>,
        #[arg(short, long)]
        file: Option<String>,
        #[arg(long)]
        stdin: bool,
    },

    /// Upload a document via the agent upload endpoint.
    Upload {
        /// File path to upload.
        file: String,
        /// Assign to a project.
        #[arg(long)]
        project: Option<String>,
        /// Document classification.
        #[arg(long)]
        classification: Option<String>,
    },
}
