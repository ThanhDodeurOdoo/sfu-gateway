mod common;

use actix_web::{App, http::StatusCode, test, web};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{GATEWAY_KEY, create_app_state, make_test_claims, sign_claims};
use sfu_gateway::config::SfuConfig;
use sfu_gateway::http::channel;

const SFU_KEY_EU: &[u8] = b"sfu-key-eu-padded-to-32-bytes!!";
const SFU_KEY_US: &[u8] = b"sfu-key-us-padded-to-32-bytes!!";

fn multi_region_sfus(eu_address: &str, us_address: &str) -> Vec<SfuConfig> {
    vec![
        SfuConfig {
            address: eu_address.to_string(),
            region: Some("eu-west".to_string()),
            key: SFU_KEY_EU.to_vec(),
        },
        SfuConfig {
            address: us_address.to_string(),
            region: Some("us-east".to_string()),
            key: SFU_KEY_US.to_vec(),
        },
    ]
}

async fn setup_mock_sfu(server: &MockServer, uuid: &str, url: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/channel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uuid": uuid,
            "url": url
        })))
        .mount(server)
        .await;
}

#[actix_web::test]
async fn test_region_routing_selects_correct_sfu() {
    let mock_eu = MockServer::start().await;
    let mock_us = MockServer::start().await;

    setup_mock_sfu(&mock_eu, "eu-channel", "wss://eu.sfu.example.com").await;
    setup_mock_sfu(&mock_us, "us-channel", "wss://us.sfu.example.com").await;

    let state = create_app_state(
        multi_region_sfus(&mock_eu.uri(), &mock_us.uri()),
        GATEWAY_KEY,
        false,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let token = sign_claims(&make_test_claims(), GATEWAY_KEY);

    let req = test::TestRequest::get()
        .uri("/v1/channel?region=eu-west")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["uuid"], "eu-channel");
}

#[actix_web::test]
async fn test_country_routing_maps_to_region() {
    let mock_eu = MockServer::start().await;
    let mock_us = MockServer::start().await;

    setup_mock_sfu(&mock_eu, "eu-channel", "wss://eu.sfu.example.com").await;
    setup_mock_sfu(&mock_us, "us-channel", "wss://us.sfu.example.com").await;

    let state = create_app_state(
        multi_region_sfus(&mock_eu.uri(), &mock_us.uri()),
        GATEWAY_KEY,
        false,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/v1/channel", web::get().to(channel)),
    )
    .await;

    let token = sign_claims(&make_test_claims(), GATEWAY_KEY);

    let req = test::TestRequest::get()
        .uri("/v1/channel?country=FR")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["uuid"], "eu-channel", "France should route to EU");
}
