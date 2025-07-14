#[cfg(test)]
pub mod test_utils {
    use axum::Router;
    use axum_login::AuthManagerLayerBuilder;
    use chrono::Utc;
    use sea_orm::{DatabaseBackend, DatabaseConnection, MockDatabase, MockExecResult};
    use std::sync::Arc;
    use tower_sessions::{Expiry, SessionManagerLayer};
    use tower_sessions_memory_store::MemoryStore;
    use uuid::Uuid;

    use crate::{
        api::{
            admin, auth as auth_handlers, bots, exports, friends, lobbies, matches, matchmaking,
            notifications, rooms, users,
        },
        auth::Backend,
        health,
        queue::TestQueueProvider,
        service::AppState,
    };
    use entity::{bot, user};

    /// Create a test app with mock database and in-memory sessions
    pub fn create_test_app(db: Arc<DatabaseConnection>) -> Router {
        // Create test state with mock queue provider
        let state = AppState {
            db: db.clone(),
            queue: Arc::new(TestQueueProvider::new()),
        };

        // Use memory store for sessions in tests
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

        // Auth backend with mock database
        let backend = Backend::new(db.clone());
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        // Create API routes that match the main service structure
        let api_routes = Router::new()
            // Authentication routes
            .route("/auth/logout", axum::routing::post(auth_handlers::logout))
            .route("/auth/me", axum::routing::get(auth_handlers::me))
            // User routes (excluding public registration)
            .route("/users/{id}", axum::routing::get(users::get_user))
            .route("/users/{id}", axum::routing::patch(users::update_user))
            .route("/users/{id}", axum::routing::delete(users::delete_user))
            // Bot routes
            .route("/bots", axum::routing::post(bots::create_bot))
            .route("/bots/{id}", axum::routing::get(bots::get_bot))
            .route("/bots/{id}", axum::routing::patch(bots::update_bot))
            .route("/bots/{id}", axum::routing::delete(bots::delete_bot))
            .route("/users/{id}/bots", axum::routing::get(bots::get_user_bots))
            // Lobby routes (excluding public lobby list)
            .route("/lobbies", axum::routing::post(lobbies::create_lobby))
            .route("/lobbies/all", axum::routing::get(lobbies::get_all_lobbies))
            .route("/lobbies/{id}", axum::routing::get(lobbies::get_lobby))
            .route("/lobbies/{id}", axum::routing::patch(lobbies::update_lobby))
            .route(
                "/lobbies/{id}",
                axum::routing::delete(lobbies::delete_lobby),
            )
            // Matchmaking queue routes
            .route("/queue/join", axum::routing::post(matchmaking::join_queue))
            .route(
                "/queue/join-bot",
                axum::routing::post(matchmaking::join_queue_as_bot),
            )
            .route(
                "/queue/{id}",
                axum::routing::delete(matchmaking::leave_queue),
            )
            .route(
                "/queue/status",
                axum::routing::get(matchmaking::get_queue_status),
            )
            .route(
                "/queue/lobby/{id}/stats",
                axum::routing::get(matchmaking::get_lobby_queue_stats),
            )
            // Private room routes
            .route("/rooms", axum::routing::get(rooms::get_user_rooms))
            .route("/rooms", axum::routing::post(rooms::create_room))
            .route("/rooms/join", axum::routing::post(rooms::join_room))
            .route("/rooms/{id}", axum::routing::get(rooms::get_room))
            .route("/rooms/{id}/leave", axum::routing::post(rooms::leave_room))
            .route(
                "/rooms/{id}/invite",
                axum::routing::post(rooms::invite_to_room),
            )
            // Match and statistics routes
            .route(
                "/matches/user/{id}",
                axum::routing::get(matches::get_user_match_history),
            )
            .route(
                "/matches/bot/{id}",
                axum::routing::get(matches::get_bot_match_history),
            )
            .route("/matches/{id}", axum::routing::get(matches::get_match))
            .route(
                "/stats/user/{id}",
                axum::routing::get(matches::get_user_stats),
            )
            .route(
                "/stats/bot/{id}",
                axum::routing::get(matches::get_bot_stats),
            )
            // Friend system routes
            .route("/friends", axum::routing::get(friends::get_friends))
            .route(
                "/friends/requests",
                axum::routing::post(friends::send_friend_request),
            )
            .route(
                "/friends/requests",
                axum::routing::get(friends::get_pending_friend_requests),
            )
            .route(
                "/friends/requests/{id}",
                axum::routing::patch(friends::respond_to_friend_request),
            )
            .route(
                "/friends/{id}",
                axum::routing::delete(friends::remove_friend),
            )
            // Notification routes
            .route(
                "/notifications",
                axum::routing::get(notifications::get_notifications),
            )
            .route(
                "/notifications/mark-read",
                axum::routing::post(notifications::mark_notifications_as_read),
            )
            .route(
                "/notifications/mark-all-read",
                axum::routing::post(notifications::mark_all_notifications_as_read),
            )
            .route(
                "/notifications/summary",
                axum::routing::get(notifications::get_notification_summary),
            )
            .route(
                "/notifications/{id}",
                axum::routing::delete(notifications::delete_notification),
            )
            // Data export routes
            .route(
                "/exports",
                axum::routing::post(exports::request_data_export),
            )
            .route("/exports", axum::routing::get(exports::get_export_requests))
            .route(
                "/exports/{id}/download",
                axum::routing::get(exports::download_export),
            )
            .route(
                "/exports/{id}/cancel",
                axum::routing::delete(exports::cancel_export_request),
            )
            // Admin routes
            .route("/admin/reports", axum::routing::get(admin::get_reports))
            .route(
                "/admin/reports/{id}",
                axum::routing::patch(admin::update_report),
            )
            .route(
                "/admin/users/{id}/moderate",
                axum::routing::post(admin::moderate_user),
            )
            .route(
                "/admin/config",
                axum::routing::get(admin::get_system_config),
            )
            .route(
                "/admin/config",
                axum::routing::post(admin::update_system_config),
            )
            .route(
                "/admin/audit-logs",
                axum::routing::get(admin::get_audit_logs),
            )
            .route(
                "/admin/notifications",
                axum::routing::post(notifications::create_notification),
            )
            .route(
                "/admin/exports/{id}/complete",
                axum::routing::post(exports::complete_export),
            )
            .with_state(state.clone());

        // Create public routes
        let public_routes = Router::new()
            .route("/health", axum::routing::get(health::health_check))
            .route("/ready", axum::routing::get(health::readiness_check))
            .route("/auth/login", axum::routing::post(auth_handlers::login))
            .route("/users", axum::routing::post(users::create_user)) // Registration is public
            .route("/lobbies", axum::routing::get(lobbies::get_lobbies)) // Public lobby list
            .with_state(state);

        Router::new()
            .nest("/v1", api_routes)
            .merge(public_routes)
            .layer(auth_layer)
    }

