use std::sync::Arc;

use sfu_gateway::config::SfuConfig;
use sfu_gateway::http::{AppState, Claims, sign};
use sfu_gateway::routing::Balancer;

pub const GATEWAY_KEY: &[u8] = b"gateway-key-padded-to-32-bytes!!";

pub fn make_test_claims() -> Claims {
    Claims {
        iss: "test-channel-123".to_string(),
        key: Some("encryption-key".to_string()),
        exp: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time went backwards")
                .as_secs()
                + 3600,
        ),
        iat: None,
    }
}

pub fn create_app_state(
    sfus: Vec<SfuConfig>,
    gateway_key: &[u8],
    trust_proxy: bool,
) -> Arc<AppState> {
    Arc::new(AppState {
        balancer: Balancer::new(sfus),
        http_client: reqwest::Client::new(),
        gateway_key: gateway_key.to_vec(),
        trust_proxy,
    })
}

pub fn sign_claims(claims: &Claims, key: &[u8]) -> String {
    sign(claims, key).expect("signing should work")
}
