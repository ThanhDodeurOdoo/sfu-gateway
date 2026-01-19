use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth;
use crate::balancer::Balancer;

pub struct AppState {
    pub balancer: Balancer,
    pub http_client: reqwest::Client,
    /// Gateway's JWT secret key for verifying tokens from Odoo (decoded bytes)
    pub gateway_key: Vec<u8>,
}

/// Query parameters for /v1/channel (gateway-specific only)
#[derive(Debug, Deserialize)]
pub struct ChannelQuery {
    /// Region hint for load balancing (not forwarded to SFU)
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
        Some(chain) => format!("{client_ip}, {chain}"),
        None => client_ip.to_string(),
    }
}

/// Extract X-Forwarded-For components from `HttpRequest` and build the header.
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

const BLACKLISTED_QUERY_PARAMS: &[&str] = &["region"];

/// Filter query string, removing gateway-specific parameters (blacklist approach).
/// Pure function for testability.
fn filter_query_params(query_string: &str) -> String {
    query_string
        .split('&')
        .filter(|part| {
            !BLACKLISTED_QUERY_PARAMS
                .iter()
                .any(|blocked| part.starts_with(&format!("{blocked}=")))
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn noop() -> HttpResponse {
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
    let Some(sfu) = state.balancer.select(query.region.as_deref()) else {
        warn!("No SFU instances available");
        return HttpResponse::ServiceUnavailable()
            .json(serde_json::json!({ "error": "no SFU instances available" }));
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

    let filtered_query = filter_query_params(req.query_string());
    if !filtered_query.is_empty() {
        sfu_url.push('?');
        sfu_url.push_str(&filtered_query);
    }

    // Make the request to the SFU with re-signed JWT and forwarded client IP
    let request = state
        .http_client
        .get(&sfu_url)
        .header("Authorization", format!("Bearer {sfu_token}"))
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
            HttpResponse::BadGateway().json(serde_json::json!({ "error": "failed to contact SFU" }))
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
        let result = build_forwarded_for("192.168.1.100", Some("10.0.0.1, 172.16.0.1"));
        assert_eq!(result, "192.168.1.100, 10.0.0.1, 172.16.0.1");
    }

    #[test]
    fn test_filter_query_params_passes_through_all() {
        let result = filter_query_params("webRTC=true&recordingAddress=http%3A%2F%2Flocalhost");
        assert_eq!(
            result,
            "webRTC=true&recordingAddress=http%3A%2F%2Flocalhost"
        );
    }

    #[test]
    fn test_filter_query_params_removes_region() {
        let result =
            filter_query_params("webRTC=true&region=eu&recordingAddress=http%3A%2F%2Flocalhost");
        assert_eq!(
            result,
            "webRTC=true&recordingAddress=http%3A%2F%2Flocalhost"
        );
    }

    #[test]
    fn test_filter_query_params_region_only() {
        let result = filter_query_params("region=us");
        assert_eq!(result, "");
    }

    #[test]
    fn test_filter_query_params_empty() {
        let result = filter_query_params("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_filter_query_params_preserves_new_params() {
        let result = filter_query_params("newParam=value&anotherNew=123&region=eu");
        assert_eq!(result, "newParam=value&anotherNew=123");
    }
}
