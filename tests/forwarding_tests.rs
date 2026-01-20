mod common;

use std::sync::Arc;

use actix_web::{App, http::StatusCode, test, web};
use serde_json::json;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{GATEWAY_KEY, SFU_KEY, create_app_state, make_test_claims, single_sfu};
use sfu_gateway::auth;
use sfu_gateway::balancer::Balancer;
use sfu_gateway::handlers::{AppState, channel};

#[actix_web::test]
async fn test_forwarded_for_no_proxy() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    Mock::given(method("GET"))
        .and(path("/v1/channel"))
        .and(header_exists("X-Forwarded-For"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uuid": "test-uuid",
            "url": "wss://test"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let token = auth::sign(&make_test_claims(), GATEWAY_KEY).expect("signing should work");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/channel")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header(("X-Forwarded-For", "spoofed-ip"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn test_forwarded_for_with_proxy() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        true,
    );

    Mock::given(method("GET"))
        .and(path("/v1/channel"))
        .and(header_exists("X-Forwarded-For"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uuid": "test-uuid",
            "url": "wss://test"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let token = auth::sign(&make_test_claims(), GATEWAY_KEY).expect("signing should work");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/channel")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header(("X-Forwarded-For", "10.0.0.1"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn test_query_params_forwarded_except_region() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    Mock::given(method("GET"))
        .and(path("/v1/channel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uuid": "test-uuid",
            "url": "wss://test"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let token = auth::sign(&make_test_claims(), GATEWAY_KEY).expect("signing should work");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/channel?region=eu-west&webRTC=true&customParam=value")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn test_sfu_unavailable() {
    let state = Arc::new(AppState {
        balancer: Balancer::new(vec![]),
        http_client: reqwest::Client::new(),
        gateway_key: GATEWAY_KEY.to_vec(),
        trust_proxy: false,
    });

    let token = auth::sign(&make_test_claims(), GATEWAY_KEY).expect("signing should work");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/channel")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body, json!({ "error": "no SFU instances available" }));
}
