//! JWT authentication module
//!
//! Handles:
//! - Verifying JWTs from Odoo (signed with gateway's key)
//! - Re-signing JWTs for SFUs (signed with each SFU's key)

use base64::Engine;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims structure matching the SFU's expected format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Issuer - identifies the caller (Odoo channel UUID)
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
            AuthError::MissingToken => write!(f, "missing authorization token"),
            AuthError::InvalidToken(e) => write!(f, "invalid token: {}", e),
            AuthError::SigningFailed(e) => write!(f, "failed to sign token: {}", e),
        }
    }
}

impl std::error::Error for AuthError {}

/// Verify a JWT using the gateway's secret key.
/// The key should be base64-encoded (matching Odoo's format).
pub fn verify(token: &str, gateway_key: &str) -> Result<Claims, AuthError> {
    use tracing::debug;

    debug!(token_len = token.len(), "Verifying JWT");
    debug!(key_len = gateway_key.len(), key_preview = &gateway_key[..gateway_key.len().min(10)], "Using gateway key");

    // Decode the base64 key (Odoo uses base64-decoded key for HMAC)
    let key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(gateway_key.trim_end_matches('='))
        .or_else(|e| {
            debug!(url_safe_error = %e, "URL-safe base64 decode failed, trying standard");
            // Try standard base64 if URL-safe fails
            base64::engine::general_purpose::STANDARD.decode(gateway_key)
        })
        .map_err(|e| AuthError::InvalidToken(format!("invalid key encoding: {}", e)))?;

    debug!(key_bytes_len = key_bytes.len(), "Decoded key bytes");

    let key = DecodingKey::from_secret(&key_bytes);

    // Parse the token header to see what algorithm it uses
    if let Some(dot_pos) = token.find('.') {
        if let Ok(header_bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token[..dot_pos].trim_end_matches('='))
        {
            if let Ok(header_str) = std::str::from_utf8(&header_bytes) {
                debug!(header = header_str, "JWT header");
            }
        }
    }

    // Allow some clock skew and don't require exp for flexibility
    let mut validation = Validation::default();
    validation.required_spec_claims.clear();
    validation.validate_exp = false;

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| {
            debug!(error = %e, "JWT decode failed");
            AuthError::InvalidToken(e.to_string())
        })?;

    debug!(iss = %token_data.claims.iss, "JWT verified successfully");
    Ok(token_data.claims)
}

/// Sign claims with the SFU's secret key.
/// The key should be base64-encoded (matching SFU's format).
pub fn sign(claims: &Claims, sfu_key: &str) -> Result<String, AuthError> {
    // Decode the base64 key
    let key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sfu_key.trim_end_matches('='))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(sfu_key))
        .map_err(|e| AuthError::SigningFailed(format!("invalid key encoding: {}", e)))?;

    let key = EncodingKey::from_secret(&key_bytes);
    encode(&Header::default(), claims, &key).map_err(|e| AuthError::SigningFailed(e.to_string()))
}

/// Extract token from Authorization header (format: "<scheme> <token>")
pub fn extract_token(auth_header: Option<&str>) -> Result<&str, AuthError> {
    let header = auth_header.ok_or(AuthError::MissingToken)?;
    header
        .split_once(' ')
        .map(|(_, token)| token)
        .ok_or(AuthError::InvalidToken("expected '<scheme> <token>' format".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Base64-encoded test keys (32 random bytes each)
    const TEST_KEY: &str = "dGVzdC1zZWNyZXQta2V5LTEyMzQ1Njc4OTAxMjM0NTY="; // "test-secret-key-12345678901234567"
    const WRONG_KEY: &str = "d3JvbmctdGVzdC1rZXktMTIzNDU2Nzg5MDEyMzQ1Njc="; // "wrong-test-key-12345678901234567"

    fn make_test_claims() -> Claims {
        Claims {
            iss: "test-channel-123".to_string(),
            key: Some("encryption-key".to_string()),
            exp: None,
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
        let gateway_key = "Z2F0ZXdheS1zZWNyZXQta2V5LTEyMzQ1Njc4OTAxMjM0"; // base64
        let sfu_key = "c2Z1LXNlY3JldC1rZXktMTIzNDU2Nzg5MDEyMzQ1Njc4"; // base64

        // Odoo signs with gateway key
        let claims = make_test_claims();
        let original_token = sign(&claims, gateway_key).unwrap();

        // Gateway verifies with its key
        let verified_claims = verify(&original_token, gateway_key).unwrap();

        // Gateway re-signs with SFU key
        let new_token = sign(&verified_claims, sfu_key).unwrap();

        // SFU can verify with its key
        let sfu_verified = verify(&new_token, sfu_key).unwrap();
        assert_eq!(sfu_verified.iss, "test-channel-123");

        // But not with gateway key
        assert!(verify(&new_token, gateway_key).is_err());
    }
}
