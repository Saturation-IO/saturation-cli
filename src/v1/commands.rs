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
    q
}

/// Parse an inline JSON argument into a `serde_json::Value` for a request body.
fn parse_json(data: &str) -> Result<Value> {
    serde_json::from_str(data).map_err(|e| anyhow::anyhow!("invalid JSON body: {e}"))
}

fn document_link_body(target_id: &str, replace: bool) -> Value {
    serde_json::json!({ "targetId": target_id, "replace": replace })
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
                // exit non-zero, bypassing anyhow's "Error: " Debug wrapper so
                // scripts can parse `code`, `message`, and `fieldErrors`.
                let json = serde_json::to_string(&err).unwrap_or_else(|_| err.render());
                eprintln!("{json}");
                std::process::exit(1);
            }
            Err(anyhow::anyhow!("{}", err.render()))
        }
    }
}

fn render_stream(result: ApiResult<()>, output: &Output) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            if matches!(output.format, OutputFormat::Json) {
                let json = serde_json::to_string(&err).unwrap_or_else(|_| err.render());
                eprintln!("{json}");
                std::process::exit(1);
            }
            Err(anyhow::anyhow!("{}", err.render()))
        }
    }
}

fn requires_idempotency_key(command: &V1Command) -> bool {
    match command {
        V1Command::Projects(ProjectArgs {
            command:
                ProjectCommand::Create { .. }
                | ProjectCommand::Comments(ProjectCommentArgs {
                    command: CrudCommand::Create { .. },
                    ..
                }),
        })
        | V1Command::Contacts(ResourceArgs {
            command: ResourceCommand::Create { .. },
        })
        | V1Command::Spaces(CrudArgs {
            command: CrudCommand::Create { .. },
        }) => true,
        V1Command::Budget(BudgetArgs { command }) => matches!(
            command,
            BudgetCommand::Lines(LineArgs {
                command: LineCommand::Create { .. } | LineCommand::Bulk { .. }
            }) | BudgetCommand::PhaseData(BudgetPhaseDataArgs {
                command: BudgetPhaseDataCommand::Bulk { .. }
            }) | BudgetCommand::Phases(ResourceCommandWrap {
                command: ResourceCommand::Create { .. }
            })
        ),
        V1Command::Transactions(TransactionArgs { command }) => matches!(
            command,
            TransactionCommand::Create { .. }
                | TransactionCommand::Bulk { .. }
                | TransactionCommand::Items(TxItemArgs {
                    command: CrudCommand::Create { .. },
                    ..
                })
        ),
        V1Command::PurchaseOrders(PurchaseOrderArgs { command }) => matches!(
            command,
            PurchaseOrderCommand::Create { .. }
                | PurchaseOrderCommand::Items(PoItemArgs {
                    command: CrudCommand::Create { .. },
                    ..
                })
        ),
        V1Command::Library(LibraryArgs { command }) => match command {
            LibraryCommand::RatePacks(LibraryRatesArgs { command }) => matches!(
                command,
                LibraryRatesCommand::Create { .. }
                    | LibraryRatesCommand::Items(RatePackItemArgs {
                        command: CrudCommand::Create { .. },
                        ..
                    })
            ),
            LibraryCommand::Fringes(LibraryCrudArgs { command })
            | LibraryCommand::Globals(LibraryCrudArgs { command })
            | LibraryCommand::Currencies(LibraryCrudArgs { command })
            | LibraryCommand::FringeGroups(LibraryCrudArgs { command })
            | LibraryCommand::Tags(LibraryCrudArgs { command }) => {
                matches!(command, LibraryCrudCommand::Create { .. })
            }
            LibraryCommand::Units(LibraryUnitsArgs {
                command: LibraryUnitsCommand::Create { .. },
            }) => true,
            LibraryCommand::Incentives(_) | LibraryCommand::Project(_) => false,
            LibraryCommand::Units(_) => false,
        },
        _ => false,
    }
}

fn supports_project_scope(command: &V1Command) -> bool {
    matches!(
        command,
        V1Command::Budget(_)
            | V1Command::Library(LibraryArgs {
                command: LibraryCommand::Project(_),
            })
            | V1Command::Search(_)
            | V1Command::Transactions(_)
            | V1Command::PurchaseOrders(_)
            | V1Command::PaymentRequests(_)
            | V1Command::Payments(_)
    )
}

