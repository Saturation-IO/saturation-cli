//! Typed `/v1` client.
//!
//! ## Generation
//!
//! This module contains generated transport and typed operations with a thin
//! ergonomic layer (`super::commands`) on top. The public OpenAPI 3.1 contract
//! at `openapi/openapi.yaml` is the source of truth.
//!
//! The committed transport keeps builds deterministic and offline. Contract
//! updates must change this module and the OpenAPI snapshot together.
//!
//! The transport implements the spec §5d response model:
//! - success (2xx): the **bare resource** (single) or `{ data, nextCursor? }`
//!   (collection); there is no `success: true` wrapper. Success is keyed off the
//!   HTTP status.
//! - error (non-2xx): `{ success: false, code, message, requestId, fieldErrors? }`
//!   parsed into [`super::error::ApiError`] with the typed `code` preserved.

use std::path::Path;

use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::error::ApiError;
use super::error::ErrorCode;

/// Result of a `/v1` call: either the parsed success body or the typed error.
pub type ApiResult<T> = std::result::Result<T, ApiError>;

/// A single query parameter (`key=value`), already string-encoded. Optional
/// params are dropped by the caller before reaching here.
pub type Query<'a> = &'a [(&'a str, String)];

/// The generated `/v1` transport. Holds the resolved base URL (server + `/v1`)
/// and the bearer token. The token itself determines the workspace, so `/v1`
/// paths never interpolate a workspace id.
pub struct Client {
    http: reqwest::Client,
    /// Server root including the `/v1` suffix, no trailing slash
    /// (e.g. `https://next-api.saturation.io/v1`).
    base_url: String,
    token: String,
}

impl Client {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        let mut base = base_url.into();
        // Normalize: strip trailing slash, ensure a single `/v1` suffix.
        while base.ends_with('/') {
            base.pop();
        }
        if !base.ends_with("/v1") {
            base.push_str("/v1");
        }
        Self {
            // Bound every /v1 round-trip so a black-holed host cannot hang the CLI.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base,
            token: token.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// Core request: send `method path?query` with an optional JSON body and
    /// optional `Idempotency-Key`, then map the response to the §5d model.
    async fn send<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        query: Query<'_>,
        body: Option<&B>,
        idempotency_key: Option<&str>,
        if_none_match: Option<&str>,
    ) -> ApiResult<Value> {
        let mut req = self
            .http
            .request(method, self.url(path))
            .bearer_auth(&self.token);

        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }
        if let Some(etag) = if_none_match {
            req = req.header("If-None-Match", etag);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| transport_error(e.to_string()))?;

        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            // DELETE returns 204 with no body.
            if status == StatusCode::NO_CONTENT
                || status == StatusCode::NOT_MODIFIED
                || bytes.is_empty()
            {
                return Ok(Value::Null);
            }
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|e| transport_error(format!("failed to parse success body: {e}")))
        } else {
            // §5d error envelope. Fall back to a synthetic envelope if the body
            // isn't the documented shape (e.g. a proxy 502) so the typed code is
            // never lost.
            let mut err = serde_json::from_slice::<ApiError>(&bytes).unwrap_or_else(|_| ApiError {
                success: false,
                code: status_to_code(status),
                message: String::from_utf8_lossy(&bytes).trim().to_string(),
                request_id: None,
                field_errors: None,
                required_ability: None,
                retry_after: None,
                http_status: 0,
            });
            err.http_status = status.as_u16();
            Err(err)
        }
    }

    // ── Verb helpers used by every operation below ──────────────────────────

    pub async fn get(&self, path: &str, query: Query<'_>) -> ApiResult<Value> {
        self.send::<()>(Method::GET, path, query, None, None, None)
            .await
    }

    pub async fn get_conditional(
        &self,
        path: &str,
        query: Query<'_>,
        if_none_match: Option<&str>,
    ) -> ApiResult<Value> {
        self.send::<()>(Method::GET, path, query, None, None, if_none_match)
            .await
    }

    pub async fn post<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> ApiResult<Value> {
        self.send(Method::POST, path, &[], Some(body), idempotency_key, None)
            .await
    }

    pub async fn post_with_query<B: Serialize>(
        &self,
        path: &str,
        query: Query<'_>,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> ApiResult<Value> {
        self.send(Method::POST, path, query, Some(body), idempotency_key, None)
            .await
    }

    pub async fn patch<B: Serialize>(&self, path: &str, body: &B) -> ApiResult<Value> {
        self.send(Method::PATCH, path, &[], Some(body), None, None)
            .await
    }

    pub async fn patch_with_query<B: Serialize>(
        &self,
        path: &str,
        query: Query<'_>,
        body: &B,
    ) -> ApiResult<Value> {
        self.send(Method::PATCH, path, query, Some(body), None, None)
            .await
    }

    pub async fn put<B: Serialize>(&self, path: &str, body: &B) -> ApiResult<Value> {
        self.send(Method::PUT, path, &[], Some(body), None, None)
            .await
    }

    pub async fn delete(&self, path: &str) -> ApiResult<Value> {
        self.send::<()>(Method::DELETE, path, &[], None, None, None)
            .await
    }

    pub async fn delete_with_query(&self, path: &str, query: Query<'_>) -> ApiResult<Value> {
        self.send::<()>(Method::DELETE, path, query, None, None, None)
            .await
    }

    pub async fn stream_get<W>(&self, path: &str, query: Query<'_>, writer: &mut W) -> ApiResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut req = self.http.get(self.url(path)).bearer_auth(&self.token);
        if !query.is_empty() {
            req = req.query(query);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| transport_error(e.to_string()))?;
            return Err(parse_error(status, &bytes));
        }

        let mut response = resp;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| transport_error(e.to_string()))?
        {
            writer
                .write_all(&chunk)
                .await
                .map_err(|e| transport_error(e.to_string()))?;
        }
        writer
            .flush()
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        Ok(())
    }

    /// Multipart document upload (`POST /v1/documents`).
    pub async fn upload_document(
        &self,
        file_path: &Path,
        metadata: Value,
        idempotency_key: Option<&str>,
    ) -> ApiResult<Value> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload")
            .to_string();
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| transport_error(format!("failed to read {}: {e}", file_path.display())))?;

        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .map_err(|e| transport_error(e.to_string()))?;
        let meta_part = reqwest::multipart::Part::text(metadata.to_string())
            .mime_str("application/json")
            .map_err(|e| transport_error(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .part("metadata", meta_part);

        let mut req = self
            .http
            .post(self.url("documents"))
            .bearer_auth(&self.token)
            .multipart(form);
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| transport_error(e.to_string()))?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        if status.is_success() {
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|e| transport_error(format!("parse upload response: {e}")))
        } else {
            let mut err = serde_json::from_slice::<ApiError>(&bytes).unwrap_or_else(|_| ApiError {
                success: false,
                code: status_to_code(status),
                message: String::from_utf8_lossy(&bytes).trim().to_string(),
                request_id: None,
                field_errors: None,
                required_ability: None,
                retry_after: None,
                http_status: 0,
            });
            err.http_status = status.as_u16();
            Err(err)
        }
    }

    // ── Path builders (token-scoped + project templating in one place) ──────

    /// Token-scoped resource path from the `/v1` root.
    pub fn ws_path(&self, suffix: &str) -> String {
        suffix.trim_start_matches('/').to_string()
    }

    /// `/projects/{projectId}` prefix. Accepts a project `slug` or canonical id
    /// (both are valid in the path per the spec).
    pub fn project_path(&self, project: &str, suffix: &str) -> String {
        format!("projects/{}/{}", project, suffix.trim_start_matches('/'))
    }
}

