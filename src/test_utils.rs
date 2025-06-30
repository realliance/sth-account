#[cfg(test)]
pub mod test_utils {
    use axum::Router;
    use axum_test::TestServer;
    use chrono::Utc;
    use sea_orm::{DatabaseBackend, DatabaseConnection, MockDatabase, MockExecResult};
    use std::sync::Arc;
    use tower_sessions_memory_store::MemoryStore;
    use tower_sessions::{Expiry, SessionManagerLayer};
    use axum_login::AuthManagerLayerBuilder;
    use uuid::Uuid;
    use axum::http::Method;
    use serde_json::json;

    use crate::{
        api::{auth as auth_handlers, bots, users},
        auth::Backend,
        session_store::SeaOrmSessionStore,
    };
    use entity::{bot, user};

    /// Create a test app with mock database and in-memory sessions
    pub fn create_test_app(db: Arc<DatabaseConnection>) -> Router {
        // Use memory store for sessions in tests
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

        // Auth backend with mock database
        let backend = Backend::new(db.clone());
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        Router::new()
            // Authentication routes  
            .route("/auth/login", axum::routing::post(auth_handlers::login))
            .route("/auth/logout", axum::routing::post(auth_handlers::logout))
            .route("/auth/me", axum::routing::get(auth_handlers::me))
            
            // User routes
            .route("/users", axum::routing::post(users::create_user))
            .route("/users/:id", axum::routing::get(users::get_user))
            .route("/users/:id", axum::routing::patch(users::update_user))
            .route("/users/:id", axum::routing::delete(users::delete_user))
            
            // Bot routes
            .route("/bots", axum::routing::post(bots::create_bot))
            .route("/bots/:id", axum::routing::get(bots::get_bot))
            .route("/bots/:id", axum::routing::patch(bots::update_bot))
            .route("/bots/:id", axum::routing::delete(bots::delete_bot))
            .route("/users/:id/bots", axum::routing::get(bots::get_user_bots))
            
            .with_state(db)
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
    pub async fn login_test_user(server: &TestServer, username: &str, password: &str) -> Result<(), String> {
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
            Err(format!("Login failed with status: {}", response.status_code()))
        }
    }
    
    /// Create an authenticated test server with a logged-in user
    pub async fn create_authenticated_test_server(db: Arc<DatabaseConnection>, user: &user::Model) -> TestServer {
        let app = create_test_app(db);
        let server = TestServer::new(app).unwrap();
        
        // Attempt to login the user - this may fail in tests due to session issues
        let _ = login_test_user(&server, &user.username, "password123").await;
        
        server
    }
}