//! The `saturation agent` namespace is a thin client for the public MCP endpoint.
//! It shares the OAuth token and tool catalog used by Claude, OpenAI, and other
//! MCP clients.

pub mod cli;
pub mod client;

use anyhow::{Context, Result};
use std::io::{IsTerminal, Read};

use crate::output::Output;
use cli::AgentCommand;
use client::AgentClient;

pub use cli::AgentArgs;

pub async fn execute(args: AgentArgs, client: &AgentClient, output: &Output) -> Result<()> {
    match args.command {
        AgentCommand::Tools => {
            let tools = client.list_tools().await?;
            output.print(&tools)
        }
        AgentCommand::Call { tool, params, file } => {
            let body = resolve_params(params, file)?;
            let result = client.call_tool(&tool, &body).await?;
            output.print(&result)
        }
        AgentCommand::Query { dsl, file, stdin } => {
            let raw = resolve_input(dsl, file, stdin, "query params")?;
            let query: serde_json::Value =
                serde_json::from_str(&raw).context("invalid JSON query params")?;
            let result = client.call_tool("query", &query).await?;
            output.print(&result)
        }
        AgentCommand::Upload {
            source_url,
            project,
        } => {
            let mut params = serde_json::Map::new();
            params.insert("sourceUrl".into(), serde_json::Value::String(source_url));
            if let Some(p) = project {
                params.insert(
                    "assignTo".into(),
                    serde_json::json!({ "kind": "project", "id": p }),
                );
            }
            let result = client
                .call_tool("upload", &serde_json::Value::Object(params))
                .await?;
            output.print(&result)
        }
    }
}

/// Resolve `--params <json>` | `--file <path>` for `agent call`.
fn resolve_params(inline: Option<String>, file: Option<String>) -> Result<serde_json::Value> {
    let raw = match (inline, file) {
        (Some(s), _) => s,
        (None, Some(path)) => std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read params file: {path}"))?,
        (None, None) => "{}".to_string(),
    };
    serde_json::from_str(&raw).context("invalid JSON params")
}

/// Resolve inline | file | stdin input.
fn resolve_input(
    inline: Option<String>,
    file: Option<String>,
    stdin: bool,
    what: &str,
) -> Result<String> {
    if let Some(s) = inline {
        return Ok(s);
    }
    if let Some(path) = file {
        return std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {what} file: {path}"));
    }
    if stdin || !std::io::stdin().is_terminal() {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read from stdin")?;
        if !buf.trim().is_empty() {
            return Ok(buf);
        }
    }
    anyhow::bail!("no {what} provided. Pass inline JSON, --file <path>, or pipe via stdin.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_params_defaults_to_empty_object() {
        let v = resolve_params(None, None).unwrap();
        assert_eq!(v, serde_json::json!({}));
    }

    #[test]
    fn resolve_params_parses_inline() {
        let v = resolve_params(Some(r#"{"a":1}"#.into()), None).unwrap();
        assert_eq!(v, serde_json::json!({"a": 1}));
    }

    #[test]
    fn resolve_input_prefers_inline() {
        let s = resolve_input(Some("x".into()), Some("f".into()), true, "thing").unwrap();
        assert_eq!(s, "x");
    }
}
