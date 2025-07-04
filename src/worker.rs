use crate::{
    config::Config,
    database::establish_connection,
    error::Result,
    queue::{QueueClient, QueueConfig, messages::*},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::{env, sync::Arc};
use tracing::{error, info, warn};
use uuid::Uuid;

use entity::{bot, game_history, r#match, queue, user};

pub async fn run_worker() -> Result<()> {
    info!("Starting worker mode - consuming RabbitMQ messages");

    // Get RabbitMQ connection details from environment
    let rabbitmq_host = env::var("RABBITMQ_HOST").unwrap_or_else(|_| "localhost".to_string());
    let rabbitmq_port = env::var("RABBITMQ_PORT")
        .unwrap_or_else(|_| "5672".to_string())
        .parse::<u16>()
        .unwrap_or(5672);
    let rabbitmq_user = env::var("RABBITMQ_USER").unwrap_or_else(|_| "guest".to_string());
    let rabbitmq_password = env::var("RABBITMQ_PASSWORD").unwrap_or_else(|_| "guest".to_string());

    // Establish database connection
    let config = Config::from_env()?;
    let db = Arc::new(establish_connection(&config).await?);

    // Create queue client
    let config = QueueConfig::default();
    let mut queue_client = QueueClient::new(config);

    // Connect to RabbitMQ
    queue_client
        .connect(
            &rabbitmq_host,
            rabbitmq_port,
            &rabbitmq_user,
            &rabbitmq_password,
        )
        .await?;

    info!("Connected to RabbitMQ, starting message consumer");

    // Create message handler
    let handler = {
        let db_clone = db.clone();
        move |message: IncomingMessage| -> anyhow::Result<()> {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async { handle_incoming_message(db_clone.clone(), message).await })
        }
    };

    // Setup consumer and start processing messages
    queue_client.setup_consumer(handler).await?;

    info!("Worker is now consuming messages. Press Ctrl+C to stop.");

    // Keep the worker running
    tokio::signal::ctrl_c().await?;

    info!("Shutting down worker");
    queue_client.close().await?;

    Ok(())
}

async fn handle_incoming_message(
    db: Arc<DatabaseConnection>,
    message: IncomingMessage,
) -> anyhow::Result<()> {
    match message {
        IncomingMessage::MatchFound {
            match_id,
            lobby_id,
            participants,
            started_at,
        } => handle_match_found(db, match_id, lobby_id, participants, started_at).await,
        IncomingMessage::MatchCompleted {
            match_id,
            participants,
            completed_at,
        } => handle_match_completed(db, match_id, participants, completed_at).await,
        IncomingMessage::QueueCancelled {
            queue_id,
            participant_id,
            reason,
            cancelled_at,
        } => handle_queue_cancelled(db, queue_id, participant_id, reason, cancelled_at).await,
        IncomingMessage::BotHeartbeat {
            bot_id,
            heartbeat_at,
            status,
        } => handle_bot_heartbeat(db, bot_id, heartbeat_at, status).await,
    }
}

async fn handle_match_found(
    db: Arc<DatabaseConnection>,
    match_id: Uuid,
    lobby_id: Uuid,
    participants: Vec<MatchParticipant>,
    started_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    info!("Processing MatchFound message for match {}", match_id);

    // Update queue entries to "InMatch" status
    for participant in &participants {
        if let Some(queue_entry) = queue::Entity::find_by_id(participant.queue_id)
            .one(db.as_ref())
            .await?
        {
            let mut queue_update: queue::ActiveModel = queue_entry.into();
            queue_update.status = Set("InMatch".to_string());
            queue_update.update(db.as_ref()).await?;
        }
    }

    // Create a placeholder game history entry
    let game_history_id = Uuid::new_v4();
    let game_history = game_history::ActiveModel {
        id: Set(game_history_id),
        history_blob: Set(vec![]), // Empty blob for now
    };
    game_history.insert(db.as_ref()).await?;

    // Create match record
    if participants.len() >= 3 {
        let mut new_match = r#match::ActiveModel {
            id: Set(match_id),
            lobby_id: Set(lobby_id),
            game_history_id: Set(game_history_id),
            started_at: Set(started_at.into()),
            completed_at: Set(None),
            participant1_type: Set(participants[0].participant_type.to_string()),
            participant1_id: Set(participants[0].participant_id),
            participant1_score: Set(None),
            participant1_mmr_delta: Set(None),
            participant2_type: Set(participants[1].participant_type.to_string()),
            participant2_id: Set(participants[1].participant_id),
            participant2_score: Set(None),
            participant2_mmr_delta: Set(None),
            participant3_type: Set(participants[2].participant_type.to_string()),
            participant3_id: Set(participants[2].participant_id),
            participant3_score: Set(None),
            participant3_mmr_delta: Set(None),
            participant4_type: Set(None),
            participant4_id: Set(None),
            participant4_score: Set(None),
            participant4_mmr_delta: Set(None),
        };

        // Add 4th participant if present
        if participants.len() >= 4 {
            new_match.participant4_type = Set(Some(participants[3].participant_type.to_string()));
            new_match.participant4_id = Set(Some(participants[3].participant_id));
        }

        new_match.insert(db.as_ref()).await?;
        info!("Created match record for match {}", match_id);
    } else {
        error!("Not enough participants for match {}", match_id);
    }

    Ok(())
}

