mod auth;
mod balancer;
mod config;
mod handlers;

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::balancer::Balancer;
use crate::config::{GatewayConfig, SecretsFile};
use crate::handlers::AppState;

#[derive(Parser, Debug)]
#[command(name = "sfu-gateway")]
#[command(about = "Gateway/load-balancer for SFU instances")]
struct Args {
    /// Path to secrets file containing SFU entries
    #[arg(short, long, default_value = "secrets.toml")]
    secrets: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
    let args = Args::parse();

    // Load gateway config from environment
    let gateway = GatewayConfig::from_env().unwrap_or_else(|e| {
        eprintln!("Error loading gateway config: {}", e);
        std::process::exit(1);
    });

    // Load secrets file
    let secrets = SecretsFile::load(&args.secrets).unwrap_or_else(|e| {
        eprintln!("Error loading secrets file: {}", e);
        std::process::exit(1);
    });

    info!(
        bind = %gateway.bind,
        port = gateway.port,
        sfu_count = secrets.sfu.len(),
        "Starting SFU Gateway"
    );

    for sfu in &secrets.sfu {
        info!(
            address = %sfu.address,
            region = ?sfu.region,
            "Registered SFU"
        );
    }

    let state = Arc::new(AppState {
        balancer: Balancer::new(secrets.sfu),
        http_client: reqwest::Client::new(),
        gateway_key: gateway.key,
    });

    let bind_addr = format!("{}:{}", gateway.bind, gateway.port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/health", web::get().to(handlers::health))
            .route("/v1/channel", web::get().to(handlers::channel))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
