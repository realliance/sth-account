use amqprs::{
    BasicProperties, Deliver,
    callbacks::{DefaultChannelCallback, DefaultConnectionCallback},
    channel::{
        BasicAckArguments, BasicConsumeArguments, BasicNackArguments, BasicPublishArguments,
        Channel, QueueBindArguments, QueueDeclareArguments,
    },
    connection::{Connection, OpenConnectionArguments},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use super::messages::{IncomingMessage, OutgoingMessage, QueueConfig};

pub struct QueueClient {
    connection: Option<Connection>,
    channel: Option<Channel>,
    config: QueueConfig,
}

impl QueueClient {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            connection: None,
            channel: None,
            config,
        }
    }

    /// Connect to RabbitMQ server
    pub async fn connect(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<()> {
        info!("Connecting to RabbitMQ at {}:{}", host, port);

        let connection = Connection::open(&OpenConnectionArguments::new(
            host, port, username, password,
        ))
        .await?;

        connection
            .register_callback(DefaultConnectionCallback)
            .await?;

        let channel = connection.open_channel(None).await?;
        channel.register_callback(DefaultChannelCallback).await?;

        self.connection = Some(connection);
        self.channel = Some(channel);

        info!("Successfully connected to RabbitMQ");
        Ok(())
    }

    /// Publish an outgoing message to the matchmaking exchange
    pub async fn publish_outgoing(&self, message: &OutgoingMessage) -> Result<()> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to RabbitMQ"))?;

        let content = serde_json::to_vec(message)?;

        let args = BasicPublishArguments::new(
            &self.config.outgoing_exchange,
            &self.config.outgoing_routing_key,
        );

        channel
            .basic_publish(BasicProperties::default(), content, args)
            .await?;

        info!("Published outgoing message: {:?}", message);
        Ok(())
    }

    /// Set up consumer for incoming messages (for Worker mode)
    pub async fn setup_consumer<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(IncomingMessage) -> Result<()> + Send + 'static,
    {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to RabbitMQ"))?;

        // Declare the queue for incoming messages
        let queue_args = QueueDeclareArguments::new(&self.config.queue_name);
        let (queue_name, _, _) = channel.queue_declare(queue_args).await?.unwrap();

        // Bind queue to the incoming exchange
        channel
            .queue_bind(QueueBindArguments::new(
                &queue_name,
                &self.config.incoming_exchange,
                &self.config.incoming_routing_key,
            ))
            .await?;

        info!(
            "Queue '{}' declared and bound to exchange '{}'",
            queue_name, self.config.incoming_exchange
        );

        // Start consuming messages
        let consumer = WorkerConsumer::new(handler);
        let args = BasicConsumeArguments::new(&queue_name, "sth-account-worker");

        channel.basic_consume(consumer, args).await?;

        info!("Started consuming messages from queue '{}'", queue_name);
        Ok(())
    }

    /// Close the connection
    pub async fn close(&mut self) -> Result<()> {
        if let Some(channel) = self.channel.take() {
            channel.close().await?;
        }
        if let Some(connection) = self.connection.take() {
            connection.close().await?;
        }
        info!("Closed RabbitMQ connection");
        Ok(())
    }
}

/// Consumer implementation for worker mode
struct WorkerConsumer<F>
where
    F: FnMut(IncomingMessage) -> Result<()> + Send + 'static,
{
    handler: Arc<Mutex<F>>,
}

impl<F> WorkerConsumer<F>
where
    F: FnMut(IncomingMessage) -> Result<()> + Send + 'static,
{
    fn new(handler: F) -> Self {
        Self {
            handler: Arc::new(Mutex::new(handler)),
        }
    }
}

#[async_trait::async_trait]
impl<F> amqprs::consumer::AsyncConsumer for WorkerConsumer<F>
where
    F: FnMut(IncomingMessage) -> Result<()> + Send + 'static,
{
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        match serde_json::from_slice::<IncomingMessage>(&content) {
            Ok(message) => {
                let mut handler = self.handler.lock().await;
                match handler(message) {
                    Ok(()) => {
                        // Acknowledge message on successful processing
                        if let Err(e) = channel
                            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
                            .await
                        {
                            error!("Failed to acknowledge message: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to process message: {}", e);
                        // Reject and requeue message on processing failure
                        if let Err(e) = channel
                            .basic_nack(BasicNackArguments::new(
                                deliver.delivery_tag(),
                                false,
                                true, // requeue
                            ))
                            .await
                        {
                            error!("Failed to nack message: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to deserialize message: {}", e);
                // Reject malformed messages without requeuing
                if let Err(e) = channel
                    .basic_nack(BasicNackArguments::new(
                        deliver.delivery_tag(),
                        false,
                        false, // don't requeue
                    ))
                    .await
                {
                    error!("Failed to nack malformed message: {}", e);
                }
            }
        }
    }
}
