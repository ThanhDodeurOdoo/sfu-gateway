use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Gateway configuration from environment variables
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub bind: String,
    pub port: u16,
    pub key: String,
    pub nodes: Option<String>,
}

impl GatewayConfig {
    /// Load gateway configuration from environment variables.
    ///
    /// Environment variables:
    /// - `SFU_GATEWAY_BIND` - Address to bind (default: "0.0.0.0")
    /// - `SFU_GATEWAY_PORT` - Port to listen on (default: 8071)
    /// - `SFU_GATEWAY_KEY` - JWT secret key (required)
    /// - `SFU_GATEWAY_NODES` - JSON string of SFU nodes (optional)
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind = std::env::var("SFU_GATEWAY_BIND").unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = std::env::var("SFU_GATEWAY_PORT")
            .unwrap_or_else(|_| "8071".to_string())
            .parse::<u16>()
            .map_err(|e| ConfigError::Env {
                var: "SFU_GATEWAY_PORT".to_string(),
                message: format!("invalid port: {}", e),
            })?;

        let key = std::env::var("SFU_GATEWAY_KEY").map_err(|_| ConfigError::Env {
            var: "SFU_GATEWAY_KEY".to_string(),
            message: "required but not set".to_string(),
        })?;

        let nodes = std::env::var("SFU_GATEWAY_NODES").ok();

        Ok(Self {
            bind,
            port,
            key,
            nodes,
        })
    }
}

/// Node data containing SFU entries
#[derive(Debug, Clone, Deserialize)]
pub struct NodeData {
    #[serde(default)]
    pub sfu: Vec<SfuConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SfuConfig {
    /// The base URL of the SFU (e.g., "http://sfu1.example.com:3000")
    pub address: String,
    /// Geographic region identifier (e.g., "eu-west", "us-east")
    #[serde(default)]
    pub region: Option<String>,
    /// The JWT secret key for this SFU
    pub key: String,
}

impl NodeData {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io {
            path: path.as_ref().display().to_string(),
            source: e,
        })?;
        toml::from_str(&content).map_err(ConfigError::Toml)
    }

    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(json).map_err(ConfigError::Json)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Toml(toml::de::Error),
    Json(serde_json::Error),
    Env {
        var: String,
        message: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read file '{}': {}", path, source)
            }
            ConfigError::Toml(e) => write!(f, "failed to parse TOML config: {}", e),
            ConfigError::Json(e) => write!(f, "failed to parse JSON config: {}", e),
            ConfigError::Env { var, message } => {
                write!(f, "environment variable {}: {}", var, message)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_secrets_file() {
        let config_str = r#"
            [[sfu]]
            address = "http://sfu1.example.com:3000"
            region = "eu-west"
            key = "sfu1-secret-key"

            [[sfu]]
            address = "http://sfu2.example.com:3000"
            key = "sfu2-secret-key"
        "#;

        let secrets: NodeData = toml::from_str(config_str).unwrap();
        assert_eq!(secrets.sfu.len(), 2);
        assert_eq!(secrets.sfu[0].address, "http://sfu1.example.com:3000");
        assert_eq!(secrets.sfu[0].region, Some("eu-west".to_string()));
        assert_eq!(secrets.sfu[0].key, "sfu1-secret-key");
        assert_eq!(secrets.sfu[1].region, None);
        assert_eq!(secrets.sfu[1].key, "sfu2-secret-key");
    }

    #[test]
    fn test_empty_secrets_file() {
        let config_str = "";
        let secrets: NodeData = toml::from_str(config_str).unwrap();
        assert!(secrets.sfu.is_empty());
    }
    #[test]
    fn test_parse_json_nodes() {
        let json_str = r#"{
            "sfu": [
                {
                    "address": "http://sfu1.example.com:3000",
                    "region": "eu-west",
                    "key": "sfu1-secret-key"
                }
            ]
        }"#;

        let secrets = NodeData::from_json(json_str).unwrap();
        assert_eq!(secrets.sfu.len(), 1);
        assert_eq!(secrets.sfu[0].address, "http://sfu1.example.com:3000");
    }

    #[test]
    fn test_parse_invalid_json() {
        let json_str = r#"{ "sfu": [ { "address": "incomplete" "#;
        let result = NodeData::from_json(json_str);
        assert!(result.is_err());
        if let Err(ConfigError::Json(e)) = result {
            assert!(e.to_string().contains("EOF"));
        } else {
            panic!("Expected JSON error");
        }
    }

    #[test]
    fn test_gateway_config_from_env() {
        std::env::set_var("SFU_GATEWAY_KEY", "test-key");
        std::env::set_var("SFU_GATEWAY_NODES", "{\"sfu\":[]}");

        let config = GatewayConfig::from_env().unwrap();
        assert_eq!(config.key, "test-key");
        assert_eq!(config.nodes, Some("{\"sfu\":[]}".to_string()));
    }
}
