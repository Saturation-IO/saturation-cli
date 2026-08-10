//! The thin ergonomic layer over the generated `/v1` [`Client`]: it maps each
//! clap subcommand to a typed operation, builds the query string from the
//! collection flags, and renders the §5d success/error model through [`Output`].

use anyhow::Result;
use serde_json::Value;

use super::cli::*;
use super::generated::{ApiResult, Client};
use crate::cli::OutputFormat;
use crate::output::Output;

/// Build a query vec from the shared collection flags plus any resource-specific
/// pairs. Optional flags that are `None` are dropped (no hidden params).
fn build_query(
    flags: &ListFlags,
    extra: Vec<(&'static str, Option<String>)>,
) -> Vec<(&'static str, String)> {
    let mut q: Vec<(&'static str, String)> = Vec::new();
    if let Some(v) = flags.limit {
        q.push(("limit", v.to_string()));
    }
    if let Some(v) = &flags.cursor {
        q.push(("cursor", v.clone()));
    }
    if let Some(v) = &flags.expand {
        q.push(("expand", v.clone()));
    }
    if let Some(v) = &flags.fields {
        q.push(("fields", v.clone()));
    }
    if let Some(v) = &flags.sort {
        q.push(("sort", v.clone()));
    }
    if let Some(v) = &flags.order {
        q.push(("order", v.clone()));
    }
    if flags.with_count {
        q.push(("withCount", "true".to_string()));
    }
    for (k, v) in extra {
        if let Some(v) = v {
            q.push((k, v));
        }
    }
    // Raw `--filter key=value` escape hatch. Leaked here so the generated client
    // can express any documented param without a dedicated flag.
    for raw in &flags.filters {
        if let Some((k, v)) = raw.split_once('=') {
            // Leak the key string so it satisfies the `'static` query API; this
            // runs once per invocation, so the small leak is acceptable.
            let key: &'static str = Box::leak(k.to_string().into_boxed_str());
            q.push((key, v.to_string()));
        }
    }
    q
}

/// Parse an inline JSON argument into a `serde_json::Value` for a request body.
fn parse_json(data: &str) -> Result<Value> {
    serde_json::from_str(data).map_err(|e| anyhow::anyhow!("invalid JSON body: {e}"))
}

/// Build the `POST /documents/{id}/assign` body. The handler is `.strict()` and
/// requires the nested `{ target: { kind, id }, replace? }` shape
/// (`DocumentAssignRequest`); a flat `{ kind, id }` is rejected with `422`.
fn document_assign_body(kind: &str, id: &str, replace: bool) -> Value {
    serde_json::json!({ "target": { "kind": kind, "id": id }, "replace": replace })
}

/// Build the `POST /documents/{id}/unassign` body (`DocumentUnassignRequest`):
/// nested `{ target: { kind, id } }`. No `replace` field on this surface.
fn document_unassign_body(kind: &str, id: &str) -> Value {
    serde_json::json!({ "target": { "kind": kind, "id": id } })
}

/// Render the result of a `/v1` call. Success prints the bare resource /
/// collection; error prints the typed §5d envelope and returns a non-zero exit
/// via `anyhow`.
fn render(result: ApiResult<Value>, output: &Output) -> Result<()> {
    match result {
        Ok(value) => {
            output.print(&value)?;
            Ok(())
        }
        Err(err) => {
            if matches!(output.format, OutputFormat::Json) {
                // Emit the typed §5d envelope as a single JSON line on stderr and
                // exit non-zero, bypassing anyhow's "Error: " Debug wrapper so a
                // structured caller (the desktop `saturationV1` tool) parses clean
                // JSON for `code` / `message` / `fieldErrors` self-correction.
                let json = serde_json::to_string(&err).unwrap_or_else(|_| err.render());
                eprintln!("{json}");
                std::process::exit(1);
            }
            Err(anyhow::anyhow!("{}", err.render()))
        }
    }
}

pub async fn execute(args: V1Args, client: &Client, output: &Output) -> Result<()> {
    let idem = args.idempotency_key.as_deref();
    let project = args.project.clone();
    let require_project = || -> Result<String> {
        project.clone().ok_or_else(|| {
            anyhow::anyhow!("this resource is project-scoped; pass --project <slug|id>")
        })
    };

    match args.command {
        // ── Meta / identity ──────────────────────────────────────────────────
        V1Command::Me => render(client.get("me", &[]).await, output),

        V1Command::Workspaces(a) => {
            let q = build_query(&a.flags, vec![]);
            render(client.get("workspaces", &q).await, output)
        }

        // ── Generic resources ────────────────────────────────────────────────
        V1Command::Projects(a) => {
            generic_resource(
                a.command,
                client,
                output,
                idem,
                &client.ws_path("projects"),
                "projects",
            )
            .await
        }
        V1Command::Spaces(a) => {
            crud_resource(a.command, client, output, idem, &client.ws_path("spaces")).await
        }
        V1Command::Contacts(a) => {
            generic_resource(
                a.command,
                client,
                output,
                idem,
                &client.ws_path("contacts"),
                "contacts",
            )
            .await
        }
        V1Command::Comments(a) => {
            crud_resource(a.command, client, output, idem, &client.ws_path("comments")).await
        }

        // ── Budget ────────────────────────────────────────────────────────────
        V1Command::Budget(a) => {
            let p = require_project()?;
            budget(a.command, client, output, idem, &p).await
        }

        // ── Transactions (workspace-root; `--project` is a `projectId` filter) ────
        V1Command::Transactions(a) => {
            transactions(a.command, client, output, idem, project.as_deref()).await
        }

        // ── Purchase orders (workspace-root; `--project` is a `projectId` filter) ──
        V1Command::PurchaseOrders(a) => {
            purchase_orders(a.command, client, output, idem, project.as_deref()).await
        }
        V1Command::PaymentRequests(a) => {
            payment_requests(a.command, client, output, project.as_deref()).await
        }
        V1Command::Payments(a) => payments(a.command, client, output, project.as_deref()).await,

        // ── Library (workspace source scope) ──────────────────────────────────────
        V1Command::Library(a) => library(a.command, client, output, idem).await,

        // ── Project-resident Library ──────────────────────────────────────────────
        V1Command::ProjectLibrary(a) => {
            let p = require_project()?;
            project_library(a.command, client, output, idem, &p).await
        }

        V1Command::Incentives(a) => incentives(a.command, client, output).await,

        // ── Saved views ───────────────────────────────────────────────────────────
        V1Command::Views(a) => {
            let p = require_project()?;
            views(a.command, client, output, &p).await
        }

        // ── Documents ───────────────────────────────────────────────────────────
        V1Command::Documents(a) => {
            documents(a.command, client, output, idem, project.as_deref()).await
        }

        // ── Search ──────────────────────────────────────────────────────────────
        V1Command::Search(a) => {
            let q = build_query(&a.flags, vec![("q", Some(a.query)), ("types", a.types)]);
            let path = match &project {
                Some(p) => client.project_path(p, "search"),
                None => client.ws_path("search"),
            };
            render(client.get(&path, &q).await, output)
        }

        // ── Webhooks ──────────────────────────────────────────────────────────────
        V1Command::Webhooks(a) => webhooks(a.command, client, output, idem).await,

        // ── Usage ───────────────────────────────────────────────────────────────
        V1Command::Usage(a) => usage(a.command, client, output, project.as_deref()).await,
    }
}

/// CRUD for a workspace-scoped collection at `base` (already templated).
async fn generic_resource(
    cmd: ResourceCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    base: &str,
    _label: &str,
) -> Result<()> {
    match cmd {
        ResourceCommand::List(flags) => {
            let q = build_query(&flags, vec![]);
            render(client.get(base, &q).await, output)
        }
        ResourceCommand::Get { id, flags } => {
            let q = build_query(&flags, vec![]);
            render(client.get(&format!("{base}/{id}"), &q).await, output)
        }
        ResourceCommand::Create { data } => {
            let body = parse_json(&data)?;
            render(client.post(base, &body, idem).await, output)
        }
        ResourceCommand::Update { id, data } => {
            let body = parse_json(&data)?;
            render(client.patch(&format!("{base}/{id}"), &body).await, output)
        }
        ResourceCommand::Delete { id } => {
            render(client.delete(&format!("{base}/{id}")).await, output)
        }
    }
}

/// CRUD for a collection whose single-resource path has no `GET /{id}` (spaces,
/// comments, line items, custom units): list / create / update / delete only.
async fn crud_resource(
    cmd: CrudCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    base: &str,
) -> Result<()> {
    match cmd {
        CrudCommand::List(flags) => {
            let q = build_query(&flags, vec![]);
            render(client.get(base, &q).await, output)
        }
        CrudCommand::Create { data } => {
            let body = parse_json(&data)?;
            render(client.post(base, &body, idem).await, output)
        }
        CrudCommand::Update { id, data } => {
            let body = parse_json(&data)?;
            render(client.patch(&format!("{base}/{id}"), &body).await, output)
        }
        CrudCommand::Delete { id } => render(client.delete(&format!("{base}/{id}")).await, output),
    }
}

async fn budget(
    cmd: BudgetCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
) -> Result<()> {
    let base = |suffix: &str| client.project_path(project, suffix);
    match cmd {
        BudgetCommand::Document(f) => {
            let q = vec![
                ("path", f.path),
                ("accountCode", f.account_code),
                ("phase", f.phase),
            ]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect::<Vec<_>>();
            render(client.get(&base("budget"), &q).await, output)
        }
        BudgetCommand::Totals(f) => render(
            client
                .get(&base("budget/totals"), &build_query(&f, vec![]))
                .await,
            output,
        ),
        BudgetCommand::Rollup(f) => render(
            client
                .get(&base("budget/rollup"), &build_query(&f, vec![]))
                .await,
            output,
        ),
        BudgetCommand::Variance(f) => render(
            client
                .get(&base("budget/variance"), &build_query(&f, vec![]))
                .await,
            output,
        ),
        BudgetCommand::Cells { account, column } => {
            let q = vec![("account", account), ("column", column)];
            render(client.get(&base("budget/cells"), &q).await, output)
        }
        BudgetCommand::Lines(la) => budget_lines(la.command, client, output, idem, project).await,
        BudgetCommand::PhaseData(phase_data) => {
            budget_phase_data(phase_data.command, client, output, idem, project).await
        }
        BudgetCommand::Phases(pw) => {
            generic_resource(
                pw.command,
                client,
                output,
                idem,
                &base("budget/phases"),
                "phases",
            )
            .await
        }
        BudgetCommand::Accounts(f) => render(
            client
                .get(&base("budget/accounts"), &build_query(&f, vec![]))
                .await,
            output,
        ),
    }
}

async fn budget_lines(
    cmd: LineCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
) -> Result<()> {
    let base = client.project_path(project, "budget/lines");
    match cmd {
        LineCommand::List {
            account_id,
            path,
            phase,
            tags,
            kind,
            flags,
        } => {
            let q = build_query(
                &flags,
                vec![
                    ("accountId", account_id),
                    ("path", path),
                    ("phase", phase),
                    ("tags", tags),
                    ("kind", kind),
                ],
            );
            render(client.get(&base, &q).await, output)
        }
        LineCommand::Get { line_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(client.get(&format!("{base}/{line_id}"), &q).await, output)
        }
        LineCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        LineCommand::CreateBatch { data } => render(
            client
                .post(&format!("{base}/batch"), &parse_json(&data)?, idem)
                .await,
            output,
        ),
        LineCommand::Update { line_id, data } => render(
            client
                .patch(&format!("{base}/{line_id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        LineCommand::Delete { line_id } => {
            render(client.delete(&format!("{base}/{line_id}")).await, output)
        }
    }
}

async fn budget_phase_data(
    cmd: BudgetPhaseDataCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
) -> Result<()> {
    let base = client.project_path(project, "budget");
    match cmd {
        BudgetPhaseDataCommand::Upsert {
            line_id,
            phase_id,
            data,
        } => render(
            client
                .put(
                    &format!("{base}/lines/{line_id}/phase-data/{phase_id}"),
                    &parse_json(&data)?,
                )
                .await,
            output,
        ),
        BudgetPhaseDataCommand::Batch { data } => render(
            client
                .post(
                    &format!("{base}/lines/phase-data/batch"),
                    &parse_json(&data)?,
                    idem,
                )
                .await,
            output,
        ),
    }
}

async fn transactions(
    cmd: TransactionCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: Option<&str>,
) -> Result<()> {
    // Transactions are workspace-root; the token determines the workspace and
    // `--project` narrows to one project via the `projectId` query filter.
    let base = |suffix: &str| client.ws_path(suffix);
    let project_id = || project.map(|p| p.to_string());
    match cmd {
        TransactionCommand::List {
            source,
            r#type,
            status,
            flags,
        } => {
            let q = build_query(
                &flags,
                vec![
                    ("projectId", project_id()),
                    ("source", source),
                    ("type", r#type),
                    ("status", status),
                ],
            );
            render(client.get(&base("transactions"), &q).await, output)
        }
        TransactionCommand::Get { tx_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(&base(&format!("transactions/{tx_id}")), &q)
                    .await,
                output,
            )
        }
        TransactionCommand::Create { data } => render(
            client
                .post(&base("transactions"), &parse_json(&data)?, idem)
                .await,
            output,
        ),
        TransactionCommand::Update { tx_id, data } => render(
            client
                .patch(&base(&format!("transactions/{tx_id}")), &parse_json(&data)?)
                .await,
            output,
        ),
        TransactionCommand::Delete { tx_id } => render(
            client.delete(&base(&format!("transactions/{tx_id}"))).await,
            output,
        ),
        TransactionCommand::Stats(f) => {
            let q = build_query(&f, vec![("projectId", project_id())]);
            render(client.get(&base("transactions/stats"), &q).await, output)
        }
        TransactionCommand::Types => {
            render(client.get(&base("transactions/types"), &[]).await, output)
        }
        TransactionCommand::Batch { data } => render(
            client
                .post(&base("transactions/batch"), &parse_json(&data)?, idem)
                .await,
            output,
        ),
        TransactionCommand::Items(item) => {
            let item_base = base(&format!("transactions/{}/items", item.tx_id));
            crud_resource(item.command, client, output, idem, &item_base).await
        }
    }
}

async fn purchase_orders(
    cmd: PurchaseOrderCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: Option<&str>,
) -> Result<()> {
    // Purchase orders are workspace-root; `--project` narrows the list to one
    // project via the `projectId` query filter.
    let base = |suffix: &str| client.ws_path(suffix);
    let pos = "purchase-orders";
    match cmd {
        PurchaseOrderCommand::List(f) => {
            let q = build_query(&f, vec![("projectId", project.map(|p| p.to_string()))]);
            render(client.get(&base(pos), &q).await, output)
        }
        PurchaseOrderCommand::Get {
            purchase_order_id,
            flags,
        } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(&base(&format!("{pos}/{purchase_order_id}")), &q)
                    .await,
                output,
            )
        }
        PurchaseOrderCommand::Create { data } => render(
            client.post(&base(pos), &parse_json(&data)?, idem).await,
            output,
        ),
        PurchaseOrderCommand::Update {
            purchase_order_id,
            data,
        } => render(
            client
                .patch(
                    &base(&format!("{pos}/{purchase_order_id}")),
                    &parse_json(&data)?,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Delete { purchase_order_id } => render(
            client
                .delete(&base(&format!("{pos}/{purchase_order_id}")))
                .await,
            output,
        ),
        PurchaseOrderCommand::Submit { purchase_order_id } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/submit")),
                    &Value::Null,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::CancelSubmission { purchase_order_id } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/cancel-submission")),
                    &Value::Null,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Void { purchase_order_id } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/void")),
                    &Value::Null,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::MarkPaid { purchase_order_id } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/mark-paid")),
                    &Value::Null,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Link {
            purchase_order_id,
            data,
        } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/link")),
                    &parse_json(&data)?,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Unlink {
            purchase_order_id,
            data,
        } => render(
            client
                .post(
                    &base(&format!("{pos}/{purchase_order_id}/unlink")),
                    &parse_json(&data)?,
                    idem,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Activity { purchase_order_id } => render(
            client
                .get(&base(&format!("{pos}/{purchase_order_id}/activity")), &[])
                .await,
            output,
        ),
        PurchaseOrderCommand::SuggestedMatches { purchase_order_id } => render(
            client
                .get(
                    &base(&format!("{pos}/{purchase_order_id}/suggested-matches")),
                    &[],
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::Timeline {
            purchase_order_id,
            flags,
        } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(&base(&format!("{pos}/{purchase_order_id}/timeline")), &q)
                    .await,
                output,
            )
        }
        PurchaseOrderCommand::Items(item) => {
            let item_base = base(&format!("{pos}/{}/items", item.purchase_order_id));
            crud_resource(item.command, client, output, idem, &item_base).await
        }
        PurchaseOrderCommand::Transactions {
            purchase_order_id,
            flags,
        } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(
                        &base(&format!("{pos}/{purchase_order_id}/transactions")),
                        &q,
                    )
                    .await,
                output,
            )
        }
        PurchaseOrderCommand::Documents {
            purchase_order_id,
            flags,
        } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(&base(&format!("{pos}/{purchase_order_id}/documents")), &q)
                    .await,
                output,
            )
        }
    }
}

async fn payment_requests(
    cmd: PaymentRequestCommand,
    client: &Client,
    output: &Output,
    project: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("payment-requests");
    match cmd {
        PaymentRequestCommand::List(flags) => {
            let q = build_query(&flags, vec![("projectId", project.map(|p| p.to_string()))]);
            render(client.get(&base, &q).await, output)
        }
        PaymentRequestCommand::Get {
            payment_request_id,
            flags,
        } => render(
            client
                .get(
                    &format!("{base}/{payment_request_id}"),
                    &build_query(&flags, vec![]),
                )
                .await,
            output,
        ),
    }
}

async fn payments(
    cmd: PaymentCommand,
    client: &Client,
    output: &Output,
    project: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("payments");
    match cmd {
        PaymentCommand::List(flags) => {
            let q = build_query(&flags, vec![("projectId", project.map(|p| p.to_string()))]);
            render(client.get(&base, &q).await, output)
        }
        PaymentCommand::Get { payment_id, flags } => render(
            client
                .get(
                    &format!("{base}/{payment_id}"),
                    &build_query(&flags, vec![]),
                )
                .await,
            output,
        ),
        PaymentCommand::Timeline { payment_id, flags } => render(
            client
                .get(
                    &format!("{base}/{payment_id}/timeline"),
                    &build_query(&flags, vec![]),
                )
                .await,
            output,
        ),
    }
}

async fn library(
    cmd: LibraryCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    match cmd {
        LibraryCommand::Rates(a) => library_rates(a.command, client, output, idem).await,
        LibraryCommand::Fringes(a) => {
            library_crud(a.command, client, output, idem, "fringes").await
        }
        LibraryCommand::Globals(a) => {
            library_crud(a.command, client, output, idem, "globals").await
        }
        LibraryCommand::Currencies(a) => {
            library_crud(a.command, client, output, idem, "currencies").await
        }
        LibraryCommand::FringeTags(a) => {
            library_crud(a.command, client, output, idem, "fringe-tags").await
        }
        LibraryCommand::Tags(a) => library_crud(a.command, client, output, idem, "tags").await,
        LibraryCommand::Units(a) => library_units(a.command, client, output, idem).await,
    }
}

/// CRUD for the workspace-Library template sections (fringes / globals /
/// currencies / fringe-tags / tags): list / get / create / update / delete.
async fn library_crud(
    cmd: LibraryCrudCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    section: &str,
) -> Result<()> {
    let base = client.ws_path(&format!("library/{section}"));
    match cmd {
        LibraryCrudCommand::List(f) => {
            render(client.get(&base, &build_query(&f, vec![])).await, output)
        }
        LibraryCrudCommand::Get { id, flags } => {
            let q = build_query(&flags, vec![]);
            render(client.get(&format!("{base}/{id}"), &q).await, output)
        }
        LibraryCrudCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        LibraryCrudCommand::Update { id, data } => render(
            client
                .patch(&format!("{base}/{id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        LibraryCrudCommand::Delete { id } => {
            render(client.delete(&format!("{base}/{id}")).await, output)
        }
    }
}

/// Rate packs: create, read, update, delete, enable, and disable.
/// + the per-pack `/items` sub-resource.
async fn library_rates(
    cmd: LibraryRatesCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("library/rates");
    match cmd {
        LibraryRatesCommand::List(f) => {
            render(client.get(&base, &build_query(&f, vec![])).await, output)
        }
        LibraryRatesCommand::Get { id, flags } => {
            let q = build_query(&flags, vec![]);
            render(client.get(&format!("{base}/{id}"), &q).await, output)
        }
        LibraryRatesCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        LibraryRatesCommand::Update { id, data } => render(
            client
                .patch(&format!("{base}/{id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        LibraryRatesCommand::Delete { id } => {
            render(client.delete(&format!("{base}/{id}")).await, output)
        }
        LibraryRatesCommand::Enable { id } => render(
            client
                .post(&format!("{base}/{id}/enable"), &Value::Null, idem)
                .await,
            output,
        ),
        LibraryRatesCommand::Disable { id } => {
            render(client.delete(&format!("{base}/{id}/enable")).await, output)
        }
        LibraryRatesCommand::Items(item) => {
            let item_base = format!("{base}/{}/items", item.pack_id);
            crud_resource(item.command, client, output, idem, &item_base).await
        }
    }
}

/// Units: built-in + custom read (`GET /library/units`) and custom-unit CRUD
/// (`/library/units/custom`).
async fn library_units(
    cmd: LibraryUnitsCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    match cmd {
        LibraryUnitsCommand::List(f) => render(
            client
                .get(&client.ws_path("library/units"), &build_query(&f, vec![]))
                .await,
            output,
        ),
        LibraryUnitsCommand::Custom(a) => {
            crud_resource(
                a.command,
                client,
                output,
                idem,
                &client.ws_path("library/units/custom"),
            )
            .await
        }
    }
}

async fn incentives(cmd: IncentiveCommand, client: &Client, output: &Output) -> Result<()> {
    let base = client.ws_path("library/incentives");
    match cmd {
        IncentiveCommand::List(f) => {
            render(client.get(&base, &build_query(&f, vec![])).await, output)
        }
        IncentiveCommand::Get { pack_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(client.get(&format!("{base}/{pack_id}"), &q).await, output)
        }
        IncentiveCommand::Programs { pack_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client.get(&format!("{base}/{pack_id}/programs"), &q).await,
                output,
            )
        }
        IncentiveCommand::Enable { pack_id } => render(
            client
                .post(&format!("{base}/{pack_id}/enable"), &Value::Null, None)
                .await,
            output,
        ),
        IncentiveCommand::Disable { pack_id } => render(
            client.delete(&format!("{base}/{pack_id}/enable")).await,
            output,
        ),
    }
}

async fn project_library(
    cmd: ProjectLibraryCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
) -> Result<()> {
    match cmd {
        ProjectLibraryCommand::Rates(a) => {
            let base = |s: &str| client.project_path(project, &format!("library/rates/{s}"));
            match a.command {
                ProjectRateCommand::List(f) => render(
                    client
                        .get(
                            &client.project_path(project, "library/rates"),
                            &build_query(&f, vec![]),
                        )
                        .await,
                    output,
                ),
                ProjectRateCommand::Add { pack_id } => render(
                    client
                        .post(&base(&format!("{pack_id}/add")), &Value::Null, idem)
                        .await,
                    output,
                ),
                ProjectRateCommand::Remove { pack_id } => render(
                    client.delete(&base(&format!("{pack_id}/add"))).await,
                    output,
                ),
            }
        }
        ProjectLibraryCommand::Incentives(a) => {
            let coll = client.project_path(project, "library/incentives");
            match a.command {
                ProjectIncentiveCommand::List(f) => {
                    render(client.get(&coll, &build_query(&f, vec![])).await, output)
                }
                ProjectIncentiveCommand::Get {
                    incentive_id,
                    flags,
                } => render(
                    client
                        .get(
                            &format!("{coll}/{incentive_id}"),
                            &build_query(&flags, vec![]),
                        )
                        .await,
                    output,
                ),
                ProjectIncentiveCommand::Add { data } => render(
                    client
                        .post(&format!("{coll}/add"), &parse_json(&data)?, idem)
                        .await,
                    output,
                ),
                ProjectIncentiveCommand::Update { incentive_id, data } => render(
                    client
                        .patch(&format!("{coll}/{incentive_id}"), &parse_json(&data)?)
                        .await,
                    output,
                ),
                ProjectIncentiveCommand::Delete { incentive_id } => render(
                    client.delete(&format!("{coll}/{incentive_id}")).await,
                    output,
                ),
            }
        }
        ProjectLibraryCommand::Fringes(a) => {
            project_library_copy(a.command, client, output, idem, project, "fringes").await
        }
        ProjectLibraryCommand::Globals(a) => {
            project_library_copy(a.command, client, output, idem, project, "globals").await
        }
        ProjectLibraryCommand::Currencies(a) => {
            project_library_copy(a.command, client, output, idem, project, "currencies").await
        }
        ProjectLibraryCommand::FringeTags(a) => {
            project_library_copy(a.command, client, output, idem, project, "fringe-tags").await
        }
        ProjectLibraryCommand::Tags(a) => {
            let coll = client.project_path(project, "library/tags");
            match a.command {
                ProjectTagCommand::List(f) => {
                    render(client.get(&coll, &build_query(&f, vec![])).await, output)
                }
                ProjectTagCommand::Add { tag_id, data } => {
                    let body = match data {
                        Some(d) => parse_json(&d)?,
                        None => Value::Null,
                    };
                    render(
                        client
                            .post(&format!("{coll}/{tag_id}/add"), &body, idem)
                            .await,
                        output,
                    )
                }
                ProjectTagCommand::Remove { tag_id } => {
                    render(client.delete(&format!("{coll}/{tag_id}/add")).await, output)
                }
            }
        }
    }
}

/// CRUD for the copy-on-use project-Library sections (fringes / globals /
/// currencies / fringe-tags). `add` posts `{ sourceId }` to `…/{section}/add`.
async fn project_library_copy(
    cmd: ProjectCopyCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
    section: &str,
) -> Result<()> {
    let coll = client.project_path(project, &format!("library/{section}"));
    match cmd {
        ProjectCopyCommand::List(f) => {
            render(client.get(&coll, &build_query(&f, vec![])).await, output)
        }
        ProjectCopyCommand::Get { id, flags } => render(
            client
                .get(&format!("{coll}/{id}"), &build_query(&flags, vec![]))
                .await,
            output,
        ),
        ProjectCopyCommand::Add { source_id } => {
            let body = serde_json::json!({ "sourceId": source_id });
            render(
                client.post(&format!("{coll}/add"), &body, idem).await,
                output,
            )
        }
        ProjectCopyCommand::Update { id, data } => render(
            client
                .patch(&format!("{coll}/{id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        ProjectCopyCommand::Delete { id } => {
            render(client.delete(&format!("{coll}/{id}")).await, output)
        }
    }
}

async fn views(cmd: ViewCommand, client: &Client, output: &Output, project: &str) -> Result<()> {
    let coll = client.project_path(project, "views");
    match cmd {
        ViewCommand::List {
            subject_type,
            visibility,
            flags,
        } => {
            let q = build_query(
                &flags,
                vec![("subjectType", subject_type), ("visibility", visibility)],
            );
            render(client.get(&coll, &q).await, output)
        }
        ViewCommand::Get { view_id } => {
            render(client.get(&format!("{coll}/{view_id}"), &[]).await, output)
        }
        ViewCommand::Data { view_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client.get(&format!("{coll}/{view_id}/data"), &q).await,
                output,
            )
        }
    }
}

async fn documents(
    cmd: DocumentCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("documents");
    // Project-scoped reverse reads share this guard.
    let require_project = || -> Result<&str> {
        project.ok_or_else(|| {
            anyhow::anyhow!("this reverse read is project-scoped; pass --project <slug|id>")
        })
    };
    match cmd {
        DocumentCommand::List(f) => {
            render(client.get(&base, &build_query(&f, vec![])).await, output)
        }
        DocumentCommand::Get { document_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client.get(&format!("{base}/{document_id}"), &q).await,
                output,
            )
        }
        DocumentCommand::Drop {
            file,
            classification,
            name,
        } => {
            let mut meta = serde_json::Map::new();
            if let Some(c) = classification {
                meta.insert("classification".into(), Value::String(c));
            }
            if let Some(n) = name {
                meta.insert("name".into(), Value::String(n));
            }
            let result = client
                .upload_document(std::path::Path::new(&file), Value::Object(meta))
                .await;
            render(result, output)
        }
        DocumentCommand::Update { document_id, data } => render(
            client
                .patch(&format!("{base}/{document_id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        DocumentCommand::Delete { document_id } => render(
            client.delete(&format!("{base}/{document_id}")).await,
            output,
        ),
        DocumentCommand::Content { document_id } => render(
            client
                .get(&format!("{base}/{document_id}/content"), &[])
                .await,
            output,
        ),
        DocumentCommand::Extraction { document_id } => render(
            client
                .get(&format!("{base}/{document_id}/extraction"), &[])
                .await,
            output,
        ),
        DocumentCommand::Assign {
            document_id,
            kind,
            id,
            replace,
        } => {
            let body = document_assign_body(&kind, &id, replace);
            render(
                client
                    .post(&format!("{base}/{document_id}/assign"), &body, idem)
                    .await,
                output,
            )
        }
        DocumentCommand::Unassign {
            document_id,
            kind,
            id,
        } => {
            let body = document_unassign_body(&kind, &id);
            render(
                client
                    .post(&format!("{base}/{document_id}/unassign"), &body, idem)
                    .await,
                output,
            )
        }
        DocumentCommand::Assignments { document_id } => render(
            client
                .get(&format!("{base}/{document_id}/assignments"), &[])
                .await,
            output,
        ),
        DocumentCommand::ByProject { flags } => {
            let p = require_project()?;
            let q = build_query(&flags, vec![]);
            render(
                client.get(&client.project_path(p, "documents"), &q).await,
                output,
            )
        }
        DocumentCommand::ByTransaction { tx_id, flags } => {
            // Transactions are workspace-root; this reverse read is NOT project-scoped.
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(
                        &client.ws_path(&format!("transactions/{tx_id}/documents")),
                        &q,
                    )
                    .await,
                output,
            )
        }
        DocumentCommand::ByContact { contact_id, flags } => {
            // Contacts are workspace-level; this reverse read is NOT project-scoped.
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(
                        &client.ws_path(&format!("contacts/{contact_id}/documents")),
                        &q,
                    )
                    .await,
                output,
            )
        }
    }
}

async fn webhooks(
    cmd: WebhookCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("webhooks");
    match cmd {
        WebhookCommand::List(f) => {
            render(client.get(&base, &build_query(&f, vec![])).await, output)
        }
        WebhookCommand::Get { webhook_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client.get(&format!("{base}/{webhook_id}"), &q).await,
                output,
            )
        }
        WebhookCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        WebhookCommand::Update { webhook_id, data } => render(
            client
                .patch(&format!("{base}/{webhook_id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        WebhookCommand::Delete { webhook_id } => {
            render(client.delete(&format!("{base}/{webhook_id}")).await, output)
        }
        WebhookCommand::Ping { webhook_id } => render(
            client
                .post(&format!("{base}/{webhook_id}/ping"), &Value::Null, None)
                .await,
            output,
        ),
        WebhookCommand::Deliveries { webhook_id, flags } => {
            let q = build_query(&flags, vec![]);
            render(
                client
                    .get(&format!("{base}/{webhook_id}/deliveries"), &q)
                    .await,
                output,
            )
        }
    }
}

async fn usage(
    cmd: UsageCommand,
    client: &Client,
    output: &Output,
    project: Option<&str>,
) -> Result<()> {
    match cmd {
        UsageCommand::Summary(f) => {
            let path = match project {
                Some(p) => client.project_path(p, "usage"),
                None => client.ws_path("usage"),
            };
            render(client.get(&path, &build_query(&f, vec![])).await, output)
        }
        UsageCommand::Credits => render(
            client.get(&client.ws_path("usage/credits"), &[]).await,
            output,
        ),
        UsageCommand::Operations(f) => render(
            client
                .get(
                    &client.ws_path("usage/operations"),
                    &build_query(&f, vec![]),
                )
                .await,
            output,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_none_flags_from_query() {
        let flags = ListFlags {
            limit: Some(10),
            ..Default::default()
        };
        let q = build_query(&flags, vec![("source", None), ("type", Some("po".into()))]);
        assert!(q.contains(&("limit", "10".to_string())));
        assert!(q.contains(&("type", "po".to_string())));
        assert!(!q.iter().any(|(k, _)| *k == "source"));
    }

    #[test]
    fn with_count_emits_true() {
        let flags = ListFlags {
            with_count: true,
            ..Default::default()
        };
        let q = build_query(&flags, vec![]);
        assert!(q.contains(&("withCount", "true".to_string())));
    }

    #[test]
    fn raw_filter_splits_on_first_eq() {
        let flags = ListFlags {
            filters: vec!["tagMode=all".into(), "kind=line,account".into()],
            ..Default::default()
        };
        let q = build_query(&flags, vec![]);
        assert!(q.contains(&("tagMode", "all".to_string())));
        assert!(q.contains(&("kind", "line,account".to_string())));
    }

    // The `.strict()` assign/unassign handlers reject a flat `{ kind, id }` with
    // `422`; the contract (`DocumentAssignRequest` / `DocumentUnassignRequest`)
    // requires the target nested under `target`. These guard the wrapping so the
    // CLI body matches the OpenAPI schema, not the legacy flat shape.
    #[test]
    fn assign_body_nests_target_and_includes_replace() {
        let body = document_assign_body("transaction", "txn_8f2a", true);
        // Flat shape must NOT leak — that was the 422 bug.
        assert!(body.get("kind").is_none());
        assert!(body.get("id").is_none());
        assert_eq!(body["target"]["kind"], "transaction");
        assert_eq!(body["target"]["id"], "txn_8f2a");
        assert_eq!(body["replace"], true);
    }

    #[test]
    fn assign_body_defaults_replace_false() {
        let body = document_assign_body("budgetLine", "lin_3d77", false);
        assert_eq!(body["replace"], false);
    }

    #[test]
    fn unassign_body_nests_target_without_replace() {
        let body = document_unassign_body("transaction", "txn_8f2a");
        assert!(body.get("kind").is_none());
        assert!(body.get("id").is_none());
        assert_eq!(body["target"]["kind"], "transaction");
        assert_eq!(body["target"]["id"], "txn_8f2a");
        // `replace` is not part of DocumentUnassignRequest (additionalProperties: false).
        assert!(body.get("replace").is_none());
    }
}
