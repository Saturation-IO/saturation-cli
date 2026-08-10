use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".saturation";
const CONFIG_FILE: &str = "config.json";
/// Default host for the agent registry and legacy desktop-token refresh.
/// (`/api/cli/...`), which live on the internal web host.
const DEFAULT_SERVER: &str = "https://next.saturation.io";
/// Default host for the public `/v1` API. Distinct from `DEFAULT_SERVER`: the
/// public surface is served from `next-api.saturation.io` (per the OpenAPI `servers`
/// block and the TypeScript SDK), while auth + the agent registry stay on the
/// internal host. An explicit `--server` / config / env override applies to both.
const DEFAULT_V1_SERVER: &str = "https://next-api.saturation.io";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub user: Option<UserInfo>,
    pub active_workspace: Option<String>,
    pub workspaces: HashMap<String, WorkspaceInfo>,
    pub token: Option<TokenInfo>,
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(CONFIG_DIR))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        let config: Config = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse config at {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir()?;
        self.save_to_dir(&dir)
    }

    fn save_to_dir(&self, dir: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create config directory at {}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }

        let path = dir.join(CONFIG_FILE);
        let contents = serde_json::to_string_pretty(self)?;

        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .with_context(|| format!("Failed to open config at {}", path.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("Failed to write config at {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Host for auth + the agent registry (internal web host). An explicit
    /// override (`--server` / config / env) wins for every namespace.
    pub fn server_url(&self) -> &str {
        self.server.as_deref().unwrap_or(DEFAULT_SERVER)
    }

    /// Host for the public `/v1` API client. Falls back to the public API host
    /// (`next-api.saturation.io`), not the internal one, when no override is set.
    pub fn v1_server_url(&self) -> &str {
        self.server.as_deref().unwrap_or(DEFAULT_V1_SERVER)
    }

    pub fn require_token(&self) -> Result<&TokenInfo> {
        self.token
            .as_ref()
            .context("Not authenticated. Run `saturation auth token TOKEN` first.")
    }
}

impl TokenInfo {
    /// Whether the access token is expired (or within a 60s skew window).
    /// An empty `expires_at` (e.g. a non-expiring injected token) is treated as
    /// not-expired.
    pub fn is_expired(&self) -> bool {
        if self.expires_at.trim().is_empty() {
            return false;
        }
        match chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(exp) => {
                let now = chrono::Utc::now();
                exp.with_timezone(&chrono::Utc) <= now + chrono::Duration::seconds(60)
            }
            // Unparseable expiry: don't block, let the server reject if stale.
            Err(_) => false,
        }
    }

    /// Whether this token can be refreshed (has a non-empty refresh token).
    /// Injected JWTs store an empty refresh token and are non-refreshable.
    pub fn is_refreshable(&self) -> bool {
        !self.refresh_token.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn with_temp_config<F: FnOnce(PathBuf)>(f: F) {
        let dir = TempDir::new().unwrap();
        f(dir.path().to_path_buf());
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.user.is_none());
        assert!(config.active_workspace.is_none());
        assert!(config.workspaces.is_empty());
        assert!(config.token.is_none());
    }

    #[test]
    fn test_roundtrip_config() {
        with_temp_config(|dir| {
            let path = dir.join("config.json");
            let mut config = Config {
                user: Some(UserInfo {
                    id: "user-1".into(),
                    email: "test@example.com".into(),
                    name: Some("Test User".into()),
                }),
                active_workspace: Some("ws-abc".into()),
                ..Config::default()
            };
            config.workspaces.insert(
                "ws-abc".into(),
                WorkspaceInfo {
                    name: "Test Studio".into(),
                    role: "admin".into(),
                },
            );

            let contents = serde_json::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &contents).unwrap();

            let loaded: Config =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(loaded.active_workspace.as_deref(), Some("ws-abc"));
            assert_eq!(loaded.user.unwrap().email, "test@example.com");
            assert_eq!(loaded.workspaces["ws-abc"].name, "Test Studio");
        });
    }

    #[cfg(unix)]
    #[test]
    fn saved_token_config_is_private() {
        use std::os::unix::fs::PermissionsExt;

        with_temp_config(|dir| {
            let config = Config {
                token: Some(TokenInfo {
                    access_token: "secret-token".into(),
                    refresh_token: String::new(),
                    expires_at: String::new(),
                }),
                ..Config::default()
            };

            config.save_to_dir(&dir).unwrap();

            assert_eq!(
                std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(dir.join(CONFIG_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        });
    }

    #[test]
    fn test_server_url_default() {
        let config = Config::default();
        assert_eq!(config.server_url(), "https://next.saturation.io");
    }

    #[test]
    fn test_server_url_override() {
        let config = Config {
            server: Some("http://localhost:4001".into()),
            ..Default::default()
        };
        assert_eq!(config.server_url(), "http://localhost:4001");
    }

    #[test]
    fn test_v1_server_url_default_is_public_api_host() {
        // The public /v1 client defaults to next-api.saturation.io, not the internal
        // host that auth + the agent registry use.
        let config = Config::default();
        assert_eq!(config.v1_server_url(), "https://next-api.saturation.io");
        assert_eq!(config.server_url(), "https://next.saturation.io");
        assert_ne!(config.v1_server_url(), config.server_url());
    }

    #[test]
    fn test_v1_server_url_honors_override() {
        // An explicit override applies to both namespaces.
        let config = Config {
            server: Some("http://localhost:4300".into()),
            ..Default::default()
        };
        assert_eq!(config.v1_server_url(), "http://localhost:4300");
        assert_eq!(config.server_url(), "http://localhost:4300");
    }

    #[test]
    fn test_require_token_missing() {
        let config = Config::default();
        assert!(config.require_token().is_err());
    }
}
