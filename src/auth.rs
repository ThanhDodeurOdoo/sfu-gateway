//! JWT authentication module
//!
//! Handles:
//! - Verifying JWTs from Odoo (signed with gateway's key)
//! - Re-signing JWTs for SFUs (signed with each SFU's key)

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims structure matching the SFU's expected format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Issuer - identifies the caller (the sfu uses it for channel generation idempotency)
    pub iss: String,
    /// Optional encryption key for recording
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Expiration time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    /// Issued at time (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
}

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    SigningFailed(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingToken => write!(f, "missing authorization token"),
            Self::InvalidToken(e) => write!(f, "invalid token: {e}"),
            Self::SigningFailed(e) => write!(f, "failed to sign token: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Verify a JWT using the gateway's secret key (raw bytes).
pub fn verify(token: &str, key_bytes: &[u8]) -> Result<Claims, AuthError> {
    use tracing::debug;

    let key = DecodingKey::from_secret(key_bytes);

    let token_data = decode::<Claims>(token, &key, &Validation::default()).map_err(|e| {
        debug!(error = %e, "JWT decode failed");
        AuthError::InvalidToken(e.to_string())
    })?;

    debug!(iss = %token_data.claims.iss, "JWT verified successfully");
    Ok(token_data.claims)
}

/// Sign claims with the SFU's secret key (raw bytes).
pub fn sign(claims: &Claims, key_bytes: &[u8]) -> Result<String, AuthError> {
    let key = EncodingKey::from_secret(key_bytes);
    encode(&Header::default(), claims, &key).map_err(|e| AuthError::SigningFailed(e.to_string()))
}

/// Extract token from Authorization header (format: "<scheme> <token>")
pub fn extract_token(auth_header: Option<&str>) -> Result<&str, AuthError> {
    let header = auth_header.ok_or(AuthError::MissingToken)?;
    header
        .split_once(' ')
        .map(|(_, token)| token)
        .ok_or(AuthError::InvalidToken(
            "expected '<scheme> <token>' format".to_string(),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &[u8] = b"test-secret-key-1234567890123456";
    const WRONG_KEY: &[u8] = b"wrong-test-key-12345678901234567";

    fn make_test_claims() -> Claims {
        Claims {
            iss: "test-channel-123".to_string(),
            key: Some("encryption-key".to_string()),
            exp: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 3600,
            ),
            iat: None,
        }
    }

    #[test]
    fn test_sign_and_verify() {
        let claims = make_test_claims();
        let token = sign(&claims, TEST_KEY).unwrap();
        let verified = verify(&token, TEST_KEY).unwrap();

        assert_eq!(verified.iss, "test-channel-123");
        assert_eq!(verified.key, Some("encryption-key".to_string()));
    }

    #[test]
    fn test_verify_with_wrong_key() {
        let claims = make_test_claims();
        let token = sign(&claims, TEST_KEY).unwrap();
        let result = verify(&token, WRONG_KEY);

        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_extract_token() {
        assert_eq!(extract_token(Some("Bearer abc123")).unwrap(), "abc123");
        assert_eq!(extract_token(Some("jwt abc123")).unwrap(), "abc123");
        assert!(extract_token(None).is_err());
        assert!(extract_token(Some("no-space")).is_err());
    }

    #[test]
    fn test_resign_with_different_key() {
        let gateway_key: &[u8] = b"gateway-secret-key-123456789012";
        let sfu_key: &[u8] = b"sfu-secret-key-12345678901234567";

        let claims = make_test_claims();
        let original_token = sign(&claims, gateway_key).unwrap();

        let verified_claims = verify(&original_token, gateway_key).unwrap();

        let new_token = sign(&verified_claims, sfu_key).unwrap();

        let sfu_verified = verify(&new_token, sfu_key).unwrap();
        assert_eq!(sfu_verified.iss, "test-channel-123");

        assert!(verify(&new_token, gateway_key).is_err());
    }
}
