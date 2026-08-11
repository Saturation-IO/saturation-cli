//! Machine-readable public API discovery for `saturation schema`.
//!
//! The resource surface is derived from the bundled OpenAPI 3.1
//!   (`openapi/openapi.yaml`). The operation inventory is extracted at build time
//!   by `build.rs` into `$OUT_DIR/v1_operations.json` and embedded here. No
//!   runtime YAML dependency, always in sync with the spec.

use anyhow::{Context, Result};

use crate::output::Output;

/// The `/v1` operation inventory, extracted from the OpenAPI by `build.rs`.
const V1_OPERATIONS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/v1_operations.json"));

/// Emit the bundled public API discovery document.
pub fn execute(output: &Output) -> Result<()> {
    let mut doc = serde_json::Map::new();
    doc.insert("api".into(), api_surface()?);
    output.print(&serde_json::Value::Object(doc))
}

/// Build the `/v1` surface from the embedded operation inventory.
fn api_surface() -> Result<serde_json::Value> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_inventory_lists_many_operations() {
        let surface = api_surface().unwrap();
        let count = surface
            .get("operationCount")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert!(count > 100, "expected >100 operations, got {count}");
    }

    #[test]
    fn inventory_includes_a_known_operation() {
        let surface = api_surface().unwrap();
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
