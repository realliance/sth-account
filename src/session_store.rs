use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;
use tower_sessions::{
    session::{Id, Record},
    session_store::{self, ExpiredDeletion, SessionStore},
};
use uuid::Uuid;

use entity::user_session;

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub user_id: Option<Uuid>,
    pub device_info: Option<String>,
    pub ip_address: Option<String>,
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            user_id: None,
            device_info: None,
            ip_address: None,
        }
    }

    pub fn with_user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_device_info(mut self, device_info: String) -> Self {
        self.device_info = Some(device_info);
        self
    }

    pub fn with_ip_address(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }
}

#[derive(Debug, Clone)]
pub struct SeaOrmSessionStore {
    db: Arc<DatabaseConnection>,
    context: Option<SessionContext>,
}

impl SeaOrmSessionStore {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { 
            db,
            context: None,
        }
    }

    pub fn with_context(mut self, context: SessionContext) -> Self {
        self.context = Some(context);
        self
    }
}

#[async_trait]
impl SessionStore for SeaOrmSessionStore {
    async fn create(&self, session_record: &mut Record) -> session_store::Result<()> {
        // Handle ID collision by checking if session already exists
        while user_session::Entity::find()
            .filter(user_session::Column::TokenHash.eq(session_record.id.to_string()))
            .one(self.db.as_ref())
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?
            .is_some()
        {
            // Generate new ID if collision detected
            session_record.id = Id::default();
        }

        self.save(session_record).await
    }

    async fn save(&self, session_record: &Record) -> session_store::Result<()> {
        let session_id = session_record.id.to_string();
        let data_json = serde_json::to_string(&session_record.data)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;

        // Convert time::OffsetDateTime to chrono::DateTime
        let expiry_chrono = chrono::DateTime::from_timestamp(
            session_record.expiry_date.unix_timestamp(),
            session_record.expiry_date.nanosecond(),
        )
        .ok_or_else(|| session_store::Error::Backend("Invalid expiry date".to_string()))?
        .fixed_offset();

        // Check if session already exists
        let existing_session = user_session::Entity::find()
            .filter(user_session::Column::TokenHash.eq(&session_id))
            .one(self.db.as_ref())
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        if let Some(session) = existing_session {
            // Update existing session
            let mut session_update: user_session::ActiveModel = session.into();
            session_update.expires_at = Set(expiry_chrono);
            session_update.last_active_at = Set(Some(Utc::now().into()));
            session_update.status = Set("Active".to_string());
            session_update.data = Set(Some(data_json));
            session_update
                .update(self.db.as_ref())
                .await
                .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        } else {
            // Extract user_id from session data, then from context, fallback to placeholder if not found
            let user_id = session_record.data.get("user_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .or_else(|| self.context.as_ref().and_then(|c| c.user_id))
                .unwrap_or_else(|| Uuid::new_v4());

            // Get device info and IP address from context if available
            let device_info = self.context.as_ref().and_then(|c| c.device_info.clone());
            let ip_address = self.context.as_ref()
                .and_then(|c| c.ip_address.clone())
                .unwrap_or_else(|| "127.0.0.1".to_string());

            let new_session = user_session::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(user_id),
                token_hash: Set(session_id),
                device_info: Set(device_info),
                ip_address: Set(ip_address),
                created_at: Set(Utc::now().into()),
                expires_at: Set(expiry_chrono),
                last_active_at: Set(Some(Utc::now().into())),
                status: Set("Active".to_string()),
                data: Set(Some(data_json)),
            };
            new_session
                .insert(self.db.as_ref())
                .await
                .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        }

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let session_id_str = session_id.to_string();

        let session = user_session::Entity::find()
            .filter(user_session::Column::TokenHash.eq(&session_id_str))
            .filter(user_session::Column::Status.eq("Active"))
            .filter(user_session::Column::ExpiresAt.gt(Utc::now()))
            .one(self.db.as_ref())
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        if let Some(session) = session {
            // Deserialize session data from the database
            let data = if let Some(data_json) = session.data {
                serde_json::from_str(&data_json)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))?
            } else {
                HashMap::new()
            };

            // Convert chrono::DateTime to time::OffsetDateTime
            let expiry_time = OffsetDateTime::from_unix_timestamp(session.expires_at.timestamp())
                .map_err(|e| {
                session_store::Error::Backend(format!("Invalid timestamp: {e}"))
            })?;

            let record = Record {
                id: *session_id,
                data,
                expiry_date: expiry_time,
            };

            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let session_id_str = session_id.to_string();

        let session = user_session::Entity::find()
            .filter(user_session::Column::TokenHash.eq(&session_id_str))
            .one(self.db.as_ref())
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        if let Some(session) = session {
            let mut session_update: user_session::ActiveModel = session.into();
            session_update.status = Set("Expired".to_string());
            session_update
                .update(self.db.as_ref())
                .await
                .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        }

        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for SeaOrmSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        // Update expired sessions to have status "Expired"
        user_session::Entity::update_many()
            .filter(user_session::Column::ExpiresAt.lt(Utc::now()))
            .filter(user_session::Column::Status.eq("Active"))
            .set(user_session::ActiveModel {
                status: Set("Expired".to_string()),
                ..Default::default()
            })
            .exec(self.db.as_ref())
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
    use std::{collections::HashMap, sync::Arc};
    use time::{Duration, OffsetDateTime};
    use tower_sessions::{
        session::{Id, Record},
        session_store::{ExpiredDeletion, SessionStore},
    };
    use uuid::Uuid;

