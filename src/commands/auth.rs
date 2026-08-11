use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{distr::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::cli::LoginArgs;
use crate::config::{Config, OAuthInfo, TokenInfo, UserInfo};
use crate::output::Output;

#[derive(Debug, Deserialize)]
struct OAuthMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RegistrationRequest<'a> {
    client_name: &'a str,
    redirect_uris: Vec<&'a str>,
    grant_types: Vec<&'a str>,
    response_types: Vec<&'a str>,
    token_endpoint_auth_method: &'a str,
}

#[derive(Debug, Deserialize)]
struct RegistrationResponse {
    client_id: String,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    id_token: Option<String>,
}

const API_RESOURCE: &str = "https://next-api.saturation.io/v1";

pub async fn login(args: LoginArgs, output: &Output) -> Result<()> {
    let issuer = args.issuer.trim_end_matches('/');
    let http = oauth_http_client()?;
    let metadata: OAuthMetadata = http
        .get(format!("{issuer}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .context("failed to reach the Saturation OAuth authority")?
        .error_for_status()
        .context("Saturation OAuth discovery failed")?
        .json()
        .await
        .context("invalid Saturation OAuth metadata")?;
    validate_metadata(issuer, &metadata)?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to open the local OAuth callback")?;
    let callback = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let registration: RegistrationResponse = http
        .post(&metadata.registration_endpoint)
        .json(&RegistrationRequest {
            client_name: "Saturation CLI",
            redirect_uris: vec![&callback],
            grant_types: vec!["authorization_code", "refresh_token"],
            response_types: vec!["code"],
            token_endpoint_auth_method: "none",
        })
        .send()
        .await
        .context("failed to register the CLI OAuth client")?
        .error_for_status()
        .context("Saturation rejected the CLI OAuth registration")?
        .json()
        .await
        .context("invalid OAuth client registration response")?;

    let verifier = random_string(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_string(32);
    let mut authorize = Url::parse(&metadata.authorization_endpoint)?;
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &registration.client_id)
        .append_pair("redirect_uri", &callback)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("resource", API_RESOURCE)
        .append_pair("state", &state);

    output.status("Sign in", authorize.as_str());
    if !args.no_browser && open::that(authorize.as_str()).is_err() {
        output.status(
            "Browser",
            "Could not open automatically. Use the link above.",
        );
    }

    let code = wait_for_callback(listener, &state).await?;
    let token_response = http
        .post(&metadata.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("client_id", registration.client_id.as_str()),
            ("redirect_uri", callback.as_str()),
            ("code_verifier", verifier.as_str()),
            ("resource", API_RESOURCE),
        ])
        .send()
        .await
        .context("failed to exchange the OAuth code")?;
    let status = token_response.status();
    let body = token_response.text().await?;
    if !status.is_success() {
        anyhow::bail!("Saturation login failed: HTTP {status}: {body}");
    }
    let tokens: OAuthTokenResponse =
        serde_json::from_str(&body).context("invalid OAuth token response")?;
    if !tokens.token_type.eq_ignore_ascii_case("bearer") {
        anyhow::bail!("Saturation login returned an unsupported token type");
    }
    if tokens.refresh_token.is_empty() {
        anyhow::bail!("Saturation login did not return a refresh token");
    }
    if tokens.expires_in.is_some_and(|seconds| seconds <= 0) {
        anyhow::bail!("Saturation login returned an invalid token lifetime");
    }

    let claims = tokens
        .id_token
        .as_deref()
        .and_then(decode_jwt_payload)
        .or_else(|| decode_jwt_payload(&tokens.access_token))
        .unwrap_or_default();
    let expires_at = tokens
        .expires_in
        .map(|seconds| (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339())
        .or_else(|| {
            claims
                .get("exp")
                .and_then(|v| v.as_i64())
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339()))
        })
        .unwrap_or_default();
    let mut config = Config::load().unwrap_or_default();
    config.token = Some(TokenInfo {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at,
    });
    config.oauth = Some(OAuthInfo {
        client_id: registration.client_id,
        token_endpoint: metadata.token_endpoint,
        resource: API_RESOURCE.into(),
    });
    config.user = None;
    hydrate_identity(&mut config, &claims);
    config.save()?;
    output.success("Signed in to Saturation");
    Ok(())
}

