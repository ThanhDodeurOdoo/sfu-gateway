use std::fs;
use std::path::Path;

use base64::Engine;
use serde::Deserialize;

const EXPECTED_KEY_LENGTH: usize = 32;

/// Gateway configuration from environment variables
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub bind: String,
    pub port: u16,
    pub key: Vec<u8>,
    pub nodes: Option<String>,
    /// When true, trust X-Forwarded-For header from upstream proxy to determine client IP
    pub trust_proxy: bool,
}

impl GatewayConfig {
    /// Load gateway configuration from environment variables.
    ///
    /// Environment variables:
    /// - `SFU_GATEWAY_BIND` - Address to bind (default: "0.0.0.0")
    /// - `SFU_GATEWAY_PORT` - Port to listen on (default: 8071)
    /// - `SFU_GATEWAY_KEY` - Base64-encoded JWT secret key (required)
    /// - `SFU_GATEWAY_NODES` - JSON string of SFU nodes (optional)
    ///
    /// # Errors
    /// Returns `ConfigError::Env` if required variables are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind = std::env::var("SFU_GATEWAY_BIND").unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = std::env::var("SFU_GATEWAY_PORT")
            .unwrap_or_else(|_| "8071".to_string())
            .parse::<u16>()
            .map_err(|e| ConfigError::Env {
                var: "SFU_GATEWAY_PORT".to_string(),
                message: format!("invalid port: {e}"),
            })?;

        let key_str = std::env::var("SFU_GATEWAY_KEY").map_err(|_| ConfigError::Env {
            var: "SFU_GATEWAY_KEY".to_string(),
            message: "required but not set".to_string(),
        })?;
        let key = decode_and_validate_key(&key_str).map_err(|message| ConfigError::Env {
            var: "SFU_GATEWAY_KEY".to_string(),
            message,
        })?;

        let nodes = std::env::var("SFU_GATEWAY_NODES").ok();

        let trust_proxy = std::env::var("SFU_GATEWAY_TRUST_PROXY")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        Ok(Self {
            bind,
            port,
            key,
            nodes,
            trust_proxy,
        })
    }
}

/// Node data containing SFU entries (raw form for deserialization)
#[derive(Debug, Clone, Deserialize)]
struct RawNodeData {
    #[serde(default)]
    sfu: Vec<RawSfuConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSfuConfig {
    address: String,
    #[serde(default)]
    region: Option<String>, // TODO: region should be a well defined type, not all strings can be a region.
    key: String,
}

/// Node data containing SFU entries
#[derive(Debug, Clone)]
pub struct NodeData {
    pub sfu: Vec<SfuConfig>,
}

#[derive(Debug, Clone)]
pub struct SfuConfig {
    /// The base URL of the SFU (e.g., `http://sfu1.example.com:3000`)
    pub address: String,
    /// Geographic region identifier (e.g., "eu-west", "us-east")
    pub region: Option<String>,
    /// The decoded JWT secret key for this SFU (32 bytes)
    pub key: Vec<u8>,
}

fn decode_base64(key: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(key.trim_end_matches('='))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(key))
}

fn decode_and_validate_key(key: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_base64(key).map_err(|e| format!("invalid base64: {e}"))?;
    if bytes.len() < EXPECTED_KEY_LENGTH {
        tracing::warn!(
            key_length = bytes.len(),
            expected = EXPECTED_KEY_LENGTH,
            "Key is shorter than recommended for HMAC-SHA256"
        );
    }
    Ok(bytes)
}

