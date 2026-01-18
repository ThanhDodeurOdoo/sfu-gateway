use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth;
use crate::balancer::Balancer;

/// Shared application state
pub struct AppState {
    pub balancer: Balancer,
    pub http_client: reqwest::Client,
    /// Gateway's JWT secret key for verifying tokens from Odoo
    pub gateway_key: String,
}

/// Query parameters for /v1/channel
#[derive(Debug, Deserialize)]
pub struct ChannelQuery {
    #[serde(rename = "webRTC")]
    pub web_rtc: Option<String>,
    #[serde(rename = "recordingAddress")]
    pub recording_address: Option<String>,
    /// Region hint for load balancing
    pub region: Option<String>,
}

/// Response from SFU /v1/channel endpoint
#[derive(Debug, Deserialize, Serialize)]
pub struct ChannelResponse {
    pub uuid: String,
    pub url: String,
}

/// Build X-Forwarded-For header value by prepending client IP to existing chain.
/// Pure function for testability.
fn build_forwarded_for(client_ip: &str, existing: Option<&str>) -> String {
    match existing {
        Some(chain) => format!("{}, {}", client_ip, chain),
        None => client_ip.to_string(),
    }
}

/// Extract X-Forwarded-For components from HttpRequest and build the header.
/// The SFU reads the first IP from this header (when in proxy mode).
fn get_forwarded_for(req: &HttpRequest) -> String {
    let client_ip = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();

    let existing = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|h| h.to_str().ok());

    build_forwarded_for(&client_ip, existing)
}

/// Health check endpoint
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

/// Forward /v1/channel request to selected SFU
///
/// Flow:
/// 1. Extract and verify JWT from Odoo (using gateway's key)
/// 2. Select an SFU based on region hint
/// 3. Re-sign the JWT with the selected SFU's key
/// 4. Forward request to SFU with new JWT
pub async fn channel(
    req: HttpRequest,
    query: web::Query<ChannelQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 1. Extract and verify JWT from Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    debug!(auth_header = ?auth_header, "Received Authorization header");

    let token = match auth::extract_token(auth_header) {
        Ok(t) => t,
        Err(e) => {
            warn!(auth_header = ?auth_header, "Missing authorization: {}", e);
            return HttpResponse::Unauthorized()
                .json(serde_json::json!({ "error": "missing authorization" }));
        }
    };

    let claims = match auth::verify(token, &state.gateway_key) {
        Ok(c) => c,
        Err(e) => {
            warn!("Invalid JWT: {}", e);
            return HttpResponse::Unauthorized()
                .json(serde_json::json!({ "error": "invalid token" }));
        }
    };

    info!(iss = %claims.iss, "Verified JWT from Odoo");

    // 2. Select an SFU based on region hint
    let sfu = match state.balancer.select(query.region.as_deref()) {
        Some(sfu) => sfu,
        None => {
            warn!("No SFU instances available");
            return HttpResponse::ServiceUnavailable()
                .json(serde_json::json!({ "error": "no SFU instances available" }));
        }
    };

    info!(sfu_address = %sfu.address, "Selected SFU");

    // 3. Re-sign the JWT with the selected SFU's key
    let sfu_token = match auth::sign(&claims, &sfu.key) {
        Ok(t) => t,
        Err(e) => {
            warn!("Failed to sign JWT for SFU: {}", e);
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": "internal error" }));
        }
    };

    // 4. Build the URL to the SFU and forward request
    let mut sfu_url = format!("{}/v1/channel", sfu.address);

    // Forward query parameters (except region which is gateway-specific)
    let mut query_parts = Vec::new();
    if let Some(ref web_rtc) = query.web_rtc {
        query_parts.push(format!("webRTC={}", web_rtc));
    }
    if let Some(ref recording_address) = query.recording_address {
        query_parts.push(format!(
            "recordingAddress={}",
            urlencoding::encode(recording_address)
        ));
    }
    if !query_parts.is_empty() {
        sfu_url.push('?');
        sfu_url.push_str(&query_parts.join("&"));
    }

    // Make the request to the SFU with re-signed JWT and forwarded client IP
    let request = state
        .http_client
        .get(&sfu_url)
        .header("Authorization", format!("Bearer {}", sfu_token))
        .header("X-Forwarded-For", get_forwarded_for(&req));

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                match response.json::<ChannelResponse>().await {
                    Ok(channel_resp) => {
                        info!(uuid = %channel_resp.uuid, url = %channel_resp.url, "Channel created");
                        HttpResponse::Ok().json(channel_resp)
                    }
                    Err(e) => {
                        warn!("Failed to parse SFU response: {}", e);
                        HttpResponse::BadGateway()
                            .json(serde_json::json!({ "error": "invalid SFU response" }))
                    }
                }
            } else {
                warn!(status = %status, "SFU returned error");
                HttpResponse::build(
                    actix_web::http::StatusCode::from_u16(status.as_u16())
                        .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                )
                .finish()
            }
        }
        Err(e) => {
            warn!("Failed to contact SFU: {}", e);
            HttpResponse::BadGateway()
                .json(serde_json::json!({ "error": "failed to contact SFU" }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_forwarded_for_no_existing() {
        let result = build_forwarded_for("192.168.1.100", None);
        assert_eq!(result, "192.168.1.100");
    }

    #[test]
    fn test_build_forwarded_for_with_existing() {
        let result = build_forwarded_for("192.168.1.100", Some("10.0.0.1"));
        assert_eq!(result, "192.168.1.100, 10.0.0.1");
    }

    #[test]
    fn test_build_forwarded_for_with_chain() {
        // Gateway receives request that already went through another proxy
        let result = build_forwarded_for("192.168.1.100", Some("10.0.0.1, 172.16.0.1"));
        assert_eq!(result, "192.168.1.100, 10.0.0.1, 172.16.0.1");
    }
}
