use axum::{routing::get, Json, Router};
use identity::config::AppConfig;
use identity::handler::rest;
use identity::handler::IdentityHandler;
use identity::manager::biz::IdentityBiz;
use identity::manager::repository::IdentityRepository;
use identity::pb::service::identity::identity_service_server::IdentityServiceServer;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "identity=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Config
    let config = AppConfig::from_env();
    tracing::info!(
        "Config loaded: gRPC={}, HTTP={}",
        config.grpc_port,
        config.http_port
    );

    // Database
    let pool = sqlx::MySqlPool::connect(&config.database_url).await?;
    let pool = Arc::new(pool);

    // Migrations
    sqlx::migrate!("./migrations").run(&*pool).await?;
    tracing::info!("Migrations applied");

    // Consul: register service and read KV config (best-effort)
    config.register_consul().await?;
    let consul_kv = config.read_consul_kv().await;
    if !consul_kv.is_empty() {
        tracing::info!("Consul KV overrides: {:?}", consul_kv);
    }

    // Wire layers: repository → biz → handler
    let repo = IdentityRepository::new(pool.clone());
    let biz = Arc::new(IdentityBiz::new(repo, config.clone()));

    // Seed the initial super-admin user (idempotent — skips if already exists)
    biz.init_super_admin()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to init super admin: {}", e.message()))?;

    let grpc_handler = IdentityHandler::new(biz.clone());

    // gRPC server
    let grpc_addr: SocketAddr = format!("{}:{}", config.grpc_host, config.grpc_port).parse()?;
    let grpc_server = tonic::transport::Server::builder()
        .add_service(IdentityServiceServer::new(grpc_handler))
        .serve(grpc_addr);
    tracing::info!("gRPC server listening on {}", grpc_addr);

    // HTTP server (REST API + health + OpenAPI + Swagger UI)
    let http_addr: SocketAddr = format!("{}:{}", config.http_host, config.http_port).parse()?;

    let mut openapi = rest::ApiDoc::openapi();
    // Merge health check into the spec
    openapi.paths.paths.insert(
        "/health".to_string(),
        utoipa::openapi::PathItem::new(
            utoipa::openapi::path::HttpMethod::Get,
            utoipa::openapi::path::OperationBuilder::new()
                .summary(Some("Health check endpoint"))
                .tag("health")
                .response(
                    "200",
                    utoipa::openapi::ResponseBuilder::new()
                        .description("Service is healthy")
                        .build(),
                )
                .build(),
        ),
    );

    let http_app = Router::new()
        .route("/health", get(health_check))
        .merge(rest::router())
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .with_state(biz);

    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("HTTP server listening on {}", http_addr);

    // Run both concurrently
    tokio::select! {
        res = grpc_server => {
            if let Err(e) = res {
                tracing::error!("gRPC server error: {}", e);
            }
        }
        res = axum::serve(http_listener, http_app) => {
            if let Err(e) = res {
                tracing::error!("HTTP server error: {}", e);
            }
        }
    }

    Ok(())
}

/// Health check endpoint.
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "identity"
    }))
}