    /// Create a mock database connection
    pub fn create_mock_db() -> MockDatabase {
        MockDatabase::new(DatabaseBackend::Postgres)
    }

    /// Create a mock database connection that fails ping operations
    pub fn create_failing_mock_db() -> MockDatabase {
        use sea_orm::DbErr;
        // Mock database that will fail any query (including ping)
        MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_errors([DbErr::Custom("Database connection failed".to_string())])
    }

    /// Create a sample user model for testing
    pub fn sample_user(id: Option<Uuid>) -> user::Model {
        user::Model {
            id: id.unwrap_or_else(Uuid::new_v4),
            username: "testuser".to_string(),
            country: "USA".to_string(),
            favorite_tile: Some("Man1".to_string()),
            pronouns: Some("they/them".to_string()),
            matchmaking_rank: 1000,
            // Properly hashed "password123" using argon2
            password: "$argon2id$v=19$m=19456,t=2,p=1$gZiV/M1gPc22ElAH/Jh1Hw$CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSABRRAOds".to_string(),
            created_at: Utc::now().into(),
            passkey: None,
            role: "Active".to_string(),
            email: Some("test@example.com".to_string()),
            last_active_at: Some(Utc::now().into()),
            account_status: "Active".to_string(),
            settings: None,
            deleted_at: None,
        }
    }

    /// Create a sample bot model for testing
    pub fn sample_bot(id: Option<Uuid>, owner_id: Uuid) -> bot::Model {
        bot::Model {
            id: id.unwrap_or_else(Uuid::new_v4),
            name: "testbot".to_string(),
            owner_id,
            source_code: Some("https://github.com/user/bot".to_string()),
            matchmaking_rank: 1000,
            api_key: "bot_key_123".to_string(),
            live: false,
            icon: Some("🤖".to_string()),
            description: Some("A test bot".to_string()),
            version: Some("1.0.0".to_string()),
            last_heartbeat: None,
            created_at: Utc::now().into(),
        }
    }

    /// Create a successful mock exec result
    pub fn mock_exec_success(last_insert_id: u64) -> MockExecResult {
        MockExecResult {
            last_insert_id,
            rows_affected: 1,
        }
    }











    /// Create an admin user for testing
    pub fn sample_admin_user(id: Option<Uuid>) -> user::Model {
        let mut user = sample_user(id);
        user.role = "Admin".to_string();
        user
    }

    /// Create a moderator user for testing
    pub fn sample_moderator_user(id: Option<Uuid>) -> user::Model {
        let mut user = sample_user(id);
        user.role = "Moderator".to_string();
        user
    }
}
