//! OAuth-aware adapter for the public Saturation MCP endpoint.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

const DEFAULT_MCP_RESOURCE: &str = "https://mcp.saturation.io/mcp";

pub struct AgentClient {
    http: reqwest::Client,
    resource: String,
    token: String,
    workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ToolsResult {
    tools: Vec<ToolInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    protocol_version: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

impl AgentClient {
    pub fn from_config(config: &Config, workspace_override: Option<&str>) -> Result<Self> {
        let token = config.require_token()?;
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .context("failed to build the MCP client")?,
            resource: config
                .oauth
                .as_ref()
                .map(|oauth| oauth.resource.clone())
                .unwrap_or_else(|| DEFAULT_MCP_RESOURCE.to_string()),
            token: token.access_token.clone(),
            workspace: workspace_override.map(String::from),
        })
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let protocol = self.initialize().await?;
        let result: ToolsResult = self
            .rpc(
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }),
                Some(&protocol),
            )
            .await?;
        Ok(result.tools)
    }

    pub async fn call_tool(
        &self,
        tool: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let protocol = self.initialize().await?;
        self.rpc(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": tool, "arguments": params }
            }),
            Some(&protocol),
        )
        .await
    }

    async fn initialize(&self) -> Result<String> {
        let initialized: InitializeResult = self
            .rpc(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "saturation-cli", "version": env!("CARGO_PKG_VERSION") }
                }
            }), None)
            .await?;
        self.notify_initialized(&initialized.protocol_version)
            .await?;
        Ok(initialized.protocol_version)
    }

    async fn notify_initialized(&self, protocol: &str) -> Result<()> {
        let response = self
            .request(
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }),
                Some(protocol),
            )
            .send()
            .await
            .context("MCP initialized notification failed")?;
        if !response.status().is_success() {
            anyhow::bail!(
                "MCP initialized notification failed with HTTP {}",
                response.status()
            );
        }
        Ok(())
    }

    fn request(&self, body: serde_json::Value, protocol: Option<&str>) -> reqwest::RequestBuilder {
        let mut builder = self
            .http
            .post(&self.resource)
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(protocol) = protocol {
            builder = builder.header("MCP-Protocol-Version", protocol);
        }
        if let Some(workspace) = &self.workspace {
            builder = builder.header("Saturation-Workspace", workspace);
        }
        builder
    }

    async fn rpc<T: for<'de> Deserialize<'de>>(
        &self,
        request: serde_json::Value,
        protocol: Option<&str>,
    ) -> Result<T> {
        let builder = self.request(request, protocol);
        let response = builder.send().await.context("MCP request failed")?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .context("failed to read MCP response")?;
        if !status.is_success() {
            anyhow::bail!("MCP request failed with HTTP {status}");
        }
        let envelope: RpcEnvelope<T> =
            serde_json::from_slice(&bytes).context("failed to parse MCP response")?;
        if let Some(error) = envelope.error {
            anyhow::bail!("MCP error {}: {}", error.code, error.message);
        }
        envelope
            .result
            .context("MCP response did not include a result")
    }

    #[cfg(test)]
    fn resource(&self) -> &str {
        &self.resource
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OAuthInfo, TokenInfo};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config() -> Config {
        Config {
            token: Some(TokenInfo {
                access_token: "token".into(),
                refresh_token: "refresh".into(),
                expires_at: "2099-01-01T00:00:00Z".into(),
            }),
            oauth: Some(OAuthInfo {
                client_id: "client".into(),
                token_endpoint: "https://connect.saturation.io/oauth2/token".into(),
                resource: "https://mcp.saturation.io/mcp".into(),
            }),
            ..Config::default()
        }
    }

    #[test]
    fn oauth_resource_is_the_agent_transport() {
        let client = AgentClient::from_config(&config(), None).unwrap();
        assert_eq!(client.resource(), "https://mcp.saturation.io/mcp");
    }

    #[tokio::test]
    async fn list_tools_initializes_and_sends_the_oauth_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(header("authorization", "Bearer token"))
            .and(body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "saturation-cli", "version": env!("CARGO_PKG_VERSION") }
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 0,
                "result": { "protocolVersion": "2025-11-25" }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(header("mcp-protocol-version", "2025-11-25"))
            .and(body_json(serde_json::json!({
                "jsonrpc": "2.0", "method": "notifications/initialized"
            })))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(header("mcp-protocol-version", "2025-11-25"))
            .and(header("saturation-workspace", "ws_override"))
            .and(body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "tools": [{ "name": "find", "description": "Find records", "inputSchema": {} }] }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut cfg = config();
        cfg.oauth.as_mut().unwrap().resource = format!("{}/mcp", server.uri());
        let client = AgentClient::from_config(&cfg, Some("ws_override")).unwrap();
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools[0].name, "find");
    }
}
