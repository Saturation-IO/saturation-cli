//! clap definitions for the `sat /v1` namespace.
//!
//! One subcommand per `/v1` resource group in the OpenAPI inventory. Filters,
//! `expand` keys and `fields` selectors are expressible as flags (see
//! [`ListFlags`]); every flag maps to a query parameter on the generated client
//! — there are no hidden params and no hand-built JSON bodies (bodies are passed
//! as typed `--data` JSON or built from explicit flags).

use clap::{Args, Subcommand};

/// Shared collection-read flags (pagination, expand, fields, sort, filters).
/// Every field maps 1:1 to a documented `/v1` query parameter.
#[derive(Args, Clone, Debug, Default)]
pub struct ListFlags {
    /// Page size (max 100, default server-side 50).
    #[arg(long)]
    pub limit: Option<u32>,
    /// Keyset pagination cursor from a prior `nextCursor`.
    #[arg(long)]
    pub cursor: Option<String>,
    /// Comma list of relations to inline (`expand=a,b`, depth <= 2).
    #[arg(long)]
    pub expand: Option<String>,
    /// Comma list of fields to project (sparse fieldsets).
    #[arg(long)]
    pub fields: Option<String>,
    /// Sort key.
    #[arg(long)]
    pub sort: Option<String>,
    /// Sort order (`asc` | `desc`).
    #[arg(long)]
    pub order: Option<String>,
    /// Include a total `count` in the response.
    #[arg(long)]
    pub with_count: bool,
    /// Repeatable raw filter `key=value` for any documented query param not
    /// surfaced as a dedicated flag (escape hatch with no hidden params).
    #[arg(long = "filter", value_name = "KEY=VALUE")]
    pub filters: Vec<String>,
}

#[derive(Args)]
pub struct V1Args {
    #[command(subcommand)]
    pub command: V1Command,

    /// Project slug or id (required by project-scoped resources).
    #[arg(long, global = true)]
    pub project: Option<String>,

    /// Optional Idempotency-Key for unsafe creates (safe retries).
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
}

#[derive(Subcommand)]
pub enum V1Command {
    /// Identity bootstrap — `GET /me`.
    Me,
    /// Workspaces the token can act on.
    Workspaces(SimpleList),
    /// Projects (CRUD), addressable by slug or id.
    Projects(ResourceArgs),
    /// Workspace spaces (folders that group projects).
    Spaces(CrudArgs),
    /// Workspace contacts (vendors, crew, payees).
    Contacts(ResourceArgs),
    /// Budget — document, totals, rollup, variance, cells, lines, phase data, phases, accounts.
    Budget(BudgetArgs),
    /// Transactions — unified ledger (source · type · status), items, batch, stats.
    Transactions(TransactionArgs),
    /// Purchase orders — read/write, Activity, Summary, Timeline, suggestions, and actions.
    #[command(name = "purchase-orders", alias = "po")]
    PurchaseOrders(PurchaseOrderArgs),
    /// Requests to pay a person or company.
    #[command(name = "payment-requests")]
    PaymentRequests(PaymentRequestArgs),
    /// Payments from request through settlement, including Timeline.
    Payments(PaymentArgs),
    /// Library — workspace source scope (rates, fringes, globals, currencies, tags, units).
    Library(LibraryArgs),
    /// Project-resident Library — copy-on-use installs/copies (`--project` required).
    #[command(name = "project-library")]
    ProjectLibrary(ProjectLibraryArgs),
    /// Library incentives (enable-only packs + programs).
    Incentives(IncentiveArgs),
    /// Saved views — list / get / data (`--project` required).
    Views(ViewArgs),
    /// Documents — drop once, then assign to typed targets.
    Documents(DocumentArgs),
    /// Spotlight search (workspace + project scope).
    Search(SearchArgs),
    /// Comments on workspace + project entities.
    Comments(CrudArgs),
    /// Outbound webhooks (subscriptions, ping, deliveries).
    Webhooks(WebhookArgs),
    /// Metered usage ledger, credits and operations.
    Usage(UsageArgs),
}

// ── Generic resource (list/get/create/update/delete by id) ──────────────────

#[derive(Args)]
pub struct SimpleList {
    #[command(flatten)]
    pub flags: ListFlags,
}

#[derive(Args)]
pub struct ResourceArgs {
    #[command(subcommand)]
    pub command: ResourceCommand,
}

