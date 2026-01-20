#![deny(clippy::correctness)]
#![deny(clippy::suspicious)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::pedantic)]
#![warn(clippy::perf)]
#![warn(clippy::complexity)]
#![warn(clippy::style)]
#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::nonstandard_macro_braces)]
#![warn(clippy::option_if_let_else)]
#![warn(clippy::single_option_map)]
#![warn(clippy::type_repetition_in_bounds)]
#![warn(clippy::uninhabited_references)]
#![warn(clippy::unnecessary_struct_initialization)]
#![warn(clippy::use_self)]

mod auth;
mod balancer;
mod config;
mod geo;
mod handlers;

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::balancer::Balancer;
use crate::config::{GatewayConfig, NodeData};
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
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!("Failed to set tracing subscriber");
        std::process::exit(1);
    }
    let args = Args::parse();

    // Load gateway config from environment
    let gateway = GatewayConfig::from_env().unwrap_or_else(|e| {
        eprintln!("Error loading gateway config: {e}");
        std::process::exit(1);
    });

    // Load secrets: prioritize environment variable JSON over local file
    let nodes = if let Some(ref nodes_json) = gateway.nodes {
        info!("Loading SFU nodes from environment variable");
        NodeData::from_json(nodes_json).unwrap_or_else(|e| {
            eprintln!("Error parsing SFU nodes from environment: {e}");
            std::process::exit(1);
        })
    } else {
        info!("Loading SFU nodes from file: {}", args.secrets);
        NodeData::load(&args.secrets).unwrap_or_else(|e| {
            eprintln!("Error loading secrets file: {e}");
            std::process::exit(1);
        })
    };

    info!(
        bind = %gateway.bind,
        port = gateway.port,
        sfu_count = nodes.sfu.len(),
        "Starting SFU Gateway"
    );

    for sfu in &nodes.sfu {
        info!(
            address = %sfu.address,
            region = ?sfu.region,
            "Registered SFU"
        );
    }

    let state = Arc::new(AppState {
        balancer: Balancer::new(nodes.sfu),
        http_client: reqwest::Client::new(),
        gateway_key: gateway.key,
        trust_proxy: gateway.trust_proxy,
    });

    let bind_addr = format!("{}:{}", gateway.bind, gateway.port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/noop", web::get().to(handlers::noop))
            .route("/v1/channel", web::get().to(handlers::channel))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