fn validate_metadata(expected_issuer: &str, metadata: &OAuthMetadata) -> Result<()> {
    let issuer_url = Url::parse(expected_issuer).context("invalid OAuth issuer")?;
    let issuer_is_loopback = matches!(issuer_url.host_str(), Some("localhost" | "127.0.0.1"));
    if issuer_url.scheme() != "https" && !(issuer_url.scheme() == "http" && issuer_is_loopback) {
        anyhow::bail!("OAuth issuer must use HTTPS outside loopback development");
    }
    if metadata.issuer.trim_end_matches('/') != expected_issuer {
        anyhow::bail!("Saturation OAuth metadata returned a different issuer");
    }
    if !metadata
        .code_challenge_methods_supported
        .iter()
        .any(|method| method == "S256")
    {
        anyhow::bail!("Saturation OAuth authority does not support PKCE S256");
    }
    for endpoint in [
        &metadata.authorization_endpoint,
        &metadata.token_endpoint,
        &metadata.registration_endpoint,
    ] {
        let url = Url::parse(endpoint).context("invalid OAuth endpoint")?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
        if url.scheme() != "https" && !(issuer_is_loopback && local) {
            anyhow::bail!("Saturation OAuth metadata contained an insecure endpoint");
        }
    }
    Ok(())
}

fn oauth_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build the OAuth client")
}

fn random_string(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

async fn wait_for_callback(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(300), listener.accept())
            .await
            .context("Saturation login timed out")??;
    let mut buffer = vec![0_u8; 8192];
    let count = stream.read(&mut buffer).await?;
    let request = std::str::from_utf8(&buffer[..count]).context("invalid OAuth callback")?;
    let target = request
        .split_whitespace()
        .nth(1)
        .context("invalid OAuth callback request")?;
    let url = Url::parse(&format!("http://localhost{target}"))?;
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let result = match query.get("error") {
        Some(error) => Err(anyhow::anyhow!("Saturation login was denied: {error}")),
        None if query.get("state").map(String::as_str) != Some(expected_state) => Err(
            anyhow::anyhow!("Saturation login callback had an invalid state"),
        ),
        None => query
            .get("code")
            .cloned()
            .context("OAuth callback did not include a code"),
    };
    let (status, message) = if result.is_ok() {
        ("200 OK", "Signed in. You can return to the terminal.")
    } else {
        (
            "400 Bad Request",
            "Sign-in failed. Return to the terminal for details.",
        )
    };
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}", message.len());
    stream.write_all(response.as_bytes()).await?;
    result
}

fn hydrate_identity(config: &mut Config, claims: &serde_json::Value) {
    let email = claims.get("email").and_then(|v| v.as_str());
    if let Some(email) = email {
        config.user = Some(UserInfo {
            id: claims
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            email: email.to_string(),
            name: claims
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from),
        });
    }
}

// ─── Token refresh ──────────────────────────────────────────────────────────────
//
// OAuth sessions may contain a refresh token. Use the recorded token endpoint
// so the CLI can renew an expired session.

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
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
    let http = oauth_http_client()?;
    let oauth = config
        .oauth
        .clone()
        .context("this stored token predates OAuth login. Run `saturation login` again.")?;
    let resp = http
        .post(&oauth.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token.refresh_token.as_str()),
            ("client_id", oauth.client_id.as_str()),
            ("resource", oauth.resource.as_str()),
        ])
        .send()
        .await
        .context("failed to reach token-refresh endpoint")?;

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
        expires_at: refreshed
            .expires_in
            .map(|seconds| (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339())
            .unwrap_or_default(),
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

pub fn logout(output: &Output) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    config.token = None;
    config.user = None;
    config.oauth = None;
    config.save()?;
    output.success("Logged out");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn callback_requires_matching_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let callback = tokio::spawn(wait_for_callback(listener, "expected"));
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"GET /callback?code=secret&state=wrong HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        assert!(callback
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("invalid state"));
    }

    #[tokio::test]
    async fn refresh_uses_the_discovered_oauth_client() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("client_id=client_test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;
        let mut config = Config {
            token: Some(TokenInfo {
                access_token: "old-access".into(),
                refresh_token: "old-refresh".into(),
                expires_at: String::new(),
            }),
            oauth: Some(OAuthInfo {
                client_id: "client_test".into(),
                token_endpoint: format!("{}/oauth2/token", server.uri()),
                resource: API_RESOURCE.into(),
            }),
            ..Config::default()
        };

        refresh_in_place(&mut config).await.unwrap();

        let token = config.token.unwrap();
        assert_eq!(token.access_token, "new-access");
        assert_eq!(token.refresh_token, "new-refresh");
        assert!(!token.expires_at.is_empty());
    }
}