#[derive(Subcommand)]
pub enum ResourceCommand {
    /// List the collection (keyset-paginated).
    List(ListFlags),
    /// Get a single resource by id (or slug, for projects).
    Get {
        id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create from inline JSON.
    Create {
        /// Resource body as JSON.
        data: String,
    },
    /// Patch a resource by id with inline JSON.
    Update {
        id: String,
        /// Patch body as JSON.
        data: String,
    },
    /// Soft-delete a resource by id.
    Delete { id: String },
}

/// CRUD for a collection whose single-resource path exposes only
/// create/update/delete (no `GET /{id}`) — spaces, comments, and line items.
#[derive(Args)]
pub struct CrudArgs {
    #[command(subcommand)]
    pub command: CrudCommand,
}

#[derive(Subcommand)]
pub enum CrudCommand {
    /// List the collection (keyset-paginated).
    List(ListFlags),
    /// Create from inline JSON.
    Create {
        /// Resource body as JSON.
        data: String,
    },
    /// Patch a resource by id with inline JSON.
    Update {
        id: String,
        /// Patch body as JSON.
        data: String,
    },
    /// Soft-delete a resource by id.
    Delete { id: String },
}

// ── Budget ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct BudgetArgs {
    #[command(subcommand)]
    pub command: BudgetCommand,
}

#[derive(Subcommand)]
pub enum BudgetCommand {
    /// Budget document: lines, visible phases, totals, and editable phase data (`GET /budget`).
    #[command(name = "document", alias = "doc")]
    Document(BudgetDocumentFlags),
    /// Rolled-up totals (`GET /budget/totals`).
    Totals(ListFlags),
    /// Account rollup (`GET /budget/rollup`).
    Rollup(ListFlags),
    /// Estimate-vs-actual variance (`GET /budget/variance`).
    Variance(ListFlags),
    /// Positional cell read (`GET /budget/cells?account=&column=`).
    Cells {
        #[arg(long)]
        account: String,
        #[arg(long)]
        column: String,
    },
    /// Budget lines (list / get / create / update / delete).
    Lines(LineArgs),
    /// Editable line phase data (single upsert / batch upsert).
    #[command(name = "phase-data")]
    PhaseData(BudgetPhaseDataArgs),
    /// Budget phases (list / get / create / update / delete).
    Phases(ResourceCommandWrap),
    /// Budget accounts.
    Accounts(ListFlags),
}

#[derive(Args)]
pub struct BudgetDocumentFlags {
    /// Materialized account path naming one root line and its descendants.
    #[arg(long)]
    pub path: Option<String>,
    /// Leaf account code naming one root line and its descendants.
    #[arg(long = "account-code")]
    pub account_code: Option<String>,
    /// Visible phase id, alias, name, or type.
    #[arg(long)]
    pub phase: Option<String>,
}

#[derive(Args)]
pub struct LineArgs {
    #[command(subcommand)]
    pub command: LineCommand,
}

#[derive(Subcommand)]
pub enum LineCommand {
    /// List lines with filters (`accountId`, `path`, `phase`, `tags`, `kind`, ...).
    List {
        #[arg(long)]
        account_id: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        phase: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[command(flatten)]
        flags: ListFlags,
    },
    Get {
        line_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    Create {
        data: String,
    },
    #[command(name = "create-batch")]
    CreateBatch {
        data: String,
    },
    Update {
        line_id: String,
        data: String,
    },
    Delete {
        line_id: String,
    },
}

#[derive(Args)]
pub struct BudgetPhaseDataArgs {
    #[command(subcommand)]
    pub command: BudgetPhaseDataCommand,
}

#[derive(Subcommand)]
pub enum BudgetPhaseDataCommand {
    /// Upsert one editable phase-data entry for a line and phase.
    Upsert {
        line_id: String,
        phase_id: String,
        data: String,
    },
    /// Upsert many editable phase-data entries in one all-or-nothing batch.
    Batch { data: String },
}

#[derive(Args)]
pub struct ResourceCommandWrap {
    #[command(subcommand)]
    pub command: ResourceCommand,
}

// ── Transactions ──────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct TransactionArgs {
    #[command(subcommand)]
    pub command: TransactionCommand,
}

