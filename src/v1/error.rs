//! The `/v1` response + error model (spec §5d).
//!
//! Success responses are the **bare resource** (single) or `{ data, nextCursor? }`
//! (collection) — there is no `success: true` wrapper; clients key off the HTTP
//! status. Errors carry `{ success: false, code, message, requestId, fieldErrors? }`
//! with a stable, typed string `code`.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The closed set of stable, typed error codes returned at the `/v1` boundary.
///
/// Mirrors `components.schemas.ErrorCode` in the OpenAPI. Unknown codes are
/// tolerated via [`ErrorCode::Other`] so a server-side addition never crashes
/// an older client, but every documented code is surfaced verbatim (never
/// flattened to a generic string).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthenticated,
    InvalidToken,
    MissingAuthorization,
    TokenRevoked,
    PermissionRevoked,
    ScopeExceeded,
    Forbidden,
    FeatureNotAvailable,
    RoleCeilingExceeded,
    NotFound,
    DocumentTargetNotFound,
    Validation,
    CursorInvalid,
    ExpandInvalid,
    InvalidDateRange,
    WebhookHttpsRequired,
    DocumentInvalidTargetKind,
    RangeTooLarge,
    AccountPathAmbiguous,
    AccountCodeAmbiguous,
    BudgetComputeStale,
    IdempotencyConflict,
    AlreadyAssigned,
    StatusUnreachableForSource,
    PoInvalidStatus,
    WebhookUrlInvalidSsrf,
    FieldReadOnly,
    SourceNotPostable,
    WebhookUrlBlocked,
    DocumentAssignForbidden,
    DocumentCrossWorkspaceTarget,
    BudgetTooLarge,
    BatchTooLarge,
    RateLimited,
    SigningKeyNotConfigured,
    InternalError,
    BudgetComputeTimeout,
    /// A code not in the documented set — surfaced verbatim, never silently dropped.
    #[serde(untagged)]
    Other(String),
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::Other(s) => write!(f, "{s}"),
            other => {
                // Re-serialize the unit variant to its snake_case wire string.
                let v = serde_json::to_value(other).unwrap_or_default();
                write!(f, "{}", v.as_str().unwrap_or("unknown"))
            }
        }
    }
}

/// The uniform `/v1` error envelope (`components.schemas.Error`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// Always `false`; present only on errors. Kept to document the wire shape
    /// (`components.schemas.Error.success`); the client keys off HTTP status.
    #[serde(default)]
    #[allow(dead_code)]
    pub success: bool,
    pub code: ErrorCode,
    pub message: String,
    pub request_id: Option<String>,
    /// Present on validation / mass-assignment failures.
    #[serde(default)]
    pub field_errors: Option<HashMap<String, Vec<String>>>,
    /// Present on `permission_revoked` (403): the missing `action:subject` ability.
    #[serde(default)]
    pub required_ability: Option<String>,
    /// Present on `rate_limited` (429): seconds to wait before retrying.
    #[serde(default)]
    pub retry_after: Option<i64>,
    /// HTTP status the error arrived with (filled in by the client; not on the wire).
    #[serde(skip)]
    pub http_status: u16,
}

impl ApiError {
    /// Render a self-diagnosable, multi-line error for the terminal. The typed
    /// `code` is always preserved — never flattened to a generic string.
    pub fn render(&self) -> String {
        let mut out = format!("[{}] {} ({})", self.code, self.message, self.http_status);
        if let Some(rid) = &self.request_id {
            out.push_str(&format!("\n  request-id: {rid}"));
        }
        if let Some(ability) = &self.required_ability {
            out.push_str(&format!("\n  required-ability: {ability}"));
        }
        if let Some(after) = self.retry_after {
            out.push_str(&format!("\n  retry-after: {after}s"));
        }
        if let Some(fields) = &self.field_errors {
            for (field, msgs) in fields {
                out.push_str(&format!("\n  {field}: {}", msgs.join("; ")));
            }
        }
        out
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render())
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_code() {
        let json = r#"{"success":false,"code":"idempotency_conflict","message":"replayed key","requestId":"req_1"}"#;
        let err: ApiError = serde_json::from_str(json).unwrap();
        assert_eq!(err.code, ErrorCode::IdempotencyConflict);
        assert_eq!(err.request_id.as_deref(), Some("req_1"));
    }

    #[test]
    fn round_trips_code_display() {
        assert_eq!(
            ErrorCode::AccountPathAmbiguous.to_string(),
            "account_path_ambiguous"
        );
        assert_eq!(
            ErrorCode::AccountCodeAmbiguous.to_string(),
            "account_code_ambiguous"
        );
        assert_eq!(ErrorCode::NotFound.to_string(), "not_found");
        assert_eq!(
            ErrorCode::BudgetComputeStale.to_string(),
            "budget_compute_stale"
        );
        assert_eq!(
            ErrorCode::BudgetComputeTimeout.to_string(),
            "budget_compute_timeout"
        );
    }

    #[test]
    fn tolerates_unknown_code_without_flattening() {
        let json = r#"{"success":false,"code":"some_new_code","message":"x","requestId":"req_2"}"#;
        let err: ApiError = serde_json::from_str(json).unwrap();
        assert_eq!(err.code, ErrorCode::Other("some_new_code".into()));
        assert_eq!(err.code.to_string(), "some_new_code");
    }

    #[test]
    fn renders_field_errors() {
        let json = r#"{"success":false,"code":"validation","message":"bad","requestId":"r","fieldErrors":{"types":["unknown kind"]}}"#;
        let err: ApiError = serde_json::from_str(json).unwrap();
        let rendered = err.render();
        assert!(rendered.contains("types: unknown kind"));
        assert!(rendered.contains("validation"));
    }

    #[test]
    fn renders_required_ability_on_permission_revoked() {
        let json = r#"{"success":false,"code":"permission_revoked","message":"no","requestId":"r","requiredAbility":"update:Transaction"}"#;
        let err: ApiError = serde_json::from_str(json).unwrap();
        assert!(err.render().contains("update:Transaction"));
    }
}
