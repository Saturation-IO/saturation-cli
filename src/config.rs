use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".saturation";
const CONFIG_FILE: &str = "config.json";
/// Default host for the canonical public `/v1` API.
const DEFAULT_V1_SERVER: &str = "https://next-api.saturation.io";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub user: Option<UserInfo>,
    pub token: Option<TokenInfo>,
    #[serde(default)]
    pub oauth: Option<OAuthInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthInfo {
    pub client_id: String,
    pub token_endpoint: String,
    pub resource: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
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

    /// Host for the public `/v1` API client. Falls back to the public API host
    /// (`next-api.saturation.io`), not the internal one, when no override is set.
    pub fn v1_server_url(&self) -> &str {
        DEFAULT_V1_SERVER
    }

    pub fn require_token(&self) -> Result<&TokenInfo> {
        self.token
            .as_ref()
            .context("Not authenticated. Run `saturation login` first.")
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
        assert!(config.token.is_none());
    }

    #[test]
    fn test_roundtrip_config() {
        with_temp_config(|dir| {
            let path = dir.join("config.json");
            let config = Config {
                user: Some(UserInfo {
                    id: "user-1".into(),
                    email: "test@example.com".into(),
                    name: Some("Test User".into()),
                }),
                ..Config::default()
            };

            let contents = serde_json::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &contents).unwrap();

            let loaded: Config =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(loaded.user.unwrap().email, "test@example.com");
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
    fn test_v1_server_url_default_is_public_api_host() {
        let config = Config::default();
        assert_eq!(config.v1_server_url(), "https://next-api.saturation.io");
    }

    #[test]
    fn test_require_token_missing() {
        let config = Config::default();
        assert!(config.require_token().is_err());
    }
}
