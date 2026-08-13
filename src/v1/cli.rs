//! clap definitions for Saturation's public resource commands.
//!
//! One subcommand per `/v1` resource group in the OpenAPI inventory. Filters,
//! `expand` keys and `fields` selectors are expressible as flags (see
//! [`ListFlags`]); every flag maps to a query parameter on the generated client
//! without hidden params or hand-built JSON bodies (bodies are passed
//! as typed `--data` JSON or built from explicit flags).

use clap::{Args, Subcommand, ValueEnum};

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
}

#[derive(Args, Clone, Debug, Default)]
pub struct ConditionalListFlags {
    #[command(flatten)]
    pub flags: ListFlags,
    /// Return 304 when the current budget projection has this ETag.
    #[arg(long = "if-none-match")]
    pub if_none_match: Option<String>,
}

#[derive(Subcommand)]
pub enum V1Command {
    /// Show the identity and workspace bound to the current token.
    Whoami,
    /// Manage projects.
    Projects(ProjectArgs),
    /// Workspace spaces (folders that group projects).
    Spaces(CrudArgs),
    /// Workspace contacts (vendors, crew, payees).
    Contacts(ResourceArgs),
    /// Read and update budgets, lines, phases, and totals.
    Budget(BudgetArgs),
    /// Manage transactions and their items.
    Transactions(TransactionArgs),
    /// Manage purchase orders and their items.
    #[command(name = "purchase-orders")]
    PurchaseOrders(PurchaseOrderArgs),
    /// Requests to pay a person or company.
    #[command(name = "payment-requests")]
    PaymentRequests(PaymentRequestArgs),
    /// Manage payments from request through settlement.
    Payments(PaymentArgs),
    /// Manage workspace Library resources.
    Library(LibraryArgs),
    /// Upload, read, and link documents.
    Documents(DocumentArgs),
    /// Search the workspace or one project.
    Search(SearchArgs),
    /// Manage webhooks and inspect deliveries.
    Webhooks(WebhookArgs),
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

#[derive(Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// List projects.
    List(ListFlags),
    /// Get one project by id or slug.
    Get {
        id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create a project from inline JSON.
    Create { data: String },
    /// Update a project from inline JSON.
    Update { id: String, data: String },
    /// Manage comments in one project.
    Comments(ProjectCommentArgs),
}

#[derive(Args)]
pub struct ProjectCommentArgs {
    /// Project id or slug.
    pub project_id: String,
    #[command(subcommand)]
    pub command: CrudCommand,
}

/// CRUD for a collection whose single-resource path exposes only
/// create/update/delete (no `GET /{id}`): spaces, comments, and line items.
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
    /// Read the budget with its lines, phases, and totals.
    Get(BudgetDocumentFlags),
    /// Read totals by phase.
    #[command(name = "phase-totals")]
    PhaseTotals(ConditionalListFlags),
    /// Budget lines (list / get / create / update / delete).
    Lines(LineArgs),
    /// Editable values for Budget Lines by Phase.
    #[command(name = "phase-data")]
    PhaseData(BudgetPhaseDataArgs),
    /// Budget phases (list / get / create / update / delete).
    Phases(ResourceCommandWrap),
}

