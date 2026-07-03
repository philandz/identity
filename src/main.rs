use axum::{routing::get, Json, Router};
use identity::handler::rest;
use identity::handler::IdentityHandler;
use identity::manager::biz::platform_settings;
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

    // Mailer — DB-stored Resend key (with env fallback). The DbKeyResolver
    // re-reads the platform_settings table per send so admin rotation from
    // the Super Admin → Global Settings page is observed without restart.
    let mailer: Arc<dyn philand_notify::Mailer> = {
        let placeholder_biz_for_key: Arc<IdentityBiz> = Arc::new(IdentityBiz::new(
            repo.clone(),
            config.clone(),
            None,
            Arc::new(philand_notify::NoopMailer::new()),
        ));
        let source = platform_settings::build_api_key_source(&placeholder_biz_for_key)
            .await
            .unwrap_or_else(|_| {
                philand_notify::ApiKeySource::Db(philand_notify::DbKeyResolver::new(|| None))
            });

        if matches!(source, philand_notify::ApiKeySource::Db(_)) {
            tracing::warn!(
                "Resend API key not configured (set platform_settings.resend_api_key or RESEND_API_KEY) — \
                 email delivery will be a no-op until configured."
            );
        }

        Arc::new(philand_notify::ResendMailer::new(source))
    };

    let biz = Arc::new(IdentityBiz::new(
        repo.clone(),
        config.clone(),
        if notify_enabled {
            Some(notify_tx)
        } else {
            None
        },
        mailer.clone(),
    ));

    // Overlay the DB-stored system config on top of env so the notify
    // worker uses the URLs/locale/From the super admin may have
    // overridden via /admin/settings without a service restart.
    biz.apply_db_system_config_at_startup().await;

    if notify_enabled {
        let telegram_enabled = philand_env::bool_flag("NOTIFY_TELEGRAM_ENABLED", false);
        let bot_token = std::env::var("NOTIFY_TELEGRAM_BOT_TOKEN").ok();
        let chat_id = std::env::var("NOTIFY_TELEGRAM_CHAT_ID").ok();
        spawn_notify_worker(
            notify_rx,
            mailer.clone(),
            biz.clone(),
            telegram_enabled,
            bot_token,
            chat_id,
        );
    }

    // Seed the initial super-admin user (idempotent — skips if already exists)
    if let Err(e) = biz.init_super_admin().await {
        tracing::warn!(
            "Failed to init super admin (may already exist): {}",
            e.message()
        );
    }

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

#[allow(clippy::too_many_arguments)]
fn spawn_notify_worker(
    mut rx: philand_queue::QueueReceiver<NotificationEvent>,
    mailer: Arc<dyn philand_notify::Mailer>,
    biz: Arc<identity::manager::biz::IdentityBiz>,
    telegram_enabled: bool,
    bot_token: Option<String>,
    chat_id: Option<String>,
) {
    if telegram_enabled && (bot_token.is_none() || chat_id.is_none()) {
        tracing::warn!(
            "NOTIFY_TELEGRAM_ENABLED=true but NOTIFY_TELEGRAM_BOT_TOKEN / \
             NOTIFY_TELEGRAM_CHAT_ID are not set — Telegram notifications will be silently dropped"
        );
    }

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        while let Some(event) = rx.recv().await {
            // Build a plain-text summary for the Telegram channel (cheap).
            let ts = philand_time::now_unix();
            let text = match &event {
                NotificationEvent::PasswordReset { email, .. } => {
                    format!("[{ts}] Password reset requested for {email}.")
                }
                NotificationEvent::OrgInvitation {
                    email,
                    org_name,
                    inviter_display_name,
                    ..
                } => format!(
                    "[{ts}] Org invitation: {inviter_display_name} -> {email} in {org_name}"
                ),
                NotificationEvent::PasswordChangeOtp { email, .. } => {
                    format!("[{ts}] Password-change OTP sent to {email}.")
                }
            };

            // Telegram fanout (best-effort, never blocks email path).
            if telegram_enabled {
                if let (Some(bt), Some(cid)) = (&bot_token, &chat_id) {
                    if let Err(err) =
                        philand_notify::send_telegram_message(&client, bt, cid, &text).await
                    {
                        tracing::warn!("telegram notify failed: {err}");
                    }
                }
            } else {
                tracing::debug!("notify event: {text}");
            }

            // Email fanout — renders the appropriate template and sends via
            // the mailer. Failures are logged and never block subsequent
            // events.
            let rendered = render_event_to_mail(&event, &biz).await;
            let outcome = match rendered {
                Some(msg) => match mailer.send(msg).await {
                    Ok(receipt) => {
                        tracing::info!(
                            "mailer dispatched (provider={}, message_id={})",
                            receipt.provider,
                            receipt.message_id
                        );
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!("mailer send failed: {e}");
                        Err(())
                    }
                },
                None => {
                    // Some events might not have a mail (future expansion).
                    tracing::debug!("no mail rendered for event variant");
                    Ok(())
                }
            };
            let _ = outcome;
        }
    });
}

/// Render a [`NotificationEvent`] into a [`philand_notify::MailMessage`].
/// Reads the live config each call so updates from the Super Admin → System
/// Config page take effect on the next email without a restart.
async fn render_event_to_mail(
    event: &NotificationEvent,
    biz: &Arc<identity::manager::biz::IdentityBiz>,
) -> Option<philand_notify::MailMessage> {
    let live = biz.live_config_snapshot().await;
    let from = live.mail_from_address.clone();
    let reply_to = if live.support_email.is_empty() {
        None
    } else {
        Some(live.support_email.clone())
    };

    match event {
        NotificationEvent::PasswordReset {
            email,
            raw_token,
            expires_at,
        } => {
            let reset_url = format!(
                "{}/{}/reset-password?token={}",
                live.app_public_base_url.trim_end_matches('/'),
                live.default_locale,
                raw_token
            );
            let rendered =
                philand_notify::render_password_reset(philand_notify::PasswordResetVars {
                    display_name: None,
                    reset_url: &reset_url,
                    ttl_human: "1 hour",
                    expires_at: *expires_at,
                    support_email: &live.support_email,
                });
            Some(rendered.into_mail(email.clone(), from, reply_to))
        }
        NotificationEvent::OrgInvitation {
            email,
            org_id: _,
            org_name,
            inviter_display_name,
            org_role_human,
            raw_token,
            expires_at,
        } => {
            let accept_url = format!(
                "{}/{}/accept-invitation?token={}",
                live.app_public_base_url.trim_end_matches('/'),
                live.default_locale,
                raw_token
            );
            let rendered =
                philand_notify::render_org_invitation(philand_notify::OrgInvitationVars {
                    invitee_email: email,
                    inviter_display_name,
                    org_name,
                    org_role_human,
                    accept_url: &accept_url,
                    ttl_human: "7 days",
                    expires_at: *expires_at,
                    support_email: &live.support_email,
                });
            Some(rendered.into_mail(email.clone(), from, reply_to))
        }
        NotificationEvent::PasswordChangeOtp {
            email,
            display_name,
            code,
            expires_at,
        } => {
            let rendered =
                philand_notify::render_password_change_otp(philand_notify::PasswordChangeOtpVars {
                    display_name: display_name.as_deref(),
                    code,
                    ttl_human: "10 minutes",
                    expires_at: *expires_at,
                    support_email: &live.support_email,
                });
            Some(rendered.into_mail(email.clone(), from, reply_to))
        }
    }
}

/// Health check endpoint.
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "identity"
    }))
}