pub async fn execute(
    command: V1Command,
    project: Option<String>,
    idempotency_key: Option<String>,
    client: &Client,
    output: &Output,
) -> Result<()> {
    let idem = idempotency_key.as_deref();
    if requires_idempotency_key(&command) && idem.is_none() {
        anyhow::bail!("this create requires --idempotency-key <KEY>");
    }
    if project.is_some() && !supports_project_scope(&command) {
        anyhow::bail!("--project is not valid for this command");
    }
    let require_project = || -> Result<String> {
        project.clone().ok_or_else(|| {
            anyhow::anyhow!("this resource is project-scoped; pass --project <slug|id>")
        })
    };

    match command {
        // ── Meta / identity ──────────────────────────────────────────────────
        V1Command::Whoami => render(client.get("me", &[]).await, output),

        // ── Generic resources ────────────────────────────────────────────────
        V1Command::Projects(a) => projects(a.command, client, output, idem).await,
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
        V1Command::Library(a) => library(a.command, client, output, idem, project.as_deref()).await,

        // ── Documents ───────────────────────────────────────────────────────────
        V1Command::Documents(a) => documents(a.command, client, output, idem).await,

        // ── Search ──────────────────────────────────────────────────────────────
        V1Command::Search(a) => {
            let q = build_query(
                &a.flags,
                vec![
                    ("q", Some(a.query)),
                    ("types", a.types),
                    ("projectId", project),
                ],
            );
            render(client.get(&client.ws_path("search"), &q).await, output)
        }

        // ── Webhooks ──────────────────────────────────────────────────────────────
        V1Command::Webhooks(a) => webhooks(a.command, client, output, idem).await,
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

async fn projects(
    cmd: ProjectCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("projects");
    match cmd {
        ProjectCommand::List(flags) => render(
            client.get(&base, &build_query(&flags, vec![])).await,
            output,
        ),
        ProjectCommand::Get { id, flags } => render(
            client
                .get(&format!("{base}/{id}"), &build_query(&flags, vec![]))
                .await,
            output,
        ),
        ProjectCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        ProjectCommand::Update { id, data } => render(
            client
                .patch(&format!("{base}/{id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        ProjectCommand::Comments(args) => {
            project_comments(args.command, client, output, idem, &args.project_id).await
        }
    }
}

async fn project_comments(
    cmd: CrudCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
    project: &str,
) -> Result<()> {
    let base = client.project_path(project, "comments");
    match cmd {
        CrudCommand::List(flags) => render(
            client.get(&base, &build_query(&flags, vec![])).await,
            output,
        ),
        CrudCommand::Create { data } => {
            render(client.post(&base, &parse_json(&data)?, idem).await, output)
        }
        CrudCommand::Update { id, data } => render(
            client
                .patch(&format!("{base}/{id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        CrudCommand::Delete { id } => render(client.delete(&format!("{base}/{id}")).await, output),
    }
}

/// CRUD for a collection whose single-resource path has no `GET /{id}` (spaces,
/// comments, and line items): list / create / update / delete only.
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
        BudgetCommand::Get(f) => {
            let q = vec![
                ("path", f.path),
                ("accountCode", f.account_code),
                ("accountId", f.account_id),
                ("phase", f.phase),
                ("tags", f.tags),
                ("tagMode", f.tag_mode),
                ("dateFrom", f.date_from),
                ("dateTo", f.date_to),
                (
                    "includeHiddenPhases",
                    f.include_hidden_phases.then(|| "true".to_string()),
                ),
                ("expand", f.expand),
            ]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect::<Vec<_>>();
            render(
                client
                    .get_conditional(&base("budget"), &q, f.if_none_match.as_deref())
                    .await,
                output,
            )
        }
        BudgetCommand::PhaseTotals(f) => render(
            client
                .get_conditional(
                    &base("budget/totals"),
                    &build_query(&f.flags, vec![]),
                    f.if_none_match.as_deref(),
                )
                .await,
            output,
        ),
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
        LineCommand::Bulk { data } => render(
            client
                .post(&format!("{base}/bulk"), &parse_json(&data)?, idem)
                .await,
            output,
        ),
        LineCommand::Update { line_id, data } => render(
            client
                .patch(&format!("{base}/{line_id}"), &parse_json(&data)?)
                .await,
            output,
        ),
        LineCommand::Delete { line_id, reset } => {
            let q = reset
                .then(|| ("reset", "true".to_string()))
                .into_iter()
                .collect::<Vec<_>>();
            render(
                client
                    .delete_with_query(&format!("{base}/{line_id}"), &q)
                    .await,
                output,
            )
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
        BudgetPhaseDataCommand::Set {
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
        BudgetPhaseDataCommand::Bulk { data } => render(
            client
                .post(
                    &format!("{base}/lines/phase-data/bulk"),
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
        TransactionCommand::Bulk { data } => render(
            client
                .post(&base("transactions/bulk"), &parse_json(&data)?, idem)
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
        PurchaseOrderCommand::Create { data, expand } => {
            let q = expand
                .into_iter()
                .map(|value| ("expand", value))
                .collect::<Vec<_>>();
            render(
                client
                    .post_with_query(&base(pos), &q, &parse_json(&data)?, idem)
                    .await,
                output,
            )
        }
        PurchaseOrderCommand::Update {
            purchase_order_id,
            data,
            expand,
        } => {
            let q = expand
                .into_iter()
                .map(|value| ("expand", value))
                .collect::<Vec<_>>();
            render(
                client
                    .patch_with_query(
                        &base(&format!("{pos}/{purchase_order_id}")),
                        &q,
                        &parse_json(&data)?,
                    )
                    .await,
                output,
            )
        }
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
        PurchaseOrderCommand::LinkTransaction {
            purchase_order_id,
            transaction_id,
        } => render(
            client
                .put(
                    &base(&format!(
                        "{pos}/{purchase_order_id}/transactions/{transaction_id}"
                    )),
                    &Value::Null,
                )
                .await,
            output,
        ),
        PurchaseOrderCommand::UnlinkTransaction {
            purchase_order_id,
            transaction_id,
        } => render(
            client
                .delete(&base(&format!(
                    "{pos}/{purchase_order_id}/transactions/{transaction_id}"
                )))
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
    project: Option<&str>,
) -> Result<()> {
    match cmd {
        LibraryCommand::RatePacks(a) => library_rates(a.command, client, output, idem).await,
        LibraryCommand::Fringes(a) => {
            library_crud(a.command, client, output, idem, "fringes").await
        }
        LibraryCommand::Globals(a) => {
            library_crud(a.command, client, output, idem, "globals").await
        }
        LibraryCommand::Currencies(a) => {
            library_crud(a.command, client, output, idem, "currencies").await
        }
        LibraryCommand::FringeGroups(a) => {
            library_crud(a.command, client, output, idem, "fringe-groups").await
        }
        LibraryCommand::Tags(a) => library_crud(a.command, client, output, idem, "tags").await,
        LibraryCommand::Units(a) => library_units(a.command, client, output, idem).await,
        LibraryCommand::Incentives(a) => incentives(a.command, client, output).await,
        LibraryCommand::Project(a) => {
            let project = project.ok_or_else(|| {
                anyhow::anyhow!("project Library tasks require --project <slug|id>")
            })?;
            project_library(a.command, client, output, idem, project).await
        }
    }
}

/// CRUD for the workspace-Library template sections (fringes / globals /
/// currencies / fringe-groups / tags): list / get / create / update / delete.
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
    let base = client.ws_path("library/rate-packs");
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
                .post(&format!("{base}/{id}/enablement"), &Value::Null, idem)
                .await,
            output,
        ),
        LibraryRatesCommand::Disable { id } => render(
            client.delete(&format!("{base}/{id}/enablement")).await,
            output,
        ),
        LibraryRatesCommand::Items(item) => {
            let item_base = format!("{base}/{}/items", item.pack_id);
            crud_resource(item.command, client, output, idem, &item_base).await
        }
    }
}

/// Workspace units.
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
        LibraryUnitsCommand::Get { unit_id } => render(
            client
                .get(
                    &format!("{}/{unit_id}", client.ws_path("library/units")),
                    &[],
                )
                .await,
            output,
        ),
        LibraryUnitsCommand::Create { data } => render(
            client
                .post(&client.ws_path("library/units"), &parse_json(&data)?, idem)
                .await,
            output,
        ),
        LibraryUnitsCommand::Update { unit_id, data } => render(
            client
                .patch(
                    &format!("{}/{unit_id}", client.ws_path("library/units")),
                    &parse_json(&data)?,
                )
                .await,
            output,
        ),
        LibraryUnitsCommand::Delete { unit_id } => render(
            client
                .delete(&format!("{}/{unit_id}", client.ws_path("library/units")))
                .await,
            output,
        ),
    }
}

async fn incentives(cmd: IncentiveCommand, client: &Client, output: &Output) -> Result<()> {
    let base = client.ws_path("library/incentive-packs");
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
                .post(&format!("{base}/{pack_id}/enablement"), &Value::Null, None)
                .await,
            output,
        ),
        IncentiveCommand::Disable { pack_id } => render(
            client.delete(&format!("{base}/{pack_id}/enablement")).await,
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
        ProjectLibraryCommand::RatePacks(a) => {
            let base = |s: &str| client.project_path(project, &format!("library/rate-packs/{s}"));
            match a.command {
                ProjectRateCommand::List(f) => render(
                    client
                        .get(
                            &client.project_path(project, "library/rate-packs"),
                            &build_query(&f, vec![]),
                        )
                        .await,
                    output,
                ),
                ProjectRateCommand::Add { pack_id } => {
                    render(client.put(&base(&pack_id), &Value::Null).await, output)
                }
                ProjectRateCommand::Remove { pack_id } => {
                    render(client.delete(&base(&pack_id)).await, output)
                }
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
                ProjectIncentiveCommand::Add { data } => {
                    render(client.post(&coll, &parse_json(&data)?, idem).await, output)
                }
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
        ProjectLibraryCommand::FringeGroups(a) => {
            project_library_copy(a.command, client, output, idem, project, "fringe-groups").await
        }
        ProjectLibraryCommand::Tags(a) => {
            let coll = client.project_path(project, "library/tags");
            match a.command {
                ProjectTagCommand::List(f) => {
                    render(client.get(&coll, &build_query(&f, vec![])).await, output)
                }
            }
        }
    }
}

/// CRUD for the copy-on-use project-Library sections (fringes / globals /
/// currencies / fringe-groups). `add` posts `{ sourceId }` to the section.
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
        ProjectCopyCommand::Add { source_id, reset } => {
            let body = serde_json::json!({ "sourceId": source_id });
            let q = reset
                .then(|| ("reset", "true".to_string()))
                .into_iter()
                .collect::<Vec<_>>();
            render(client.post_with_query(&coll, &q, &body, idem).await, output)
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

async fn documents(
    cmd: DocumentCommand,
    client: &Client,
    output: &Output,
    idem: Option<&str>,
) -> Result<()> {
    let base = client.ws_path("documents");
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
        DocumentCommand::Upload {
            file,
            name,
            description,
            folder_id,
        } => {
            let mut meta = serde_json::Map::new();
            if let Some(n) = name {
                meta.insert("name".into(), Value::String(n));
            }
            if let Some(description) = description {
                meta.insert("description".into(), Value::String(description));
            }
            if let Some(folder_id) = folder_id {
                meta.insert("folderId".into(), Value::String(folder_id));
            }
            let result = client
                .upload_document(std::path::Path::new(&file), Value::Object(meta), idem)
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
        DocumentCommand::Content { document_id } => {
            let mut stdout = tokio::io::stdout();
            render_stream(
                client
                    .stream_get(&format!("{base}/{document_id}/content"), &[], &mut stdout)
                    .await,
                output,
            )
        }
        DocumentCommand::Extraction { document_id } => render(
            client
                .get(&format!("{base}/{document_id}/extraction"), &[])
                .await,
            output,
        ),
        DocumentCommand::Link {
            document_id,
            kind,
            target_id,
            replace,
        } => render(
            client
                .put(
                    &format!("{base}/{document_id}/links/{}", kind.api_name()),
                    &document_link_body(&target_id, replace),
                )
                .await,
            output,
        ),
        DocumentCommand::Unlink { document_id, kind } => render(
            client
                .delete(&format!("{base}/{document_id}/links/{}", kind.api_name()))
                .await,
            output,
        ),
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
        WebhookCommand::TestDelivery { webhook_id } => render(
            client
                .post(
                    &format!("{base}/{webhook_id}/test-delivery"),
                    &Value::Null,
                    None,
                )
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
    fn link_body_uses_target_id_and_replace_only() {
        let body = document_link_body("txn_8f2a", true);
        assert_eq!(body["targetId"], "txn_8f2a");
        assert_eq!(body["replace"], true);
        assert!(body.get("kind").is_none());
        assert!(body.get("id").is_none());
        assert!(body.get("target").is_none());
    }
}
