use axum::{routing::get, Json, Router};
use identity::handler::rest;
use identity::handler::IdentityHandler;
use identity::manager::biz::IdentityBiz;
use identity::manager::biz::NotificationEvent;
use identity::manager::repository::IdentityRepository;
use identity::pb::service::identity::identity_service_server::IdentityServiceServer;
use std::{net::SocketAddr, sync::Arc};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let rust_log = std::env::var("RUST_LOG").ok();
    philand_logging::init(
        "identity",
        rust_log
            .as_deref()
            .or(Some("identity=debug,tower_http=debug")),
    );

    let app_info = philand_application::from_env_with_prefix("IDENTITY_APP");
    tracing::info!("starting {}", app_info.user_agent());

    // Config
    let config = philand_configs::IdentityServiceConfig::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;
    tracing::info!(
        "Config loaded: gRPC={}, HTTP={}",
        config.grpc_port,
        config.http_port
    );

    // Database + migrations via shared storage lib
    let repo = IdentityRepository::new(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to init identity repository: {e}"))?;
    tracing::info!("Storage initialized");

    // Consul: register service and read KV config (best-effort)
    if let Err(e) = config.register_consul().await {
        tracing::warn!("Consul registration failed: {e}. Continuing without Consul.");
    }
    let consul_kv = match config.read_consul_kv().await {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("Consul KV read failed: {e}");
            std::collections::HashMap::new()
        }
    };
    if !consul_kv.is_empty() {
        tracing::info!("Consul KV overrides: {:?}", consul_kv);
    }

    // Wire layers: repository → biz → handler

    let notify_enabled = philand_env::bool_flag("NOTIFY_ENABLED", false);
    let (notify_tx, notify_rx) = philand_queue::bounded(256);
    if notify_enabled {
        spawn_notify_worker(notify_rx);
    }

    let biz = Arc::new(IdentityBiz::new(
        repo,
        config.clone(),
        if notify_enabled {
            Some(notify_tx)
        } else {
            None
        },
    ));

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

    if philand_env::bool_flag("ADMIN_SSH_CHECK_ENABLED", false) {
        let target = philand_ssh::SshTarget {
            user: std::env::var("ADMIN_SSH_USER").unwrap_or_else(|_| "root".to_string()),
            host: std::env::var("ADMIN_SSH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("ADMIN_SSH_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(22),
            identity_file: std::env::var("ADMIN_SSH_KEY").ok(),
        };

        if let Ok(cmd) = philand_ssh::build_ssh_command(&target, "echo identity health") {
            tracing::info!("ssh hook command prepared: {}", cmd.join(" "));
        }
    }

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

fn spawn_notify_worker(mut rx: philand_queue::QueueReceiver<NotificationEvent>) {
    let telegram_enabled = philand_env::bool_flag("NOTIFY_TELEGRAM_ENABLED", false);
    let bot_token = std::env::var("NOTIFY_TELEGRAM_BOT_TOKEN").ok();
    let chat_id = std::env::var("NOTIFY_TELEGRAM_CHAT_ID").ok();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        while let Some(event) = rx.recv().await {
            let ts = philand_time::now_unix();
            let text = match event {
                NotificationEvent::PasswordReset { email } => {
                    format!("[{ts}] Password reset requested for {email}.")
                }
                NotificationEvent::OrgInvitation {
                    email,
                    org_id,
                    invitation_id,
                } => format!("[{ts}] Org invitation for {email} in {org_id}. id={invitation_id}"),
            };

            if telegram_enabled {
                if let (Some(bt), Some(cid)) = (&bot_token, &chat_id) {
                    if let Err(err) =
                        philand_notify::send_telegram_message(&client, bt, cid, &text).await
                    {
                        tracing::warn!("telegram notify failed: {err}");
                    }
                }
            } else {
                tracing::info!("notify event: {text}");
            }
        }
    });
}

/// Health check endpoint.
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "identity"
    }))
}
