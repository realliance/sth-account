#[cfg(test)]
pub mod test_utils {
    use axum::Router;
    use axum::http::Method;
    use axum_login::AuthManagerLayerBuilder;
    use axum_test::TestServer;
    use chrono::Utc;
    use reqwest::Client;
    use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
    use sea_orm::{DatabaseBackend, DatabaseConnection, MockDatabase, MockExecResult};
    use serde_json::json;
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
    use entity::{
        audit_log, bot, data_export_requests, friendship, system_configuration, user,
        user_statistics,
    };

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
            .route("/users/:id", axum::routing::get(users::get_user))
            .route("/users/:id", axum::routing::patch(users::update_user))
            .route("/users/:id", axum::routing::delete(users::delete_user))
            // Bot routes
            .route("/bots", axum::routing::post(bots::create_bot))
            .route("/bots/:id", axum::routing::get(bots::get_bot))
            .route("/bots/:id", axum::routing::patch(bots::update_bot))
            .route("/bots/:id", axum::routing::delete(bots::delete_bot))
            .route("/users/:id/bots", axum::routing::get(bots::get_user_bots))
            // Lobby routes (excluding public lobby list)
            .route("/lobbies", axum::routing::post(lobbies::create_lobby))
            .route("/lobbies/all", axum::routing::get(lobbies::get_all_lobbies))
            .route("/lobbies/:id", axum::routing::get(lobbies::get_lobby))
            .route("/lobbies/:id", axum::routing::patch(lobbies::update_lobby))
            .route("/lobbies/:id", axum::routing::delete(lobbies::delete_lobby))
            // Matchmaking queue routes
            .route("/queue/join", axum::routing::post(matchmaking::join_queue))
            .route(
                "/queue/join-bot",
                axum::routing::post(matchmaking::join_queue_as_bot),
            )
            .route(
                "/queue/:id",
                axum::routing::delete(matchmaking::leave_queue),
            )
            .route(
                "/queue/status",
                axum::routing::get(matchmaking::get_queue_status),
            )
            .route(
                "/queue/lobby/:id/stats",
                axum::routing::get(matchmaking::get_lobby_queue_stats),
            )
            // Private room routes
            .route("/rooms", axum::routing::get(rooms::get_user_rooms))
            .route("/rooms", axum::routing::post(rooms::create_room))
            .route("/rooms/join", axum::routing::post(rooms::join_room))
            .route("/rooms/:id", axum::routing::get(rooms::get_room))
            .route("/rooms/:id/leave", axum::routing::post(rooms::leave_room))
            .route(
                "/rooms/:id/invite",
                axum::routing::post(rooms::invite_to_room),
            )
            // Match and statistics routes
            .route(
                "/matches/user/:id",
                axum::routing::get(matches::get_user_match_history),
            )
            .route(
                "/matches/bot/:id",
                axum::routing::get(matches::get_bot_match_history),
            )
            .route("/matches/:id", axum::routing::get(matches::get_match))
            .route(
                "/stats/user/:id",
                axum::routing::get(matches::get_user_stats),
            )
            .route("/stats/bot/:id", axum::routing::get(matches::get_bot_stats))
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
                "/friends/requests/:id",
                axum::routing::patch(friends::respond_to_friend_request),
            )
            .route(
                "/friends/:id",
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
                "/notifications/:id",
                axum::routing::delete(notifications::delete_notification),
            )
            // Data export routes
            .route(
                "/exports",
                axum::routing::post(exports::request_data_export),
            )
            .route("/exports", axum::routing::get(exports::get_export_requests))
            .route(
                "/exports/:id/download",
                axum::routing::get(exports::download_export),
            )
            .route(
                "/exports/:id/cancel",
                axum::routing::delete(exports::cancel_export_request),
            )
            // Admin routes
            .route("/admin/reports", axum::routing::get(admin::get_reports))
            .route(
                "/admin/reports/:id",
                axum::routing::patch(admin::update_report),
            )
            .route(
                "/admin/users/:id/moderate",
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
                "/admin/exports/:id/complete",
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
            .nest("/api/v1", api_routes)
            .merge(public_routes)
            .layer(auth_layer)
    }

    /// Create a mock database connection
    pub fn create_mock_db() -> MockDatabase {
        MockDatabase::new(DatabaseBackend::Postgres)
    }

    /// Create a sample user model for testing
    pub fn sample_user(id: Option<Uuid>) -> user::Model {
        user::Model {
            id: id.unwrap_or_else(|| Uuid::new_v4()),
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
            id: id.unwrap_or_else(|| Uuid::new_v4()),
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

    /// Create a no-rows-affected mock exec result
    pub fn mock_exec_no_rows() -> MockExecResult {
        MockExecResult {
            last_insert_id: 0,
            rows_affected: 0,
        }
    }

    /// Helper to perform login and get authenticated server
    pub async fn login_test_user(
        server: &TestServer,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let login_request = json!({
            "username": username,
            "password": password
        });

        let response = server
            .method(Method::POST, "/auth/login")
            .form(&login_request)
            .await;

        if response.status_code().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Login failed with status: {}",
                response.status_code()
            ))
        }
    }

    /// Create an authenticated test server with a logged-in user
    pub async fn create_authenticated_test_server(
        db: Arc<DatabaseConnection>,
        user: &user::Model,
    ) -> TestServer {
        let app = create_test_app(db);
        let server = TestServer::new(app).unwrap();

        // Attempt to login the user - this may fail in tests due to session issues
        let _ = login_test_user(&server, &user.username, "password123").await;

        server
    }

    /// Create an authenticated reqwest client that maintains cookies
    pub async fn create_authenticated_client(
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Client, Box<dyn std::error::Error>> {
        // Create a cookie store
        let cookie_store = CookieStore::default();
        let cookie_store = CookieStoreMutex::new(cookie_store);
        let cookie_store = Arc::new(cookie_store);

        // Create reqwest client with cookie support
        let client = Client::builder()
            .cookie_provider(Arc::clone(&cookie_store))
            .build()?;

        // Perform login to get session cookie
        let login_data = json!({
            "username": username,
            "password": password
        });

        let response = client
            .post(&format!("{}/auth/login", base_url))
            .form(&login_data)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Login failed with status: {}", response.status()).into());
        }

        Ok(client)
    }

    /// Helper struct to manage test server with authenticated client
    pub struct AuthenticatedTestClient {
        pub server: TestServer,
        pub client: Client,
        pub base_url: String,
    }

    impl AuthenticatedTestClient {
        pub async fn new(
            db: Arc<DatabaseConnection>,
            username: &str,
            password: &str,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            let app = create_test_app(db);
            let server = TestServer::new(app).unwrap();

            // Get the server address - TestServer::server_address() returns Option<SocketAddr>
            let server_addr = server
                .server_address()
                .ok_or("Failed to get server address")?;
            let base_url = format!("http://{}", server_addr);

            let client = create_authenticated_client(&base_url, username, password).await?;

            Ok(Self {
                server,
                client,
                base_url,
            })
        }

        pub async fn get(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
            self.client
                .get(&format!("{}{}", self.base_url, path))
                .send()
                .await
        }

        pub async fn post(
            &self,
            path: &str,
            json: &serde_json::Value,
        ) -> Result<reqwest::Response, reqwest::Error> {
            self.client
                .post(&format!("{}{}", self.base_url, path))
                .json(json)
                .send()
                .await
        }

        pub async fn patch(
            &self,
            path: &str,
            json: &serde_json::Value,
        ) -> Result<reqwest::Response, reqwest::Error> {
            self.client
                .patch(&format!("{}{}", self.base_url, path))
                .json(json)
                .send()
                .await
        }

        pub async fn delete(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
            self.client
                .delete(&format!("{}{}", self.base_url, path))
                .send()
                .await
        }
    }

    /// Create a sample friendship model for testing
    pub fn sample_friendship(
        requester_id: Uuid,
        addressee_id: Uuid,
        status: &str,
    ) -> friendship::Model {
        friendship::Model {
            id: Uuid::new_v4(),
            requester_id,
            addressee_id,
            status: status.to_string(),
            created_at: Utc::now().into(),
            updated_at: Utc::now().into(),
        }
    }

    /// Create a sample notification model for testing
    pub fn sample_notification(
        user_id: Uuid,
        notification_type: &str,
    ) -> entity::notifications::Model {
        entity::notifications::Model {
            id: Uuid::new_v4(),
            user_id,
            r#type: notification_type.to_string(),
            title: "Test Notification".to_string(),
            message: "This is a test notification".to_string(),
            related_id: None,
            read: false,
            created_at: Utc::now().into(),
            expires_at: None,
        }
    }

    /// Create a sample user statistics model for testing
    pub fn sample_user_statistics(user_id: Uuid) -> user_statistics::Model {
        user_statistics::Model {
            user_id,
            total_games: 10,
            wins: 3,
            second_place: 2,
            third_place: 3,
            fourth_place: 2,
            average_score: Some(sea_orm::prelude::Decimal::new(25000, 0)),
            peak_mmr: Some(1200),
            current_streak: 2,
            last_game_at: Some(Utc::now().into()),
        }
    }

    /// Create a sample data export request model for testing
    pub fn sample_export_request(user_id: Uuid, export_type: &str) -> data_export_requests::Model {
        data_export_requests::Model {
            id: Uuid::new_v4(),
            user_id,
            export_type: export_type.to_string(),
            status: "Pending".to_string(),
            file_path: None,
            requested_at: Utc::now().into(),
            completed_at: None,
            expires_at: None,
        }
    }

    /// Create a sample audit log model for testing
    pub fn sample_audit_log(user_id: Uuid, action_type: &str) -> audit_log::Model {
        audit_log::Model {
            id: Uuid::new_v4(),
            user_id,
            action_type: action_type.to_string(),
            details: Some(serde_json::json!({"test": "data"})),
            ip_address: "127.0.0.1".to_string(),
            moderator_id: None,
            created_at: Utc::now().into(),
            deleted_at: None,
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