#[derive(Args)]
pub struct BudgetDocumentFlags {
    /// Materialized account path naming one root line and its descendants.
    #[arg(long)]
    pub path: Option<String>,
    /// Leaf account code naming one root line and its descendants.
    #[arg(long = "account-code")]
    pub account_code: Option<String>,
    /// Account id naming one root line and its descendants.
    #[arg(long)]
    pub account_id: Option<String>,
    /// Visible phase id, alias, name, or type.
    #[arg(long)]
    pub phase: Option<String>,
    /// Comma-separated tag ids.
    #[arg(long)]
    pub tags: Option<String>,
    /// Match any or all supplied tags.
    #[arg(long)]
    pub tag_mode: Option<String>,
    /// Earliest included date.
    #[arg(long)]
    pub date_from: Option<String>,
    /// Latest included date.
    #[arg(long)]
    pub date_to: Option<String>,
    /// Include hidden phases.
    #[arg(long)]
    pub include_hidden_phases: bool,
    /// Comma-separated relations to inline.
    #[arg(long)]
    pub expand: Option<String>,
    /// Return 304 when the current budget projection has this ETag.
    #[arg(long = "if-none-match")]
    pub if_none_match: Option<String>,
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
    /// Get one budget line by id.
    Get {
        line_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create one budget line from inline JSON.
    Create { data: String },
    /// Create several budget lines in one request.
    Bulk { data: String },
    /// Update one budget line from inline JSON.
    Update { line_id: String, data: String },
    /// Delete one budget line.
    Delete {
        line_id: String,
        /// Reset dependent line references while deleting.
        #[arg(long)]
        reset: bool,
    },
}

#[derive(Args)]
pub struct BudgetPhaseDataArgs {
    #[command(subcommand)]
    pub command: BudgetPhaseDataCommand,
}

#[derive(Subcommand)]
pub enum BudgetPhaseDataCommand {
    /// Set one editable Phase value for a Budget Line.
    Set {
        line_id: String,
        phase_id: String,
        data: String,
    },
    /// Set many Budget Line Phase values in one all-or-nothing request.
    Bulk { data: String },
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
    /// List transactions with source, type, and status filters.
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
    /// Get one transaction by id.
    Get {
        tx_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create one transaction from inline JSON.
    Create { data: String },
    /// Update one transaction from inline JSON.
    Update { tx_id: String, data: String },
    /// Delete one transaction.
    Delete { tx_id: String },
    /// Read transaction totals.
    Stats(ListFlags),
    /// Create or update transactions in one request.
    Bulk {
        /// Bulk body as JSON array.
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
    /// List purchase orders.
    List(ListFlags),
    /// Get one purchase order by id.
    Get {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create one purchase order from inline JSON.
    Create {
        data: String,
        /// Comma-separated relations to inline in the response.
        #[arg(long)]
        expand: Option<String>,
    },
    /// Update one purchase order from inline JSON.
    Update {
        purchase_order_id: String,
        data: String,
        /// Comma-separated relations to inline in the response.
        #[arg(long)]
        expand: Option<String>,
    },
    /// Delete one purchase order.
    Delete { purchase_order_id: String },
    /// Submit a PO for approval.
    Submit { purchase_order_id: String },
    /// Cancel an approval request.
    #[command(name = "cancel-submission")]
    CancelSubmission { purchase_order_id: String },
    /// Void a PO.
    Void { purchase_order_id: String },
    /// Mark a PO as paid. Requires a linked transaction.
    #[command(name = "mark-paid")]
    MarkPaid { purchase_order_id: String },
    /// Link a transaction to the PO.
    #[command(name = "link-transaction")]
    LinkTransaction {
        purchase_order_id: String,
        transaction_id: String,
    },
    /// Unlink a transaction from the PO.
    #[command(name = "unlink-transaction")]
    UnlinkTransaction {
        purchase_order_id: String,
        transaction_id: String,
    },
    /// Read immutable purchase-order history.
    Timeline {
        purchase_order_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Line items on a PO (list / create / update / delete).
    Items(PoItemArgs),
}

// ── Payment requests and payments ───────────────────────────────────────────

#[derive(Args)]
pub struct PaymentRequestArgs {
    #[command(subcommand)]
    pub command: PaymentRequestCommand,
}

#[derive(Subcommand)]
pub enum PaymentRequestCommand {
    /// List payment requests.
    List(ListFlags),
    /// Get one payment request by id.
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
    /// List payments.
    List(ListFlags),
    /// Get one payment by id.
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
    /// Purchase order id (`po_ID`).
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
    #[command(name = "rate-packs")]
    RatePacks(LibraryRatesArgs),
    /// Fringe templates (CRUD).
    Fringes(LibraryCrudArgs),
    /// Global templates (CRUD).
    Globals(LibraryCrudArgs),
    /// Currency templates (CRUD).
    Currencies(LibraryCrudArgs),
    /// Fringe groups (CRUD).
    #[command(name = "fringe-groups")]
    FringeGroups(LibraryCrudArgs),
    /// Tags (CRUD).
    Tags(LibraryCrudArgs),
    /// Manage units.
    Units(LibraryUnitsArgs),
    /// Manage workspace incentive packs and programs.
    Incentives(IncentiveArgs),
    /// Manage Library resources copied into one project.
    Project(ProjectLibraryArgs),
}

/// Workspace-Library CRUD shared by the template sections (fringes / globals /
/// currencies / fringe-groups / tags). Single-resource ops are PATCH / DELETE; the
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
    /// Disable a workspace rate pack.
    Disable { id: String },
    /// Rate-pack items (list / create / update / delete).
    Items(RatePackItemArgs),
}

/// Items within one rate pack.
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
    /// List units.
    List(ListFlags),
    /// Get one unit.
    Get { unit_id: String },
    /// Create a unit.
    Create { data: String },
    /// Update a unit.
    Update { unit_id: String, data: String },
    /// Delete a unit.
    Delete { unit_id: String },
}

// ── Project-resident Library ──────────────────────────────────────────────────

#[derive(Args)]
pub struct ProjectLibraryArgs {
    #[command(subcommand)]
    pub command: ProjectLibraryCommand,
}

#[derive(Subcommand)]
pub enum ProjectLibraryCommand {
    /// Project-resident incentives (list / get / add / update / delete).
    Incentives(ProjectIncentiveArgs),
    /// Project-resident fringe copies (list / get / add / update / delete).
    Fringes(ProjectCopyArgs),
    /// Project-resident global copies (list / get / add / update / delete).
    Globals(ProjectCopyArgs),
    /// Project-resident currency copies (list / get / add / update / delete).
    Currencies(ProjectCopyArgs),
    /// Project-resident Fringe Group copies (list / get / add / update / delete).
    #[command(name = "fringe-groups")]
    FringeGroups(ProjectCopyArgs),
    /// Project-associated tags (list / add / remove).
    Tags(ProjectTagArgs),
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
    /// Add an incentive program from inline JSON (`{"programId":"PROGRAM_ID"}`).
    Add { data: String },
    /// Patch a resident incentive by id.
    Update { incentive_id: String, data: String },
    /// Delete a resident incentive by id.
    Delete { incentive_id: String },
}

/// Shared CRUD for the copy-on-use sections (fringes / globals / currencies /
/// fringe-groups). `add` copies a workspace source by `sourceId`.
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
        /// Reset a diverged resident copy to the workspace source.
        #[arg(long)]
        reset: bool,
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
    /// Upload a file into the workspace.
    Upload {
        /// File path to upload.
        file: String,
        /// Optional override name.
        #[arg(long)]
        name: Option<String>,
        /// Optional free-text description.
        #[arg(long)]
        description: Option<String>,
        /// Folder id for the uploaded document.
        #[arg(long)]
        folder_id: Option<String>,
    },
    /// Patch document metadata.
    Update { document_id: String, data: String },
    /// Soft-delete a document.
    Delete { document_id: String },
    /// Write the original document bytes to standard output.
    Content { document_id: String },
    /// Fetch structured extraction.
    Extraction { document_id: String },
    /// Link a document to a public resource.
    Link {
        document_id: String,
        kind: DocumentLinkKind,
        target_id: String,
        /// Replace a different Link of the same kind.
        #[arg(long)]
        replace: bool,
    },
    /// Remove one typed Link from a document.
    Unlink {
        document_id: String,
        kind: DocumentLinkKind,
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
    /// List webhook subscriptions.
    List(ListFlags),
    /// Get one webhook subscription by id.
    Get {
        webhook_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
    /// Create a webhook subscription from inline JSON.
    Create { data: String },
    /// Update a webhook subscription from inline JSON.
    Update { webhook_id: String, data: String },
    /// Delete a webhook subscription.
    Delete { webhook_id: String },
    /// Send a test event to the endpoint.
    #[command(name = "test-delivery")]
    TestDelivery { webhook_id: String },
    /// List recent delivery attempts for a webhook.
    Deliveries {
        webhook_id: String,
        #[command(flatten)]
        flags: ListFlags,
    },
}

#[derive(Clone, ValueEnum)]
pub enum DocumentLinkKind {
    Transaction,
    Contact,
    PurchaseOrder,
    Project,
}

impl DocumentLinkKind {
    pub fn api_name(&self) -> &'static str {
        match self {
            Self::Transaction => "transaction",
            Self::Contact => "contact",
            Self::PurchaseOrder => "purchaseOrder",
            Self::Project => "project",
        }
    }
}
