//! Integration tests for the dual-namespace CLI surface.
//!
//! These assert the *intent* of the cli-rust ticket: one binary with two
//! namespaces (`/v1` resource grammar + `agent` tool registry), one login, and a
//! two-sourced `schema` that does not require a hand-maintained command tree.

use assert_cmd::Command;
use predicates::prelude::*;

fn saturation() -> Command {
    Command::cargo_bin("saturation").unwrap()
}

// ─── Top-level help: dual namespaces + one token ──────────────────────────────

#[test]
fn top_level_help_lists_both_namespaces_and_oauth_login() {
    saturation()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("--api-base-url"))
        // /v1 resource namespace
        .stdout(predicate::str::contains("v1"))
        // agent tool namespace
        .stdout(predicate::str::contains("agent"))
        .stdout(predicate::str::contains("schema"));
}

#[test]
fn version_output() {
    saturation()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("saturation"));
}

// ─── Auth: OAuth login plus personal API token fallback ──────────────────────

#[test]
fn auth_help_lists_personal_token_commands() {
    saturation()
        .args(["auth", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("token"))
        .stdout(predicate::str::contains("logout"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("device").not())
        .stdout(predicate::str::contains("refresh").not());
}

// ─── /v1 namespace: resource grammar from the OpenAPI inventory ───────────────

#[test]
fn v1_help_lists_every_resource_group() {
    saturation()
        .args(["v1", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("budget"))
        .stdout(predicate::str::contains("transactions"))
        .stdout(predicate::str::contains("purchase-orders"))
        .stdout(predicate::str::contains("library"))
        .stdout(predicate::str::contains("incentives"))
        .stdout(predicate::str::contains("documents"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("webhooks"))
        .stdout(predicate::str::contains("usage"))
        .stdout(predicate::str::contains("contacts"))
        .stdout(predicate::str::contains("projects"))
        .stdout(predicate::str::contains("project-library"))
        .stdout(predicate::str::contains("views"));
}

#[test]
fn v1_budget_help_lists_computed_and_structured_reads() {
    saturation()
        .args(["v1", "budget", "--help"])
        .assert()
        .success()
        // the budget "tree" read is exposed as the `document` subcommand
        .stdout(predicate::str::contains("document"))
        .stdout(predicate::str::contains("totals"))
        .stdout(predicate::str::contains("rollup"))
        .stdout(predicate::str::contains("variance"))
        .stdout(predicate::str::contains("cells"))
        .stdout(predicate::str::contains("lines"))
        .stdout(predicate::str::contains("phases"));
}

#[test]
fn v1_transactions_list_exposes_source_type_status_filters() {
    saturation()
        .args(["v1", "transactions", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--source"))
        .stdout(predicate::str::contains("--type"))
        .stdout(predicate::str::contains("--status"))
        // shared collection flags
        .stdout(predicate::str::contains("--expand"))
        .stdout(predicate::str::contains("--cursor"))
        .stdout(predicate::str::contains("--limit"));
}

#[test]
fn v1_purchase_orders_expose_status_actions() {
    saturation()
        .args(["v1", "purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("submit"))
        .stdout(predicate::str::contains("cancel-submission"))
        .stdout(predicate::str::contains("void"))
        .stdout(predicate::str::contains("finalize").not())
        .stdout(predicate::str::contains("lifecycle").not());
}

#[test]
fn v1_purchase_orders_expose_activity_timeline_and_suggested_matches() {
    saturation()
        .args(["v1", "purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("activity"))
        .stdout(predicate::str::contains("timeline"))
        .stdout(predicate::str::contains("suggested-matches"))
        .stdout(predicate::str::contains("reconciliation").not());
}

#[test]
fn v1_payments_keep_requests_and_timeline_separate() {
    saturation()
        .args(["v1", "payment-requests", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"));

    saturation()
        .args(["v1", "payments", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("timeline"))
        .stdout(predicate::str::contains("activity").not());
}

#[test]
fn v1_library_lists_all_sections() {
    saturation()
        .args(["v1", "library", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rates"))
        .stdout(predicate::str::contains("fringes"))
        .stdout(predicate::str::contains("globals"))
        .stdout(predicate::str::contains("currencies"))
        .stdout(predicate::str::contains("tags"))
        .stdout(predicate::str::contains("units"));
}

#[test]
fn v1_documents_expose_drop_and_assign() {
    saturation()
        .args(["v1", "documents", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("drop"))
        .stdout(predicate::str::contains("assign"))
        .stdout(predicate::str::contains("unassign"));
}

// ─── S14: CLI reaches the same /v1 surface as the SDK ─────────────────────────

#[test]
fn v1_purchase_orders_expose_items_and_reverse_reads() {
    // PO line-item CRUD + reverse reads (transactions, documents) must be
    // reachable, matching the SDK's `po.items(...)` / `po.transactions(...)`.
    saturation()
        .args(["v1", "purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("items"))
        .stdout(predicate::str::contains("transactions"))
        .stdout(predicate::str::contains("documents"));

    saturation()
        .args(["v1", "purchase-orders", "items", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));
}

// ─── Contract sync: the /v1 surface matches the workspace-root + per-resource
// shape of the next-api contract (regressions if the CLI drifts back). ─────────

#[test]
fn v1_purchase_orders_expose_mark_paid_link_unlink() {
    // Product wording is the only public command and URL.
    saturation()
        .args(["v1", "purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mark-paid"))
        .stdout(predicate::str::contains("link"))
        .stdout(predicate::str::contains("unlink"))
        .stdout(predicate::str::contains("finalize").not())
        .stdout(predicate::str::contains("lifecycle").not());
}

#[test]
fn v1_library_rates_expose_crud_lifecycle_and_items() {
    // Rate packs are pack-backed: CRUD + enable/disable + a per-pack items surface.
    saturation()
        .args(["v1", "library", "rates", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"))
        .stdout(predicate::str::contains("enable"))
        .stdout(predicate::str::contains("disable"))
        .stdout(predicate::str::contains("items"));
}

#[test]
fn v1_library_templates_are_crud_not_lifecycle() {
    // Fringes/globals/currencies/fringe-tags/tags are CRUD templates — they must
    // NOT carry the pack enable/disable lifecycle (that was the pre-sync drift).
    let output = saturation()
        .args(["v1", "library", "fringes", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("create") && stdout.contains("update") && stdout.contains("delete"));
    // The `enable`/`disable` SUBCOMMANDS must be absent (they 404 against the contract).
    assert!(
        !stdout.contains("  enable") && !stdout.contains("  disable"),
        "library template sections must not expose enable/disable subcommands"
    );
}

#[test]
fn v1_library_units_expose_custom_crud() {
    saturation()
        .args(["v1", "library", "units", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("custom"));
    saturation()
        .args(["v1", "library", "units", "custom", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));
}

#[test]
fn v1_project_library_exposes_add_remove() {
    saturation()
        .args(["v1", "project-library", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rates"))
        .stdout(predicate::str::contains("incentives"))
        .stdout(predicate::str::contains("fringes"))
        .stdout(predicate::str::contains("globals"))
        .stdout(predicate::str::contains("currencies"))
        .stdout(predicate::str::contains("fringe-tags"))
        .stdout(predicate::str::contains("tags"));

    // Project-resident rate packs are added / removed (copy-on-use), not
    // "installed/uninstalled" — both verbs map to `…/{packId}/add` (POST/DELETE).
    saturation()
        .args(["v1", "project-library", "rates", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"));

    saturation()
        .args(["v1", "project-library", "incentives", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"));
}

#[test]
fn v1_views_expose_list_get_data() {
    saturation()
        .args(["v1", "views", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("data"));
}

#[test]
fn v1_documents_expose_reverse_lookups() {
    saturation()
        .args(["v1", "documents", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("by-project"))
        .stdout(predicate::str::contains("by-transaction"))
        .stdout(predicate::str::contains("by-contact"));
}

// ─── Headless token injection (--token-file) + structured JSON errors ──────────

#[test]
fn v1_token_file_injects_token_and_emits_json_errors() {
    use std::io::Write;

    // A token from a file is read per-invocation and injected as the bearer token,
    // bypassing an interactive login (the desktop saturationV1 path).
    let mut token = tempfile::NamedTempFile::new().unwrap();
    write!(token, "header.payload.signature").unwrap();

    // Point at an unroutable host so the request fails at TRANSPORT, not at auth —
    // proving the token was injected (no "Not authenticated") AND that under the
    // default `--format json` the §5d error is emitted as clean JSON on stderr.
    let output = saturation()
        .args(["v1", "me", "--token-file"])
        .arg(token.path())
        .args(["--api-base-url", "http://127.0.0.1:1"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "transport failure should exit non-zero"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("Not authenticated"),
        "--token-file must inject a token (no auth prompt); got: {stderr}"
    );
    let parsed: serde_json::Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|e| panic!("error output must be JSON under --format json: {e}\n{stderr}"));
    assert!(
        parsed.get("code").is_some(),
        "the JSON error must carry a typed `code`"
    );
}

// ─── agent namespace: registry-driven discovery + dispatch ────────────────────

#[test]
fn agent_help_is_registry_driven() {
    saturation()
        .args(["agent", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tools"))
        .stdout(predicate::str::contains("call"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("upload"))
        .stdout(predicate::str::contains("exec").not());
}

#[test]
fn agent_call_takes_tool_name_and_params() {
    saturation()
        .args(["agent", "call", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--params"))
        .stdout(predicate::str::contains("--file"));
}

// ─── schema: two-sourced (OpenAPI /v1 + live agent registry) ──────────────────

#[test]
fn schema_v1_emits_offline_operation_inventory() {
    // The /v1 surface is embedded from the OpenAPI at build time, so it works
    // with no auth and no network.
    saturation()
        .args(["schema", "--v1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"v1\""))
        .stdout(predicate::str::contains("operationCount"))
        .stdout(predicate::str::contains("bundled OpenAPI 3.1"))
        // a known operation from the inventory
        .stdout(predicate::str::contains("budget_listLines"));
}

#[test]
fn schema_default_includes_v1_and_agent_keys() {
    // Without auth, the agent half degrades to `{ unavailable: ... }` rather than
    // crashing — both keys must still be present.
    saturation()
        .arg("schema")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"v1\""))
        .stdout(predicate::str::contains("\"agent\""));
}

// ─── Global flags ─────────────────────────────────────────────────────────────

#[test]
fn global_format_flag_accepts_json() {
    saturation()
        .args(["--format", "json", "schema", "--v1"])
        .assert()
        .success();
}

#[test]
fn global_quiet_flag() {
    saturation()
        .args(["--quiet", "schema", "--v1"])
        .assert()
        .success();
}

// ─── Auth status without config: must not crash ───────────────────────────────

#[test]
fn auth_status_no_config() {
    saturation().args(["auth", "status"]).assert().success();
}

// ─── Error cases ──────────────────────────────────────────────────────────────

#[test]
fn no_subcommand_shows_usage() {
    saturation()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn invalid_subcommand_fails() {
    saturation().arg("bogus").assert().failure();
}
