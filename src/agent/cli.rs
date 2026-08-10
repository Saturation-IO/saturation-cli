//! clap definitions for the `sat agent` namespace.

use clap::{Args, Subcommand};

#[derive(Args)]
#[command(
    about = "Discover and invoke the public Saturation MCP tools",
    long_about = "The agent surface uses the same public MCP endpoint and OAuth session as other MCP clients.\n\n\
        Discovery comes from tools/list, so `agent call` can invoke anything\n\
        `agent tools` advertises.\n\n\
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
    /// List the tools advertised by MCP (`tools/list`).
    Tools,

    /// Invoke any advertised MCP tool by name (`tools/call`).
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

    /// Upload a document from a public HTTPS URL through the MCP upload tool.
    Upload {
        /// Public HTTPS URL containing the document bytes.
        source_url: String,
        /// Assign to a project.
        #[arg(long)]
        project: Option<String>,
    },
}