fn transport_error(message: String) -> ApiError {
    ApiError {
        success: false,
        code: ErrorCode::InternalError,
        message,
        request_id: None,
        field_errors: None,
        required_ability: None,
        retry_after: None,
        http_status: 0,
    }
}

fn parse_error(status: StatusCode, bytes: &[u8]) -> ApiError {
    let mut err = serde_json::from_slice::<ApiError>(bytes).unwrap_or_else(|_| ApiError {
        success: false,
        code: status_to_code(status),
        message: String::from_utf8_lossy(bytes).trim().to_string(),
        request_id: None,
        field_errors: None,
        required_ability: None,
        retry_after: None,
        http_status: 0,
    });
    err.http_status = status.as_u16();
    err
}

/// Map a bare HTTP status (no documented envelope) to the closest typed code so
/// the CLI's error rendering never falls back to an untyped string.
fn status_to_code(status: StatusCode) -> ErrorCode {
    match status.as_u16() {
        401 => ErrorCode::Unauthenticated,
        403 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::IdempotencyConflict,
        413 => ErrorCode::BudgetTooLarge,
        422 => ErrorCode::Validation,
        429 => ErrorCode::RateLimited,
        504 => ErrorCode::BudgetComputeTimeout,
        _ => ErrorCode::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_v1_suffix() {
        let c = Client::new("https://next-api.saturation.io", "tok");
        assert_eq!(c.base_url, "https://next-api.saturation.io/v1");
        let c2 = Client::new("https://next-api.saturation.io/v1/", "tok");
        assert_eq!(c2.base_url, "https://next-api.saturation.io/v1");
        let c3 = Client::new("http://localhost:4300", "tok");
        assert_eq!(c3.base_url, "http://localhost:4300/v1");
    }

    #[test]
    fn builds_token_scoped_and_project_paths() {
        let c = Client::new("http://localhost:4300/v1", "tok");
        assert_eq!(c.ws_path("documents"), "documents");
        assert_eq!(
            c.project_path("my-film", "budget/lines"),
            "projects/my-film/budget/lines"
        );
    }

    #[test]
    fn maps_status_to_typed_code() {
        assert_eq!(status_to_code(StatusCode::NOT_FOUND), ErrorCode::NotFound);
        assert_eq!(
            status_to_code(StatusCode::TOO_MANY_REQUESTS),
            ErrorCode::RateLimited
        );
        assert_eq!(
            status_to_code(StatusCode::GATEWAY_TIMEOUT),
            ErrorCode::BudgetComputeTimeout
        );
    }
}