#[derive(Subcommand)]
pub enum TransactionCommand {
    /// List transactions (`source`/`type`/`status` filters; `source=journal` = legacy actuals).
    List {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[command(flatten)]
        flags: ListFlags,
    },
    Get {
        tx_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    Create {
        data: String,
    },
    Update {
        tx_id: String,
        data: String,
    },
    Delete {
        tx_id: String,
    },
    /// Aggregate stats (`GET /transactions/stats`).
    Stats(ListFlags),
    /// Distinct transaction types (`GET /transactions/types`).
    Types,
    /// Batch upsert (`POST /transactions/batch`) — replaces legacy `/actuals/batch`.
    Batch {
        /// Batch body as JSON array.
        data: String,
    },
    /// Line items on a transaction.
    Items(TxItemArgs),
}

#[derive(Args)]
pub struct TxItemArgs {
    pub tx_id: String,
    #[command(subcommand)]
    pub command: CrudCommand,
}

// ── Purchase orders ──────────────────────────────────────────────────────────

#[derive(Args)]
pub struct PurchaseOrderArgs {
    #[command(subcommand)]
    pub command: PurchaseOrderCommand,
}

#[derive(Subcommand)]
pub enum PurchaseOrderCommand {
    List(ListFlags),
    Get {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    Create {
        data: String,
    },
    Update {
        purchase_order_id: String,
        data: String,
    },
    Delete {
        purchase_order_id: String,
    },
    /// Submit a PO for approval.
    Submit {
        purchase_order_id: String,
    },
    /// Cancel an approval request.
    #[command(name = "cancel-submission")]
    CancelSubmission {
        purchase_order_id: String,
    },
    /// Void a PO.
    Void {
        purchase_order_id: String,
    },
    /// Mark a PO as paid. Requires a linked transaction.
    #[command(name = "mark-paid")]
    MarkPaid {
        purchase_order_id: String,
    },
    /// Link the PO to a target (`{...}` body required).
    Link {
        purchase_order_id: String,
        /// Link body as JSON.
        data: String,
    },
    /// Unlink the PO from a target (`{...}` body required).
    Unlink {
        purchase_order_id: String,
        /// Unlink body as JSON.
        data: String,
    },
    /// Read current live conditions using the product Activity vocabulary.
    Activity {
        purchase_order_id: String,
    },
    /// List records that may belong to the PO but are not linked.
    #[command(name = "suggested-matches")]
    SuggestedMatches {
        purchase_order_id: String,
    },
    /// Read immutable purchase-order history.
    Timeline {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Line items on a PO (list / create / update / delete).
    Items(PoItemArgs),
    /// Transactions linked to a PO (reverse read).
    Transactions {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Documents assigned to a PO (reverse read).
    Documents {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

// ── Payment requests and payments ───────────────────────────────────────────

#[derive(Args)]
pub struct PaymentRequestArgs {
    #[command(subcommand)]
    pub command: PaymentRequestCommand,
}

#[derive(Subcommand)]
pub enum PaymentRequestCommand {
    List(ListFlags),
    Get {
        payment_request_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

#[derive(Args)]
pub struct PaymentArgs {
    #[command(subcommand)]
    pub command: PaymentCommand,
}

#[derive(Subcommand)]
pub enum PaymentCommand {
    List(ListFlags),
    Get {
        payment_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Read payment history, newest first.
    Timeline {
        payment_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

/// PO line-item CRUD, scoped to one purchase order.
#[derive(Args)]
pub struct PoItemArgs {
    /// Purchase order id (`po_…`).
    pub purchase_order_id: String,
    #[command(subcommand)]
    pub command: CrudCommand,
}

// ── Library (workspace source scope) ─────────────────────────────────────────

#[derive(Args)]
pub struct LibraryArgs {
    #[command(subcommand)]
    pub command: LibraryCommand,
}

#[derive(Subcommand)]
pub enum LibraryCommand {
    /// Rate packs (list / get / create / update / delete / enable / disable / items).
    Rates(LibraryRatesArgs),
    /// Fringe templates (CRUD).
    Fringes(LibraryCrudArgs),
    /// Global templates (CRUD).
    Globals(LibraryCrudArgs),
    /// Currency templates (CRUD).
    Currencies(LibraryCrudArgs),
    /// Fringe-tag templates (CRUD).
    #[command(name = "fringe-tags")]
    FringeTags(LibraryCrudArgs),
    /// Tags (CRUD).
    Tags(LibraryCrudArgs),
    /// Units — built-in read + custom-unit CRUD.
    Units(LibraryUnitsArgs),
}

/// Workspace-Library CRUD shared by the template sections (fringes / globals /
/// currencies / fringe-tags / tags). Single-resource ops are PATCH / DELETE; the
/// section root is list / create. No enable/disable (that is rate-packs only).
#[derive(Args)]
pub struct LibraryCrudArgs {
    #[command(subcommand)]
    pub command: LibraryCrudCommand,
}

#[derive(Subcommand)]
pub enum LibraryCrudCommand {
    /// List the section.
    List(ListFlags),
    /// Get one template by id.
    Get {
        id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create a template from inline JSON.
    Create { data: String },
    /// Patch a template by id.
    Update { id: String, data: String },
    /// Delete a template by id.
    Delete { id: String },
}

#[derive(Args)]
pub struct LibraryRatesArgs {
    #[command(subcommand)]
    pub command: LibraryRatesCommand,
}

#[derive(Subcommand)]
pub enum LibraryRatesCommand {
    /// List rate packs.
    List(ListFlags),
    /// Get one rate pack by id.
    Get {
        id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create a rate pack from inline JSON.
    Create { data: String },
    /// Patch a rate pack by id.
    Update { id: String, data: String },
    /// Delete a rate pack by id.
    Delete { id: String },
    /// Enable a pack for the workspace. Safe to repeat.
    Enable { id: String },
    /// Disable a pack at the workspace (`DELETE …/enable`).
    Disable { id: String },
    /// Rate-pack items (list / create / update / delete).
    Items(RatePackItemArgs),
}

/// Items within one rate pack (`/library/rates/{packId}/items`).
#[derive(Args)]
pub struct RatePackItemArgs {
    /// Rate pack id.
    pub pack_id: String,
    #[command(subcommand)]
    pub command: CrudCommand,
}

#[derive(Args)]
pub struct LibraryUnitsArgs {
    #[command(subcommand)]
    pub command: LibraryUnitsCommand,
}

#[derive(Subcommand)]
pub enum LibraryUnitsCommand {
    /// List units (built-in + custom) — `GET /library/units`.
    List(ListFlags),
    /// Custom units (list / create / update / delete).
    Custom(LibraryCustomUnitArgs),
}

#[derive(Args)]
pub struct LibraryCustomUnitArgs {
    #[command(subcommand)]
    pub command: CrudCommand,
}

// ── Project-resident Library ──────────────────────────────────────────────────

#[derive(Args)]
pub struct ProjectLibraryArgs {
    #[command(subcommand)]
    pub command: ProjectLibraryCommand,
}

#[derive(Subcommand)]
pub enum ProjectLibraryCommand {
    /// Project-resident rate packs (list / add / remove).
    Rates(ProjectRateArgs),
    /// Project-resident incentives (list / get / add / update / delete).
    Incentives(ProjectIncentiveArgs),
    /// Project-resident fringe copies (list / get / add / update / delete).
    Fringes(ProjectCopyArgs),
    /// Project-resident global copies (list / get / add / update / delete).
    Globals(ProjectCopyArgs),
    /// Project-resident currency copies (list / get / add / update / delete).
    Currencies(ProjectCopyArgs),
    /// Project-resident fringe-tag copies (list / get / add / update / delete).
    #[command(name = "fringe-tags")]
    FringeTags(ProjectCopyArgs),
    /// Project-associated tags (list / add / remove).
    Tags(ProjectTagArgs),
}

#[derive(Args)]
pub struct ProjectRateArgs {
    #[command(subcommand)]
    pub command: ProjectRateCommand,
}

#[derive(Subcommand)]
pub enum ProjectRateCommand {
    /// List rate packs added to the project.
    List(ListFlags),
    /// Add a workspace-enabled rate pack into the project (`POST …/{packId}/add`, idempotent).
    Add { pack_id: String },
    /// Remove a rate pack from the project (`DELETE …/{packId}/add`, idempotent).
    Remove { pack_id: String },
}

#[derive(Args)]
pub struct ProjectIncentiveArgs {
    #[command(subcommand)]
    pub command: ProjectIncentiveCommand,
}

#[derive(Subcommand)]
pub enum ProjectIncentiveCommand {
    /// List project-resident incentives.
    List(ListFlags),
    /// Get one resident incentive by id.
    Get {
        incentive_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Add an incentive program into the project from inline JSON (`{"programId":"…"}`).
    Add { data: String },
    /// Patch a resident incentive by id.
    Update { incentive_id: String, data: String },
    /// Delete a resident incentive by id.
    Delete { incentive_id: String },
}

/// Shared CRUD for the copy-on-use sections (fringes / globals / currencies /
/// fringe-tags). `add` copies a workspace source by `sourceId`.
#[derive(Args)]
pub struct ProjectCopyArgs {
    #[command(subcommand)]
    pub command: ProjectCopyCommand,
}

#[derive(Subcommand)]
pub enum ProjectCopyCommand {
    /// List resident copies.
    List(ListFlags),
    /// Get one resident copy by id.
    Get {
        id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Copy a workspace source into the project by `sourceId` (idempotent).
    Add {
        /// Workspace source id to copy.
        source_id: String,
    },
    /// Patch a resident copy by id with inline JSON.
    Update { id: String, data: String },
    /// Delete a resident copy by id.
    Delete { id: String },
}

#[derive(Args)]
pub struct ProjectTagArgs {
    #[command(subcommand)]
    pub command: ProjectTagCommand,
}

#[derive(Subcommand)]
pub enum ProjectTagCommand {
    /// List tags associated with the project.
    List(ListFlags),
    /// Add a workspace tag to the project (`POST …/{tagId}/add`; optional inline
    /// `{"tag":{"name":"…","color":"…"}}` to create it).
    Add {
        tag_id: String,
        /// Optional add body as JSON.
        data: Option<String>,
    },
    /// Remove a tag association from the project (`DELETE …/{tagId}/add`).
    Remove { tag_id: String },
}

// ── Saved views ───────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct ViewArgs {
    #[command(subcommand)]
    pub command: ViewCommand,
}

#[derive(Subcommand)]
pub enum ViewCommand {
    /// List the project's saved views.
    List {
        #[arg(long)]
        subject_type: Option<String>,
        #[arg(long)]
        visibility: Option<String>,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Get one view's definition by id.
    Get { view_id: String },
    /// Resolve a view's rows (`expand` / `limit` / `cursor` layer on top).
    Data {
        view_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

// ── Incentives ───────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct IncentiveArgs {
    #[command(subcommand)]
    pub command: IncentiveCommand,
}

#[derive(Subcommand)]
pub enum IncentiveCommand {
    /// List incentive packs.
    List(ListFlags),
    /// Get one pack.
    Get {
        pack_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// List programs in a pack.
    Programs {
        pack_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Enable a pack (enable-only; no project install).
    Enable { pack_id: String },
    /// Disable a pack.
    Disable { pack_id: String },
}

// ── Documents ────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct DocumentArgs {
    #[command(subcommand)]
    pub command: DocumentCommand,
}

#[derive(Subcommand)]
pub enum DocumentCommand {
    /// List documents in the workspace.
    List(ListFlags),
    /// Get document metadata by id.
    Get {
        document_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Drop a file into the workspace (multipart upload).
    Drop {
        /// File path to upload.
        file: String,
        /// Optional classification (invoice, receipt, screenplay, ...).
        #[arg(long)]
        classification: Option<String>,
        /// Optional override name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Patch document metadata.
    Update { document_id: String, data: String },
    /// Soft-delete a document.
    Delete { document_id: String },
    /// Fetch compiled content.
    Content { document_id: String },
    /// Fetch structured extraction.
    Extraction { document_id: String },
    /// Assign a document to a typed target (`{ kind, id }`).
    Assign {
        document_id: String,
        /// Target kind (transaction | budgetLine | contact | purchaseOrder | project).
        #[arg(long)]
        kind: String,
        /// Target id.
        #[arg(long)]
        id: String,
        /// Replace an existing same-kind assignment to a different id (an explicit move)
        /// instead of returning `409 already_assigned`.
        #[arg(long)]
        replace: bool,
    },
    /// Remove an assignment.
    Unassign {
        document_id: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        id: String,
    },
    /// List a document's assignments.
    Assignments { document_id: String },
    /// Reverse read — documents on a project (`--project` required).
    #[command(name = "by-project")]
    ByProject {
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Reverse read — documents on a transaction (`--project` required).
    #[command(name = "by-transaction")]
    ByTransaction {
        tx_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Reverse read — documents on a contact (workspace scope).
    #[command(name = "by-contact")]
    ByContact {
        contact_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

// ── Search ───────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct SearchArgs {
    /// Search query.
    pub query: String,
    /// Comma list of entity kinds to search.
    #[arg(long)]
    pub types: Option<String>,
    #[command(flatten)]
    pub flags: ListFlags,
}

// ── Webhooks ─────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct WebhookArgs {
    #[command(subcommand)]
    pub command: WebhookCommand,
}

#[derive(Subcommand)]
pub enum WebhookCommand {
    List(ListFlags),
    Get {
        webhook_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    Create {
        data: String,
    },
    Update {
        webhook_id: String,
        data: String,
    },
    Delete {
        webhook_id: String,
    },
    /// Send a test event to the endpoint.
    Ping {
        webhook_id: String,
    },
    /// List recent delivery attempts.
    Deliveries {
        webhook_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

// ── Usage ────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct UsageArgs {
    #[command(subcommand)]
    pub command: UsageCommand,
}

#[derive(Subcommand)]
pub enum UsageCommand {
    /// Workspace usage ledger (`GET /usage`).
    Summary(ListFlags),
    /// Credit balance (`GET /usage/credits`).
    Credits,
    /// Metered operations (`GET /usage/operations`).
    Operations(ListFlags),
}
