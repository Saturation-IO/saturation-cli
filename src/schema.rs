//! `saturation schema` — machine-readable discovery for agents, two-sourced:
//!
//! - the `/v1` resource surface, derived from the bundled OpenAPI 3.1
//!   (`openapi/openapi.yaml`). The operation inventory is extracted at build time
//!   by `build.rs` into `$OUT_DIR/v1_operations.json` and embedded here — no
//!   runtime YAML dependency, always in sync with the spec.
//! - the `agent` tool registry, proxied live from `GET /api/cli/:ws/tools`.
//!
//! This replaces the old 855-line hand-built command tree: the `/v1` half stays
//! in sync with the OpenAPI, and the agent half stays in sync with the server's
//! `TOOL_REGISTRY` — neither is hand-maintained.

use anyhow::{Context, Result};

use crate::agent::client::AgentClient;
use crate::cli::SchemaArgs;
use crate::config::Config;
use crate::output::Output;

/// The `/v1` operation inventory, extracted from the OpenAPI by `build.rs`.
const V1_OPERATIONS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/v1_operations.json"));

/// Emit the discovery document. With neither `--v1` nor `--agent`, both surfaces
/// are emitted (the agent half only if an auth context is available).
pub async fn execute(args: SchemaArgs, output: &Output) -> Result<()> {
    let want_v1 = args.v1 || !args.agent;
    let want_agent = args.agent || !args.v1;

    let mut doc = serde_json::Map::new();

    if want_v1 {
        doc.insert("v1".into(), v1_surface()?);
    }
    if want_agent {
        match agent_surface().await {
            Ok(tools) => {
                doc.insert("agent".into(), tools);
            }
            Err(e) => {
                // Don't fail the whole command if the agent registry is
                // unreachable (e.g. not logged in) — surface the reason instead.
                doc.insert(
                    "agent".into(),
                    serde_json::json!({ "unavailable": e.to_string() }),
                );
            }
        }
    }

    output.print(&serde_json::Value::Object(doc))
}

/// Build the `/v1` surface from the embedded operation inventory.
fn v1_surface() -> Result<serde_json::Value> {
    let operations: serde_json::Value = serde_json::from_str(V1_OPERATIONS_JSON)
        .context("failed to parse embedded /v1 operations")?;
    let count = operations.as_array().map(|a| a.len()).unwrap_or(0);
    Ok(serde_json::json!({
        "source": "bundled OpenAPI 3.1 (openapi/openapi.yaml)",
        "title": "Saturation API",
        "operationCount": count,
        "operations": operations,
    }))
}

/// Proxy the live agent tool registry (`GET /tools`).
async fn agent_surface() -> Result<serde_json::Value> {
    let config = Config::load()?;
    let client = AgentClient::from_config(&config, None)?;
    let tools = client.list_tools().await?;
    Ok(serde_json::json!({
        "source": "GET /api/cli/:ws/tools (live registry)",
        "toolCount": tools.len(),
        "tools": tools,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_inventory_lists_many_operations() {
        let surface = v1_surface().unwrap();
        let count = surface
            .get("operationCount")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert!(count > 100, "expected >100 operations, got {count}");
    }

    #[test]
    fn inventory_includes_a_known_operation() {
        let surface = v1_surface().unwrap();
        let ops = surface
            .get("operations")
            .and_then(|v| v.as_array())
            .unwrap();
        let has = ops
            .iter()
            .any(|o| o.get("operationId").and_then(|v| v.as_str()) == Some("budget_listLines"));
        assert!(has, "expected budget_listLines in the embedded inventory");
    }
}
