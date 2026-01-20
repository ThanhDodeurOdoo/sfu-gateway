mod common;

use actix_web::{App, http::StatusCode, test, web};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{GATEWAY_KEY, SFU_KEY, create_app_state, make_test_claims, single_sfu};
use sfu_gateway::auth;
use sfu_gateway::handlers::{channel, noop};

#[actix_web::test]
async fn test_noop_endpoint() {
    let app = test::init_service(App::new().route("/noop", web::get().to(noop))).await;

    let req = test::TestRequest::get().uri("/noop").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body, json!({ "status": "ok" }));
}

#[actix_web::test]
async fn test_channel_missing_auth() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get().uri("/v1/channel").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body, json!({ "error": "missing authorization" }));
}

#[actix_web::test]
async fn test_channel_invalid_token() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/channel")
        .insert_header(("Authorization", "Bearer invalid.jwt.token"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body, json!({ "error": "invalid token" }));
}

#[actix_web::test]
async fn test_channel_wrong_key() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    let wrong_key = b"wrong-key-padded-to-32-bytes!!!!";
    let token = auth::sign(&make_test_claims(), wrong_key).expect("signing should work");

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

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_channel_valid_flow() {
    let mock_server = MockServer::start().await;
    let state = create_app_state(
        single_sfu(&mock_server.uri(), Some("eu-west"), SFU_KEY),
        GATEWAY_KEY,
        false,
    );

    Mock::given(method("GET"))
        .and(path("/v1/channel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uuid": "test-uuid-123",
            "url": "wss://sfu.example.com/channel/test-uuid-123"
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
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["uuid"], "test-uuid-123");
    assert_eq!(body["url"], "wss://sfu.example.com/channel/test-uuid-123");
}
