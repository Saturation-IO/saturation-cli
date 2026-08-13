//! Integration tests for the public CLI surface.
//!
//! These assert one task-oriented command tree, one login, and an offline
//! `schema` generated from the public OpenAPI contract.

use assert_cmd::Command;
use predicates::prelude::*;

fn saturation() -> Command {
    Command::cargo_bin("saturation").unwrap()
}

// ─── Top-level help ───────────────────────────────────────────────────────────

#[test]
fn top_level_help_lists_resources_and_oauth_login() {
    saturation()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("whoami"))
        .stdout(predicate::str::contains("  auth ").not())
        .stdout(predicate::str::contains("--api-base-url"))
        .stdout(predicate::str::contains("projects"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("agent").not())
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

// ─── /v1 namespace: resource grammar from the OpenAPI inventory ───────────────

#[test]
fn root_help_lists_only_public_task_groups() {
    saturation()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("budget"))
        .stdout(predicate::str::contains("transactions"))
        .stdout(predicate::str::contains("purchase-orders"))
        .stdout(predicate::str::contains("library"))
        .stdout(predicate::str::contains("documents"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("webhooks"))
        .stdout(predicate::str::contains("contacts"))
        .stdout(predicate::str::contains("projects"))
        .stdout(predicate::str::contains("project-library").not())
        .stdout(predicate::str::contains("  incentives ").not())
        .stdout(predicate::str::contains("  comments ").not())
        .stdout(predicate::str::contains("  views ").not())
        .stdout(predicate::str::contains("  usage ").not());
}

#[test]
fn v1_budget_help_lists_computed_and_structured_reads() {
    saturation()
        .args(["budget", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("phase-totals"))
        .stdout(predicate::str::contains("lines"))
        .stdout(predicate::str::contains("phase-data"))
        .stdout(predicate::str::contains("line-phases").not())
        .stdout(predicate::str::contains("phases"));

    saturation()
        .args(["budget", "lines", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bulk"))
        .stdout(predicate::str::contains("create-many").not());

    saturation()
        .args(["budget", "phase-data", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("bulk"))
        .stdout(predicate::str::contains("set-many").not());
}

#[test]
fn v1_transactions_list_exposes_source_type_status_filters() {
    saturation()
        .args(["transactions", "list", "--help"])
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
        .args(["purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("submit"))
        .stdout(predicate::str::contains("cancel-submission"))
        .stdout(predicate::str::contains("void"))
        .stdout(predicate::str::contains("finalize").not())
        .stdout(predicate::str::contains("lifecycle").not());
}

#[test]
fn v1_purchase_orders_expose_timeline_without_duplicate_helpers() {
    saturation()
        .args(["purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("timeline"))
        .stdout(predicate::str::contains("activity").not())
        .stdout(predicate::str::contains("suggested-matches").not())
        .stdout(predicate::str::contains("reconciliation").not());
}

#[test]
fn v1_payments_keep_requests_and_timeline_separate() {
    saturation()
        .args(["payment-requests", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"));

    saturation()
        .args(["payments", "--help"])
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
        .args(["library", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rate-packs"))
        .stdout(predicate::str::contains("fringes"))
        .stdout(predicate::str::contains("globals"))
        .stdout(predicate::str::contains("currencies"))
        .stdout(predicate::str::contains("fringe-groups"))
        .stdout(predicate::str::contains("fringe-tags").not())
        .stdout(predicate::str::contains("tags"))
        .stdout(predicate::str::contains("units"))
        .stdout(predicate::str::contains("incentives"))
        .stdout(predicate::str::contains("project"));
}

#[test]
fn v1_documents_expose_link_tasks_without_a_links_collection() {
    saturation()
        .args(["documents", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upload"))
        .stdout(predicate::str::contains("  link "))
        .stdout(predicate::str::contains("  unlink "))
        .stdout(predicate::str::contains("  links ").not());

    saturation()
        .args(["documents", "link", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("transaction"))
        .stdout(predicate::str::contains("contact"))
        .stdout(predicate::str::contains("purchase-order"))
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("budget-line").not())
        .stdout(predicate::str::contains("phase").not());
}

// ─── S14: CLI reaches the same /v1 surface as the SDK ─────────────────────────

#[test]
fn v1_purchase_orders_expose_items_without_duplicate_reverse_reads() {
    saturation()
        .args(["purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("items"))
        .stdout(predicate::str::contains("  transactions ").not())
        .stdout(predicate::str::contains("  documents ").not());

    saturation()
        .args(["purchase-orders", "items", "--help"])
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
fn v1_purchase_orders_expose_mark_paid_and_transaction_links() {
    // Product wording is the only public command and URL.
    saturation()
        .args(["purchase-orders", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mark-paid"))
        .stdout(predicate::str::contains("link-transaction"))
        .stdout(predicate::str::contains("unlink-transaction"))
        .stdout(predicate::str::contains("finalize").not())
        .stdout(predicate::str::contains("lifecycle").not());
}

#[test]
fn v1_library_rates_expose_crud_lifecycle_and_items() {
    // Rate packs are pack-backed: CRUD + enable/disable + a per-pack items surface.
    saturation()
        .args(["library", "rate-packs", "--help"])
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
    // Fringes, globals, currencies, fringe groups, and tags are CRUD templates.
    // NOT carry the pack enable/disable lifecycle (that was the pre-sync drift).
    let output = saturation()
        .args(["library", "fringes", "--help"])
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
        .args(["library", "units", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));
}

#[test]
fn v1_project_library_exposes_add_remove() {
    saturation()
        .args(["library", "project", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rate-packs").not())
        .stdout(predicate::str::contains("incentives"))
        .stdout(predicate::str::contains("fringes"))
        .stdout(predicate::str::contains("globals"))
        .stdout(predicate::str::contains("currencies"))
        .stdout(predicate::str::contains("fringe-groups"))
        .stdout(predicate::str::contains("tags"));

    saturation()
        .args(["library", "project", "incentives", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"));
}

#[test]
fn projects_own_comments_and_views_are_not_public_tasks() {
    saturation()
        .args(["projects", "comments", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));

    saturation().args(["views", "--help"]).assert().failure();
}

// ─── Headless token injection (--token-file) + structured JSON errors ──────────

#[test]
fn token_file_injects_token_and_emits_json_errors() {
    use std::io::Write;

    // A token from a file is read per invocation and injected as the bearer token,
    // bypassing an interactive login.
    let mut token = tempfile::NamedTempFile::new().unwrap();
    write!(token, "header.payload.signature").unwrap();

    // Point at an unroutable host so the request fails at TRANSPORT, not at auth —
    // proving the token was injected (no "Not authenticated") AND that under the
    // default `--format json` the §5d error is emitted as clean JSON on stderr.
    let output = saturation()
        .args(["whoami", "--token-file"])
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

// ─── schema: offline public API inventory ─────────────────────────────────────

#[test]
fn schema_emits_offline_operation_inventory() {
    // The /v1 surface is embedded from the OpenAPI at build time, so it works
    // with no auth and no network.
    saturation()
        .arg("schema")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"api\""))
        .stdout(predicate::str::contains("operationCount"))
        .stdout(predicate::str::contains("bundled OpenAPI 3.1"))
        // a known operation from the inventory
        .stdout(predicate::str::contains("budget_listLines"));
}

#[test]
fn schema_exposes_only_the_public_api_inventory() {
    saturation()
        .arg("schema")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"api\""))
        .stdout(predicate::str::contains("\"agent\"").not());
}

// ─── Global flags ─────────────────────────────────────────────────────────────

#[test]
fn global_format_flag_accepts_json() {
    saturation()
        .args(["--format", "json", "schema"])
        .assert()
        .success();
}

#[test]
fn global_quiet_flag() {
    saturation().args(["--quiet", "schema"]).assert().success();
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