impl NodeData {
    /// Load node data from a TOML file.
    ///
    /// # Errors
    /// Returns `ConfigError::Io` on file read failure, `ConfigError::Toml` on parse failure.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io {
            path: path.as_ref().display().to_string(),
            source: e,
        })?;
        let raw: RawNodeData = toml::from_str(&content).map_err(ConfigError::Toml)?;
        Self::from_raw(raw)
    }

    /// Parse node data from a JSON string.
    ///
    /// # Errors
    /// Returns `ConfigError::Json` on parse failure, `ConfigError::Key` on invalid keys.
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let raw: RawNodeData = serde_json::from_str(json).map_err(ConfigError::Json)?;
        Self::from_raw(raw)
    }

    #[cfg(test)]
    pub fn load_from_toml(toml_str: &str) -> Result<Self, ConfigError> {
        let raw: RawNodeData = toml::from_str(toml_str).map_err(ConfigError::Toml)?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawNodeData) -> Result<Self, ConfigError> {
        let sfu = raw
            .sfu
            .into_iter()
            .enumerate()
            .map(|(i, raw_sfu)| {
                let key =
                    decode_and_validate_key(&raw_sfu.key).map_err(|message| ConfigError::Key {
                        index: i,
                        address: raw_sfu.address.clone(),
                        message,
                    })?;
                Ok(SfuConfig {
                    address: raw_sfu.address,
                    region: raw_sfu.region,
                    key,
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok(Self { sfu })
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
    Key {
        index: usize,
        address: String,
        message: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read file '{path}': {source}")
            }
            Self::Toml(e) => write!(f, "failed to parse TOML config: {e}"),
            Self::Json(e) => write!(f, "failed to parse JSON config: {e}"),
            Self::Env { var, message } => {
                write!(f, "environment variable {var}: {message}")
            }
            Self::Key {
                index,
                address,
                message,
            } => {
                write!(f, "invalid key for SFU[{index}] at '{address}': {message}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    // 32 bytes: "test-secret-key-1234567890123456" in base64
    const VALID_KEY_1: &str = "dGVzdC1zZWNyZXQta2V5LTEyMzQ1Njc4OTAxMjM0NTY=";
    const VALID_KEY_1_BYTES: &[u8] = b"test-secret-key-1234567890123456";
    // 32 bytes: "other-secret-key-123456789012345" in base64
    const VALID_KEY_2: &str = "b3RoZXItc2VjcmV0LWtleS0xMjM0NTY3ODkwMTIzNDU=";
    const VALID_KEY_2_BYTES: &[u8] = b"other-secret-key-123456789012345";

    #[test]
    fn test_parse_secrets_file() {
        let config_str = format!(
            r#"
            [[sfu]]
            address = "http://sfu1.example.com:3000"
            region = "eu-west"
            key = "{VALID_KEY_1}"

            [[sfu]]
            address = "http://sfu2.example.com:3000"
            key = "{VALID_KEY_2}"
        "#
        );

        let secrets = NodeData::load_from_toml(&config_str).unwrap();
        assert_eq!(secrets.sfu.len(), 2);
        assert_eq!(secrets.sfu[0].address, "http://sfu1.example.com:3000");
        assert_eq!(secrets.sfu[0].region, Some("eu-west".to_string()));
        assert_eq!(secrets.sfu[0].key, VALID_KEY_1_BYTES);
        assert_eq!(secrets.sfu[1].region, None);
        assert_eq!(secrets.sfu[1].key, VALID_KEY_2_BYTES);
    }

    #[test]
    fn test_empty_secrets_file() {
        let config_str = "";
        let secrets = NodeData::load_from_toml(config_str).unwrap();
        assert!(secrets.sfu.is_empty());
    }

    #[test]
    fn test_parse_json_nodes() {
        let json_str = format!(
            r#"{{
            "sfu": [
                {{
                    "address": "http://sfu1.example.com:3000",
                    "region": "eu-west",
                    "key": "{VALID_KEY_1}"
                }}
            ]
        }}"#
        );

        let secrets = NodeData::from_json(&json_str).unwrap();
        assert_eq!(secrets.sfu.len(), 1);
        assert_eq!(secrets.sfu[0].address, "http://sfu1.example.com:3000");
        assert_eq!(secrets.sfu[0].key, VALID_KEY_1_BYTES);
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
    fn test_invalid_key_not_base64() {
        let json_str = r#"{
            "sfu": [{ "address": "http://sfu.example.com", "key": "not-valid-base64!!!" }]
        }"#;
        let result = NodeData::from_json(json_str);
        assert!(matches!(result, Err(ConfigError::Key { .. })));
    }

    #[test]
    fn test_short_key_accepted_with_warning() {
        // "short-key" is only 9 bytes - should succeed but would log a warning
        let short_key = "c2hvcnQta2V5";
        let json_str = format!(
            r#"{{ "sfu": [{{ "address": "http://sfu.example.com", "key": "{short_key}" }}] }}"#
        );
        let result = NodeData::from_json(&json_str);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sfu[0].key, b"short-key");
    }

    #[test]
    #[serial_test::serial]
    fn test_gateway_config_from_env() {
        // SAFETY: test runs serially
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("SFU_GATEWAY_KEY", VALID_KEY_1);
            std::env::set_var("SFU_GATEWAY_NODES", "{\"sfu\":[]}");
        }

        let config = GatewayConfig::from_env().unwrap();
        assert_eq!(config.key, VALID_KEY_1_BYTES);
        assert_eq!(config.nodes, Some("{\"sfu\":[]}".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn test_gateway_config_invalid_key() {
        // SAFETY: test runs serially
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("SFU_GATEWAY_KEY", "invalid-key");
        }
        let result = GatewayConfig::from_env();
        assert!(matches!(result, Err(ConfigError::Env { .. })));
    }
}
