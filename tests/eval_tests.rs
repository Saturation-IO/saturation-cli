//! Eval tests for agent operability.
//!
//! Verify that an LLM agent can discover and operate the CLI through the
//! two-sourced `schema` command and `--help`:
//!
//! 1. `saturation schema --v1` → the `/v1` operation inventory (offline, from
//!    the bundled OpenAPI 3.1).
//! 2. `saturation agent tools` → the live tool registry (`GET /tools`).
//! 3. `saturation <namespace> <resource> --help` → per-command usage.
//!
//! Each test asserts on structure/content an agent needs to operate the CLI.

use assert_cmd::Command;
use predicates::prelude::*;

fn saturation() -> Command {
    Command::cargo_bin("saturation").unwrap()
}

// ─── /v1 discovery is valid, complete JSON ────────────────────────────────────

#[test]
fn eval_schema_is_valid_json() {
    let output = saturation().args(["schema", "--v1"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("schema output must be valid JSON");
    assert!(parsed.get("v1").is_some(), "schema must have a 'v1' key");
    let v1 = &parsed["v1"];
    assert!(v1.get("operations").is_some(), "v1 must have 'operations'");
    assert!(
        v1.get("operationCount").is_some(),
        "v1 must have 'operationCount'"
    );
}

#[test]
fn eval_v1_inventory_covers_every_resource_family() {
    let output = saturation().args(["schema", "--v1"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let ops = parsed["v1"]["operations"].as_array().unwrap();

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
        "/library/rates",
        "/library/incentives",
        "/search",
        "/usage",
    ] {
        assert!(
            paths.iter().any(|p| p.contains(path_needle)),
            "inventory missing a path containing `{path_needle}`"
        );
    }
}

#[test]
fn eval_schema_is_deterministic() {
    let a = saturation().args(["schema", "--v1"]).output().unwrap();
    let b = saturation().args(["schema", "--v1"]).output().unwrap();
    assert_eq!(a.stdout, b.stdout, "schema output must be deterministic");
}

// ─── Every /v1 resource group is a valid --help target ────────────────────────

#[test]
fn eval_all_v1_resource_groups_have_help() {
    for resource in &[
        "budget",
        "transactions",
        "purchase-orders",
        "library",
        "incentives",
        "documents",
        "search",
        "webhooks",
        "usage",
        "contacts",
        "projects",
        "spaces",
        "comments",
        "workspaces",
        "me",
    ] {
        saturation()
            .args(["v1", resource, "--help"])
            .assert()
            .success();
    }
}

// ─── Help text carries examples an agent can copy ─────────────────────────────

#[test]
fn eval_agent_help_contains_examples() {
    let output = saturation().args(["agent", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("saturation") || stdout.contains("Example"),
        "agent help should contain usage examples"
    );
}

#[test]
fn eval_documents_help_describes_drop_and_assign() {
    saturation()
        .args(["v1", "documents", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("drop").or(predicate::str::contains("Drop")))
        .stdout(predicate::str::contains("assign").or(predicate::str::contains("Assign")));
}

// ─── Two-sourced schema: /v1 (offline) + agent (live, degrades gracefully) ────

#[test]
fn eval_schema_default_two_sourced() {
    let output = saturation().arg("schema").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        parsed.get("v1").is_some(),
        "schema must include the /v1 surface"
    );
    assert!(
        parsed.get("agent").is_some(),
        "schema must include the agent surface (even if unavailable without auth)"
    );
}
