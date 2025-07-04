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
    api::{auth as auth_handlers, bots, lobbies, matches, matchmaking, rooms, users},
    auth::Backend,
    config::Config,
    database,
    error::Result,
    health,
    session_store::SeaOrmSessionStore,
};
use std::sync::Arc;

pub async fn run_service() -> Result<()> {
    let config = Config::from_env()?;
    let db = Arc::new(database::establish_connection(&config).await?);

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
        .with_state(db.clone());

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .route("/auth/login", post(auth_handlers::login))
        .route("/users", post(users::create_user)) // Registration is public
        .route("/lobbies", get(lobbies::get_lobbies)) // Public lobby list
        .with_state(db);

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
