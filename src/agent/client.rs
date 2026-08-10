//! The agent-surface HTTP adapter.
//!
//! Targets `/api/cli/:ws/...` and speaks the internal `{ success, data, summary }`
//! envelope (server `routes/cli/index.ts`). This is deliberately separate from
//! the `/v1` [`crate::v1::Client`] so the two envelopes never bleed into each
//! other: this adapter unwraps `{success,data}`; the `/v1` client keys off HTTP
//! status with the §5d error model.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

use crate::config::Config;

pub struct AgentClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    workspace_id: String,
}

/// The internal agent envelope. `success:true` with `data`/`summary` on success;
/// `success:false` with `error`/`message` on failure.
#[derive(Debug, Deserialize)]
struct AgentEnvelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// The shape of `GET /tools`: `{ tools: [{ name, description }] }`.
#[derive(Debug, Deserialize)]
struct ToolsResponse {
    tools: Vec<ToolInfo>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

impl AgentClient {
    pub fn from_config(config: &Config, workspace_override: Option<&str>) -> Result<Self> {
        let token = config.require_token()?;
        let workspace_id = workspace_override
            .map(|s| s.to_string())
            .or_else(|| config.active_workspace.clone())
            .context("no active agent workspace. Run `saturation workspace use <id>` first.")?;
        Ok(Self {
            // Bound every request: a black-holed host must not hang the CLI
            // indefinitely. (SAT-4696 review S1.)
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: config.server_url().trim_end_matches('/').to_string(),
            token: token.access_token.clone(),
            workspace_id,
        })
    }

    fn url(&self, suffix: &str) -> String {
        format!(
            "{}/api/cli/{}/{}",
            self.base_url,
            self.workspace_id,
            suffix.trim_start_matches('/')
        )
    }

    /// `GET /api/cli/:ws/tools` — registry discovery.
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let resp = self
            .http
            .get(self.url("tools"))
            .bearer_auth(&self.token)
            .send()
            .await
            .context("request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {status}: {body}");
        }
        let parsed: ToolsResponse = resp.json().await.context("failed to parse /tools")?;
        Ok(parsed.tools)
    }

    /// `POST /api/cli/:ws/tool/:toolName` — invoke a registered tool.
    pub async fn call_tool(
        &self,
        tool: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resp = self
            .http
            .post(self.url(&format!("tool/{tool}")))
            .bearer_auth(&self.token)
            .json(params)
            .send()
            .await
            .context("request failed")?;
        self.unwrap_envelope(resp).await
    }

    /// `POST /api/cli/:ws/upload` — multipart document upload.
    pub async fn upload(
        &self,
        file_path: &Path,
        metadata: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload")
            .to_string();
        let file_bytes = tokio::fs::read(file_path)
            .await
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")?;
        let meta_part = reqwest::multipart::Part::text(serde_json::to_string(&metadata)?)
            .mime_str("application/json")?;
        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .part("metadata", meta_part);
        let resp = self
            .http
            .post(self.url("upload"))
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await
            .context("upload request failed")?;
        self.unwrap_envelope(resp).await
    }

    /// Map the internal `{success,data,summary}` envelope to the data payload,
    /// surfacing `error`/`message` on failure.
    async fn unwrap_envelope(&self, resp: reqwest::Response) -> Result<serde_json::Value> {
        let status = resp.status();
        let bytes = resp.bytes().await.context("failed to read response")?;
        if !status.is_success() {
            // Error envelope may carry a typed message; surface it.
            if let Ok(env) = serde_json::from_slice::<AgentEnvelope>(&bytes) {
                let msg = env.message.or(env.error).unwrap_or_default();
                anyhow::bail!("HTTP {status}: {msg}");
            }
            anyhow::bail!("HTTP {status}: {}", String::from_utf8_lossy(&bytes));
        }
        let env: AgentEnvelope =
            serde_json::from_slice(&bytes).context("failed to parse agent response")?;
        if !env.success {
            let msg = env
                .error
                .or(env.message)
                .unwrap_or_else(|| "tool failed".into());
            anyhow::bail!("agent error: {msg}");
        }
        // Attach the summary as a sibling so callers can show it without a second call.
        let mut data = env.data.unwrap_or(serde_json::Value::Null);
        if let (Some(summary), serde_json::Value::Object(map)) = (env.summary, &mut data) {
            map.entry("_summary")
                .or_insert(serde_json::Value::String(summary));
        }
        Ok(data)
    }

    #[allow(dead_code)] // public accessor; exercised by unit tests
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TokenInfo, WorkspaceInfo};

    fn cfg() -> Config {
        Config {
            server: Some("http://localhost:4001".into()),
            active_workspace: Some("ws-1".into()),
            token: Some(TokenInfo {
                access_token: "tok".into(),
                refresh_token: "ref".into(),
                expires_at: "2099-01-01".into(),
            }),
            workspaces: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "ws-1".into(),
                    WorkspaceInfo {
                        name: "T".into(),
                        role: "admin".into(),
                    },
                );
                m
            },
            ..Default::default()
        }
    }

    #[test]
    fn builds_tool_url() {
        let c = AgentClient::from_config(&cfg(), None).unwrap();
        assert_eq!(c.url("tools"), "http://localhost:4001/api/cli/ws-1/tools");
        assert_eq!(
            c.url("tool/query"),
            "http://localhost:4001/api/cli/ws-1/tool/query"
        );
    }

    #[test]
    fn workspace_override_wins() {
        let c = AgentClient::from_config(&cfg(), Some("ws-other")).unwrap();
        assert_eq!(c.workspace_id(), "ws-other");
    }
}
