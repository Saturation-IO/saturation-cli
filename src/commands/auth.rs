use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cli::{AuthArgs, AuthCommand};
use crate::config::{Config, TokenInfo, UserInfo, WorkspaceInfo};
use crate::output::Output;

pub async fn execute(args: AuthArgs, output: &Output) -> Result<()> {
    match args.command {
        AuthCommand::Token {
            token,
            server,
            workspace,
            email,
            user_id,
        } => inject_token(token, server, workspace, email, user_id, output),
        AuthCommand::Logout => logout(output),
        AuthCommand::Status => status(output),
    }
}

fn inject_token(
    token: String,
    server: Option<String>,
    workspace: Option<String>,
    email: Option<String>,
    user_id: Option<String>,
    output: &Output,
) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();

    // Personal API tokens are opaque. JWT claims are decoded only to improve
    // local sandbox status output; the server remains the trust boundary.
    let claims = decode_jwt_payload(&token).unwrap_or_default();
    let sub = user_id.or_else(|| claims.get("sub").and_then(|v| v.as_str()).map(String::from));
    let email = email.or_else(|| {
        claims
            .get("email")
            .and_then(|v| v.as_str())
            .map(String::from)
    });
    let name = claims
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            let first = claims
                .get("firstName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let last = claims
                .get("lastName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let full = format!("{first} {last}").trim().to_string();
            if full.is_empty() {
                None
            } else {
                Some(full)
            }
        });

    // Extract expiration
    let exp = claims
        .get("exp")
        .and_then(|v| v.as_i64())
        .map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| ts.to_string())
        })
        .unwrap_or_default();

    config.token = Some(TokenInfo {
        access_token: token,
        refresh_token: String::new(),
        expires_at: exp,
    });

    if let Some(email) = &email {
        config.user = Some(UserInfo {
            id: sub.clone().unwrap_or_default(),
            email: email.clone(),
            name,
        });
    }

    if let Some(server) = server {
        config.server = Some(server.trim_end_matches('/').to_string());
    }

    if let Some(ws_id) = &workspace {
        config.active_workspace = Some(ws_id.clone());
        if !config.workspaces.contains_key(ws_id) {
            config.workspaces.insert(
                ws_id.clone(),
                WorkspaceInfo {
                    name: ws_id.clone(),
                    role: "token".into(),
                },
            );
        }
    }

    config.save()?;

    if let Some(email) = &email {
        output.success(&format!("Token set for {email}"));
    } else {
        output.success("Token set");
    }

    if let Some(ws) = &workspace {
        output.status("Workspace", ws);
    }

    output.status("Server", config.server_url());
    output.status(
        "Expires",
        config
            .token
            .as_ref()
            .map(|t| t.expires_at.as_str())
            .unwrap_or("unknown"),
    );

    Ok(())
}

// ─── Token refresh ──────────────────────────────────────────────────────────────
//
// Older desktop-issued configs may contain a refresh token. Keep those working
// through the existing native token route without advertising a second login
// model for new CLI installs.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

/// Refresh the token on `config` in place. Errors clearly when there is no
/// refresh token.
pub async fn refresh_in_place(config: &mut Config) -> Result<()> {
    let token = config.require_token()?.clone();
    if !token.is_refreshable() {
        anyhow::bail!(
            "this token cannot be refreshed (no refresh token).\n\
             Personal API tokens and injected JWTs do not need refresh.\n\
             Replace the stored token if it has been revoked."
        );
    }
    let server = config.server_url().trim_end_matches('/').to_string();
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{server}/api/auth/desktop/refresh"))
        .json(&serde_json::json!({ "refreshToken": token.refresh_token }))
        .send()
        .await
        .context("failed to reach token-refresh endpoint")?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!(
            "token-refresh endpoint is not available on this server.\n\
             Replace the stored token under Settings > Developers > API."
        );
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("token refresh failed: HTTP {status}: {body}");
    }

    let refreshed: RefreshResponse = resp
        .json()
        .await
        .context("failed to parse refresh response")?;
    config.token = Some(TokenInfo {
        access_token: refreshed.access_token,
        // Reuse the old refresh token if the server did not rotate it.
        refresh_token: refreshed.refresh_token.unwrap_or(token.refresh_token),
        expires_at: refreshed.expires_at.unwrap_or_default(),
    });
    Ok(())
}

/// Called before authenticated commands: if the stored token is expired and
/// refreshable, transparently refresh and persist it. Non-refreshable expired
/// tokens fall through (the server will reject with a clear 401).
pub async fn ensure_fresh_token(config: &mut Config) -> Result<()> {
    let needs_refresh = config
        .token
        .as_ref()
        .map(|t| t.is_expired() && t.is_refreshable())
        .unwrap_or(false);
    if needs_refresh {
        refresh_in_place(config).await?;
        config.save()?;
    }
    Ok(())
}

/// Decode JWT payload without verification (base64url decode the middle segment).
fn decode_jwt_payload(jwt: &str) -> Option<serde_json::Value> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn logout(output: &Output) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    config.token = None;
    config.user = None;
    config.save()?;
    output.success("Logged out");
    Ok(())
}

fn status(output: &Output) -> Result<()> {
    let config = Config::load()?;

    match &config.user {
        Some(user) => {
            output.status("User", &user.email);
            if let Some(name) = &user.name {
                output.status("Name", name);
            }
        }
        None => {
            output.status("Status", "Not authenticated");
            return Ok(());
        }
    }

    output.status("Server", config.server_url());

    match &config.active_workspace {
        Some(ws_id) => {
            if let Some(ws) = config.workspaces.get(ws_id) {
                output.status(
                    "Agent workspace",
                    &format!("{} ({}) [{}]", ws.name, ws_id, ws.role),
                );
            } else {
                output.status("Agent workspace", ws_id);
            }
        }
        None => output.status(
            "Agent workspace",
            "None (run `saturation workspace use <id>`)",
        ),
    }

    if let Some(token) = &config.token {
        output.status("Token expires", &token.expires_at);
    }

    Ok(())
}
