use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use axum_login::AuthManagerLayerBuilder;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_sessions::{Expiry, SessionManagerLayer};
use tracing::info;

use crate::{
    api::{admin, auth as auth_handlers, bots, exports, friends, lobbies, matches, matchmaking, notifications, rooms, users},
    auth::Backend,
    config::Config,
    database,
    error::Result,
    health,
    queue::{QueueProvider, RealQueueProvider, TestQueueProvider},
    session_store::SeaOrmSessionStore,
};
use sea_orm::DatabaseConnection;
use std::{env, sync::Arc};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DatabaseConnection>,
    pub queue: Arc<dyn QueueProvider>,
}

pub async fn run_service() -> Result<()> {
    let config = Config::from_env()?;
    let db = Arc::new(database::establish_connection(&config).await?);

    // Initialize queue provider
    let queue_provider: Arc<dyn QueueProvider> =
        if env::var("ENVIRONMENT").unwrap_or_default() == "test" {
            Arc::new(TestQueueProvider::new())
        } else {
            let real_provider = RealQueueProvider::new();
            // Try to connect to RabbitMQ if configured
            if let (Ok(host), Ok(port), Ok(username), Ok(password)) = (
                env::var("RABBITMQ_HOST"),
                env::var("RABBITMQ_PORT")
                    .and_then(|p| p.parse::<u16>().map_err(|_| env::VarError::NotPresent)),
                env::var("RABBITMQ_USERNAME"),
                env::var("RABBITMQ_PASSWORD"),
            ) {
                if let Err(e) = real_provider
                    .connect(&host, port, &username, &password)
                    .await
                {
                    tracing::warn!("Failed to connect to RabbitMQ: {}", e);
                }
            }
            Arc::new(real_provider)
        };

    // Application state
    let state = AppState {
        db: db.clone(),
        queue: queue_provider,
    };

    // Session store using our custom SeaORM implementation
    let session_store = SeaOrmSessionStore::new(db.clone());

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Set to true in production with HTTPS
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    // Auth backend
    let backend = Backend::new(db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    // API routes
    let api_routes = Router::new()
        // Authentication routes
        .route("/auth/login", post(auth_handlers::login))
        .route("/auth/logout", post(auth_handlers::logout))
        .route("/auth/me", get(auth_handlers::me))
        // User routes
        .route("/users", post(users::create_user))
        .route("/users/:id", get(users::get_user))
        .route("/users/:id", patch(users::update_user))
        .route("/users/:id", delete(users::delete_user))
        // Bot routes
        .route("/bots", post(bots::create_bot))
        .route("/bots/:id", get(bots::get_bot))
        .route("/bots/:id", patch(bots::update_bot))
        .route("/bots/:id", delete(bots::delete_bot))
        .route("/users/:id/bots", get(bots::get_user_bots))
        // Lobby routes
        .route("/lobbies", get(lobbies::get_lobbies))
        .route("/lobbies", post(lobbies::create_lobby))
        .route("/lobbies/all", get(lobbies::get_all_lobbies))
        .route("/lobbies/:id", get(lobbies::get_lobby))
        .route("/lobbies/:id", patch(lobbies::update_lobby))
        .route("/lobbies/:id", delete(lobbies::delete_lobby))
        // Matchmaking queue routes
        .route("/queue/join", post(matchmaking::join_queue))
        .route("/queue/join-bot", post(matchmaking::join_queue_as_bot))
        .route("/queue/:id", delete(matchmaking::leave_queue))
        .route("/queue/status", get(matchmaking::get_queue_status))
        .route(
            "/queue/lobby/:id/stats",
            get(matchmaking::get_lobby_queue_stats),
        )
        // Private room routes
        .route("/rooms", get(rooms::get_user_rooms))
        .route("/rooms", post(rooms::create_room))
        .route("/rooms/join", post(rooms::join_room))
        .route("/rooms/:id", get(rooms::get_room))
        .route("/rooms/:id/leave", post(rooms::leave_room))
        .route("/rooms/:id/invite", post(rooms::invite_to_room))
        // Match and statistics routes
        .route("/matches/user/:id", get(matches::get_user_match_history))
        .route("/matches/bot/:id", get(matches::get_bot_match_history))
        .route("/matches/:id", get(matches::get_match))
        .route("/stats/user/:id", get(matches::get_user_stats))
        .route("/stats/bot/:id", get(matches::get_bot_stats))
        // Friend system routes
        .route("/friends", get(friends::get_friends))
        .route("/friends/requests", post(friends::send_friend_request))
        .route("/friends/requests", get(friends::get_pending_friend_requests))
        .route("/friends/requests/:id", patch(friends::respond_to_friend_request))
        .route("/friends/:id", delete(friends::remove_friend))
        // Notification routes
        .route("/notifications", get(notifications::get_notifications))
        .route("/notifications/mark-read", post(notifications::mark_notifications_as_read))
        .route("/notifications/mark-all-read", post(notifications::mark_all_notifications_as_read))
        .route("/notifications/summary", get(notifications::get_notification_summary))
        .route("/notifications/:id", delete(notifications::delete_notification))
        // Data export routes
        .route("/exports", post(exports::request_data_export))
        .route("/exports", get(exports::get_export_requests))
        .route("/exports/:id/download", get(exports::download_export))
        .route("/exports/:id/cancel", delete(exports::cancel_export_request))
        // Admin routes
        .route("/admin/reports", get(admin::get_reports))
        .route("/admin/reports/:id", patch(admin::update_report))
        .route("/admin/users/:id/moderate", post(admin::moderate_user))
        .route("/admin/config", get(admin::get_system_config))
        .route("/admin/config", post(admin::update_system_config))
        .route("/admin/audit-logs", get(admin::get_audit_logs))
        .route("/admin/notifications", post(notifications::create_notification))
        .route("/admin/exports/:id/complete", post(exports::complete_export))
        .with_state(state.clone());

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .route("/auth/login", post(auth_handlers::login))
        .route("/users", post(users::create_user)) // Registration is public
        .route("/lobbies", get(lobbies::get_lobbies)) // Public lobby list
        .with_state(state);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .merge(public_routes)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(auth_layer),
        );

    let bind_addr = format!("{}:{}", config.bind_address, config.bind_port);
    info!("Starting service on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