async fn handle_match_completed(
    db: Arc<DatabaseConnection>,
    match_id: Uuid,
    participants: Vec<MatchResult>,
    completed_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    info!("Processing MatchCompleted message for match {}", match_id);

    // Find the match record
    let match_record = r#match::Entity::find_by_id(match_id)
        .one(db.as_ref())
        .await?;

    if let Some(match_model) = match_record {
        let mut match_update: r#match::ActiveModel = match_model.into();
        match_update.completed_at = Set(Some(completed_at.into()));

        // Update participant scores and MMR deltas
        for result in &participants {
            if result.participant_id == *match_update.participant1_id.as_ref() {
                match_update.participant1_score = Set(Some(result.final_score));
                match_update.participant1_mmr_delta = Set(Some(result.mmr_delta));
            } else if result.participant_id == *match_update.participant2_id.as_ref() {
                match_update.participant2_score = Set(Some(result.final_score));
                match_update.participant2_mmr_delta = Set(Some(result.mmr_delta));
            } else if result.participant_id == *match_update.participant3_id.as_ref() {
                match_update.participant3_score = Set(Some(result.final_score));
                match_update.participant3_mmr_delta = Set(Some(result.mmr_delta));
            } else if match_update.participant4_id.as_ref().as_ref() == Some(&result.participant_id)
            {
                match_update.participant4_score = Set(Some(result.final_score));
                match_update.participant4_mmr_delta = Set(Some(result.mmr_delta));
            }
        }

        match_update.update(db.as_ref()).await?;

        // Update MMR for participants
        for result in &participants {
            match result.participant_type {
                ParticipantType::Human => {
                    if let Some(user) = user::Entity::find_by_id(result.participant_id)
                        .one(db.as_ref())
                        .await?
                    {
                        let mut user_update: user::ActiveModel = user.into();
                        let new_mmr = user_update.matchmaking_rank.as_ref() + result.mmr_delta;
                        user_update.matchmaking_rank = Set(new_mmr.max(0)); // Don't go below 0
                        user_update.update(db.as_ref()).await?;
                    }
                }
                ParticipantType::Bot => {
                    if let Some(bot) = bot::Entity::find_by_id(result.participant_id)
                        .one(db.as_ref())
                        .await?
                    {
                        let mut bot_update: bot::ActiveModel = bot.into();
                        let new_mmr = bot_update.matchmaking_rank.as_ref() + result.mmr_delta;
                        bot_update.matchmaking_rank = Set(new_mmr.max(0)); // Don't go below 0
                        bot_update.update(db.as_ref()).await?;
                    }
                }
            }
        }

        info!("Updated match completion for match {}", match_id);
    } else {
        warn!("Match {} not found when trying to complete", match_id);
    }

    Ok(())
}

async fn handle_queue_cancelled(
    db: Arc<DatabaseConnection>,
    queue_id: Uuid,
    _participant_id: Uuid,
    reason: String,
    _cancelled_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    info!(
        "Processing QueueCancelled message for queue {}: {}",
        queue_id, reason
    );

    // Update queue entry status
    if let Some(queue_entry) = queue::Entity::find_by_id(queue_id).one(db.as_ref()).await? {
        let mut queue_update: queue::ActiveModel = queue_entry.into();
        queue_update.status = Set("Cancelled".to_string());
        queue_update.update(db.as_ref()).await?;
        info!("Updated queue {} status to Cancelled", queue_id);
    } else {
        warn!("Queue {} not found when trying to cancel", queue_id);
    }

    Ok(())
}

async fn handle_bot_heartbeat(
    db: Arc<DatabaseConnection>,
    bot_id: Uuid,
    heartbeat_at: chrono::DateTime<chrono::Utc>,
    status: BotStatus,
) -> anyhow::Result<()> {
    info!("Processing BotHeartbeat for bot {}: {:?}", bot_id, status);

    // Update bot's last heartbeat and live status
    if let Some(bot) = bot::Entity::find_by_id(bot_id).one(db.as_ref()).await? {
        let mut bot_update: bot::ActiveModel = bot.into();
        bot_update.last_heartbeat = Set(Some(heartbeat_at.into()));

        // Update live status based on heartbeat status
        let is_live = matches!(status, BotStatus::Online);
        bot_update.live = Set(is_live);

        bot_update.update(db.as_ref()).await?;
        info!("Updated bot {} heartbeat and status", bot_id);
    } else {
        warn!("Bot {} not found when processing heartbeat", bot_id);
    }

    Ok(())
}