    use crate::session_store::SeaOrmSessionStore;
    use entity::user_session;

    fn create_sample_record() -> Record {
        Record {
            id: Id::default(),
            data: HashMap::new(),
            expiry_date: OffsetDateTime::now_utc() + Duration::hours(1),
        }
    }

    fn create_sample_session_model(session_id: &str) -> user_session::Model {
        user_session::Model {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            token_hash: session_id.to_string(),
            device_info: None,
            ip_address: "127.0.0.1".to_string(),
            created_at: chrono::Utc::now().into(),
            expires_at: chrono::Utc::now()
                .checked_add_signed(chrono::Duration::hours(1))
                .unwrap()
                .into(),
            last_active_at: Some(chrono::Utc::now().into()),
            status: "Active".to_string(),
            data: Some("{}".to_string()),
        }
    }

    #[tokio::test]
    async fn test_session_store_create_new() {
        let mut record = create_sample_record();
        let session_id = record.id.to_string();
        let inserted_session = create_sample_session_model(&session_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // Query results for the session store operations
            .append_query_results([
                Vec::<user_session::Model>::new(), // First query: collision check in create()
                Vec::<user_session::Model>::new(), // Second query: existing session check in save()
            ])
            // Insert operation
            .append_exec_results([MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }])
            // Query result for the inserted session (returned by insert())
            .append_query_results([
                vec![inserted_session], // The inserted session returned by insert()
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.create(&mut record).await;

        match result {
            Ok(_) => {}
            Err(e) => panic!("Session store create failed: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_session_store_create_with_collision() {
        let mut record = create_sample_record();
        let session_id = record.id.to_string();
        let existing_session = create_sample_session_model(&session_id);

        // We'll need to create a session model for the new ID after collision
        let new_session_id = "new_session_id".to_string();
        let inserted_session = create_sample_session_model(&new_session_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // First query: collision check - session exists
            .append_query_results([
                vec![existing_session], // Session exists (collision)
            ])
            // Second query: collision check with new ID - no collision
            .append_query_results([
                Vec::<user_session::Model>::new(), // No collision with new ID
            ])
            // Third query: existing session check in save() with new ID
            .append_query_results([
                Vec::<user_session::Model>::new(), // No existing session for save
            ])
            // Insert operation
            .append_exec_results([MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }])
            // Query result for the inserted session (returned by insert())
            .append_query_results([
                vec![inserted_session], // The inserted session returned by insert()
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let original_id = record.id;
        let result = store.create(&mut record).await;

        match result {
            Ok(_) => {}
            Err(e) => panic!("Session store create with collision failed: {e:?}"),
        }
        assert_ne!(record.id, original_id); // ID should have changed due to collision
    }

    #[tokio::test]
    async fn test_session_store_save_existing() {
        let record = create_sample_record();
        let session_id = record.id.to_string();
        let existing_session = create_sample_session_model(&session_id);
        let updated_session = create_sample_session_model(&session_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // Query: check if session exists
            .append_query_results([
                vec![existing_session], // Existing session found
            ])
            // Update operation
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            // Query result for the updated session (returned by update())
            .append_query_results([
                vec![updated_session], // The updated session returned by update()
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.save(&record).await;

        match result {
            Ok(_) => {}
            Err(e) => panic!("Session store save existing failed: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_session_store_load_existing() {
        let session_id = Id::default();
        let session_id_str = session_id.to_string();
        let session_model = create_sample_session_model(&session_id_str);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([
                vec![session_model], // Session found
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.load(&session_id).await;

        assert!(result.is_ok());
        let loaded_record = result.unwrap();
        assert!(loaded_record.is_some());

        let record = loaded_record.unwrap();
        assert_eq!(record.id, session_id);
    }

    #[tokio::test]
    async fn test_session_store_load_not_found() {
        let session_id = Id::default();

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([
                Vec::<user_session::Model>::new(), // No session found
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.load(&session_id).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_session_store_delete() {
        let session_id = Id::default();
        let session_id_str = session_id.to_string();
        let session_model = create_sample_session_model(&session_id_str);
        let updated_session = create_sample_session_model(&session_id_str);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // Query: find session to delete
            .append_query_results([
                vec![session_model], // Session found
            ])
            // Update operation (soft delete)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            // Query result for the updated session (returned by update())
            .append_query_results([
                vec![updated_session], // The updated session returned by update()
            ])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.delete(&session_id).await;

        match result {
            Ok(_) => {}
            Err(e) => panic!("Session store delete failed: {e:?}"),
        }
    }

    #[tokio::test]
    async fn test_session_store_delete_expired() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 3, // 3 expired sessions updated
            }])
            .into_connection();

        let store = SeaOrmSessionStore::new(Arc::new(db));
        let result = store.delete_expired().await;

        assert!(result.is_ok());
    }
}
