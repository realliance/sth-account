use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use axum_login::AuthManagerLayerBuilder;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tower_sessions::{Expiry, SessionManagerLayer};
use tracing::{debug, info};
use utoipa::openapi::{Info, OpenApi};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    api::{
        admin, auth as auth_handlers, bots, exports, friends, lobbies, matches, matchmaking,
        notifications, rooms, users,
    },
    auth::Backend,
    config::Config,
    database,
    error::Result,
    health, openapi,
    queue::{QueueProvider, RealQueueProvider, TestQueueProvider},
    session_store::SeaOrmSessionStore,
};
use sea_orm::DatabaseConnection;
use std::env;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DatabaseConnection>,
    pub queue: Arc<dyn QueueProvider>,
}

pub async fn run_service() -> Result<()> {
    info!("Starting STH Account Service");
    let config = Config::from_env()?;
    debug!("Configuration loaded successfully");

    let db = Arc::new(database::establish_connection(&config).await?);
    info!("Database connection established");

    // Initialize queue provider
    debug!("Initializing queue provider");
    let queue_provider: Arc<dyn QueueProvider> =
        if env::var("ENVIRONMENT").unwrap_or_default() == "test" {
            info!("Using test queue provider");
            Arc::new(TestQueueProvider::new())
        } else {
            info!("Using real queue provider");
            let real_provider = RealQueueProvider::new();
            // Try to connect to RabbitMQ if configured
            debug!("Checking for RabbitMQ configuration");
            if let (Ok(host), Ok(port), Ok(username), Ok(password)) = (
                env::var("RABBITMQ_HOST"),
                env::var("RABBITMQ_PORT")
                    .and_then(|p| p.parse::<u16>().map_err(|_| env::VarError::NotPresent)),
                env::var("RABBITMQ_USERNAME"),
                env::var("RABBITMQ_PASSWORD"),
            ) {
                info!("Attempting to connect to RabbitMQ at {}:{}", host, port);
                if let Err(e) = real_provider
                    .connect(&host, port, &username, &password)
                    .await
                {
                    tracing::warn!("Failed to connect to RabbitMQ: {}", e);
                } else {
                    info!("Successfully connected to RabbitMQ");
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

    // API routes with OpenAPI collection
    let api_routes = OpenApiRouter::with_openapi(
        OpenApi::builder()
            .info(
                Info::builder()
                    .title("Small Turtle House Account and Lobby APIs")
                    .description(Some(
                        "APIs for managing user accounts, lobbies, matchmaking, and more.",
                    ))
                    .version(env!("CARGO_PKG_VERSION"))
                    .build(),
            )
            .build(),
    )
    // All OpenAPI annotated routes
    .routes(routes!(auth_handlers::login))
    .routes(routes!(auth_handlers::logout))
    .routes(routes!(auth_handlers::me))
    .routes(routes!(
        users::create_user,
        users::get_user,
        users::update_user,
        users::delete_user
    ))
    .routes(routes!(
        bots::create_bot,
        bots::get_bot,
        bots::update_bot,
        bots::delete_bot
    ))
    .routes(routes!(bots::get_user_bots))
    .routes(routes!(lobbies::get_all_lobbies))
    .routes(routes!(lobbies::get_lobbies))
    .routes(routes!(
        lobbies::create_lobby,
        lobbies::get_lobby,
        lobbies::update_lobby,
        lobbies::delete_lobby
    ))
    // Matchmaking queue routes
    .routes(routes!(matchmaking::join_queue))
    .routes(routes!(matchmaking::join_queue_as_bot))
    .routes(routes!(matchmaking::leave_queue))
    .routes(routes!(matchmaking::get_queue_status))
    .routes(routes!(matchmaking::get_lobby_queue_stats))
    // Private room routes
    .routes(routes!(rooms::create_room))
    .routes(routes!(rooms::join_room))
    .routes(routes!(rooms::leave_room))
    .routes(routes!(rooms::get_user_rooms))
    .routes(routes!(rooms::invite_to_room))
    .routes(routes!(rooms::get_room))
    // Match and statistics routes
    .routes(routes!(matches::get_user_match_history))
    .routes(routes!(matches::get_bot_match_history))
    .routes(routes!(matches::get_match))
    .routes(routes!(matches::get_user_stats))
    .routes(routes!(matches::get_bot_stats))
    // Friend system routes
    .routes(routes!(friends::get_friends, friends::remove_friend))
    .routes(routes!(
        friends::send_friend_request,
        friends::get_pending_friend_requests,
        friends::respond_to_friend_request
    ))
    // Notification routes
    .routes(routes!(
        notifications::get_notifications,
        notifications::create_notification,
    ))
    .routes(routes!(notifications::mark_notifications_as_read))
    .routes(routes!(notifications::mark_all_notifications_as_read))
    .routes(routes!(notifications::get_notification_summary))
    .routes(routes!(notifications::delete_notification))
    // Data export routes
    .routes(routes!(
        exports::request_data_export,
        exports::get_export_requests,
        exports::cancel_export_request
    ))
    .routes(routes!(exports::download_export, exports::complete_export))
    // Admin routes
    .routes(routes!(
        admin::get_reports,
        admin::update_report,
        admin::moderate_user,
    ))
    .routes(routes!(
        admin::get_system_config,
        admin::update_system_config,
    ))
    .routes(routes!(admin::get_audit_logs,))
    .with_state(state.clone());

    // Extract OpenAPI spec from the router
    let (api_router, openapi_spec) = api_routes.split_for_parts();

    // Public routes (no auth required) - only non-API routes
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .with_state(state);

    // CORS configuration - allow credentials requires specific origins
    let allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| {
            "http://localhost:3000,http://localhost:3001,http://127.0.0.1:3000".to_string()
        })
        .split(',')
        .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    info!("CORS allowed origins: {:?}", allowed_origins);

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::USER_AGENT,
            HeaderName::from_static("x-requested-with"),
        ])
        .allow_credentials(true);

    // Enhanced trace layer with detailed logging
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
        .on_response(DefaultOnResponse::new().level(tracing::Level::INFO));

    let app = Router::new()
        .nest("/api/v1", api_router)
        .merge(public_routes)
        .merge(openapi::add_docs_routes(Router::new(), openapi_spec))
        .layer(
            ServiceBuilder::new()
                .layer(trace_layer)
                .layer(cors)
                .layer(auth_layer),
        );

    let bind_addr = format!("{}:{}", config.bind_address, config.bind_port);
    info!("Starting service on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("✅ Service ready! Listening on http://{}", bind_addr);
    info!(
        "📄 OpenAPI spec available at: http://{}/openapi.json",
        bind_addr
    );

    axum::serve(listener, app).await?;

    Ok(())
}
