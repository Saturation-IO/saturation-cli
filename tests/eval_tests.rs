//! Eval tests for machine operability.
//!
//! Verify that automation can discover and operate the public CLI through the
//! offline `schema` command and per-resource `--help`.
//!
//! Each test asserts on structure and content needed to automate the CLI.

use assert_cmd::Command;
use predicates::prelude::*;

fn saturation() -> Command {
    Command::cargo_bin("saturation").unwrap()
}

// ─── API discovery is valid, complete JSON ───────────────────────────────────

#[test]
fn eval_schema_is_valid_json() {
    let output = saturation().arg("schema").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("schema output must be valid JSON");
    assert!(parsed.get("api").is_some(), "schema must have an 'api' key");
    let api = &parsed["api"];
    assert!(
        api.get("operations").is_some(),
        "api must have 'operations'"
    );
    assert!(
        api.get("operationCount").is_some(),
        "api must have 'operationCount'"
    );
}

#[test]
fn eval_api_inventory_covers_every_resource_family() {
    let output = saturation().arg("schema").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let ops = parsed["api"]["operations"].as_array().unwrap();

    // Collect every operationId and templated path.
    let ids: Vec<&str> = ops
        .iter()
        .filter_map(|o| o.get("operationId").and_then(|v| v.as_str()))
        .collect();
    let paths: Vec<&str> = ops
        .iter()
        .filter_map(|o| o.get("path").and_then(|v| v.as_str()))
        .collect();

    // The inventory must be large (the /v1 surface is 100+ operations).
    assert!(
        ops.len() > 100,
        "expected >100 operations, got {}",
        ops.len()
    );

    // Spot-check one operation per resource family from the Appendix-A inventory.
    for needle in &[
        "budget_listLines",
        "transactions_",
        "purchase-orders_",
        "documents_",
        "webhooks_",
    ] {
        assert!(
            ids.iter()
                .any(|id| id.starts_with(needle) || id.contains(needle)),
            "inventory missing an operation matching `{needle}`"
        );
    }
    for path_needle in &[
        "/budget/lines",
        "/transactions",
        "/library/rate-packs",
        "/library/incentive-packs",
        "/search",
    ] {
        assert!(
            paths.iter().any(|p| p.contains(path_needle)),
            "inventory missing a path containing `{path_needle}`"
        );
    }
}

#[test]
fn eval_schema_is_deterministic() {
    let a = saturation().arg("schema").output().unwrap();
    let b = saturation().arg("schema").output().unwrap();
    assert_eq!(a.stdout, b.stdout, "schema output must be deterministic");
}

// ─── Every API resource group is a valid --help target ───────────────────────

#[test]
fn eval_all_api_resource_groups_have_help() {
    for resource in &[
        "budget",
        "transactions",
        "purchase-orders",
        "library",
        "documents",
        "search",
        "webhooks",
        "contacts",
        "projects",
        "spaces",
        "whoami",
    ] {
        saturation().args([resource, "--help"]).assert().success();
    }
}

#[test]
fn eval_documents_help_describes_upload_and_link_tasks() {
    saturation()
        .args(["documents", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upload"))
        .stdout(predicate::str::contains("link"))
        .stdout(predicate::str::contains("unlink"));
}

#[test]
fn eval_schema_contains_only_the_public_api_surface() {
    let output = saturation().arg("schema").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        parsed.get("api").is_some(),
        "schema must include the public API surface"
    );
    assert!(parsed.get("agent").is_none());
}
