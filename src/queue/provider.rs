use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::client::QueueClient;
use super::messages::OutgoingMessage;

/// Trait for queue message publishing - allows for test vs real implementations
#[async_trait]
pub trait QueueProvider: Send + Sync {
    async fn publish_message(&self, message: &OutgoingMessage) -> Result<()>;
    #[allow(dead_code)]
    async fn is_connected(&self) -> bool;
}

/// Real RabbitMQ implementation
pub struct RealQueueProvider {
    client: Arc<Mutex<Option<QueueClient>>>,
}

impl Default for RealQueueProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RealQueueProvider {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the queue client connection
    pub async fn connect(
        &self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let mut client_guard = self.client.lock().await;

        let mut client = QueueClient::new(super::QueueConfig::default());
        client.connect(host, port, username, password).await?;

        *client_guard = Some(client);
        info!("RealQueueProvider connected to RabbitMQ");
        Ok(())
    }
}

#[async_trait]
impl QueueProvider for RealQueueProvider {
    async fn publish_message(&self, message: &OutgoingMessage) -> Result<()> {
        let client_guard = self.client.lock().await;

        if let Some(client) = client_guard.as_ref() {
            client.publish_outgoing(message).await
        } else {
            warn!(
                "Queue client not connected, message not sent: {:?}",
                message
            );
            // Don't fail HTTP requests if queue is down - just log the issue
            Ok(())
        }
    }

    async fn is_connected(&self) -> bool {
        let client_guard = self.client.lock().await;
        client_guard.is_some()
    }
}

/// Test implementation that stores messages for verification
#[derive(Debug, Clone)]
pub struct TestQueueProvider {
    pub messages: Arc<Mutex<Vec<OutgoingMessage>>>,
    pub should_fail: Arc<Mutex<bool>>,
}

impl TestQueueProvider {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    /// Get all published messages (for test assertions)
    #[allow(dead_code)]
    pub async fn get_messages(&self) -> Vec<OutgoingMessage> {
        self.messages.lock().await.clone()
    }

    /// Clear stored messages
    #[allow(dead_code)]
    pub async fn clear_messages(&self) {
        self.messages.lock().await.clear();
    }

    /// Set whether publishing should fail (for error testing)
    #[allow(dead_code)]
    pub async fn set_should_fail(&self, should_fail: bool) {
        *self.should_fail.lock().await = should_fail;
    }

    /// Get the latest message of a specific type
    #[allow(dead_code)]
    pub async fn get_latest_message<F>(&self, filter: F) -> Option<OutgoingMessage>
    where
        F: Fn(&OutgoingMessage) -> bool,
    {
        let messages = self.messages.lock().await;
        messages.iter().rev().find(|msg| filter(msg)).cloned()
    }
}

#[async_trait]
impl QueueProvider for TestQueueProvider {
    async fn publish_message(&self, message: &OutgoingMessage) -> Result<()> {
        let should_fail = *self.should_fail.lock().await;

        if should_fail {
            return Err(anyhow::anyhow!("Test queue provider configured to fail"));
        }

        self.messages.lock().await.push(message.clone());
        info!("TestQueueProvider stored message: {:?}", message);
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        !*self.should_fail.lock().await
    }
}

impl Default for TestQueueProvider {
    fn default() -> Self {
        Self::new()
    }
}
