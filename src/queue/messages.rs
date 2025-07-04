use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages sent TO external services (from Service mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutgoingMessage {
    /// Player/bot joins matchmaking queue
    QueueJoin {
        queue_id: Uuid,
        participant_type: ParticipantType,
        participant_id: Uuid,
        lobby_id: Uuid,
        preferred_mmr_range: Option<String>,
        joined_at: DateTime<Utc>,
    },
    /// Player/bot leaves matchmaking queue
    QueueLeave {
        queue_id: Uuid,
        participant_id: Uuid,
        left_at: DateTime<Utc>,
    },
}

/// Messages received FROM external services (consumed by Worker mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IncomingMessage {
    /// Matchmaking service found a match
    MatchFound {
        match_id: Uuid,
        lobby_id: Uuid,
        participants: Vec<MatchParticipant>,
        started_at: DateTime<Utc>,
    },
    /// Game server reports match completion
    MatchCompleted {
        match_id: Uuid,
        participants: Vec<MatchResult>,
        completed_at: DateTime<Utc>,
    },
    /// Queue entry cancelled by external service
    QueueCancelled {
        queue_id: Uuid,
        participant_id: Uuid,
        reason: String,
        cancelled_at: DateTime<Utc>,
    },
    /// Bot heartbeat from bot management service
    BotHeartbeat {
        bot_id: Uuid,
        heartbeat_at: DateTime<Utc>,
        status: BotStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticipantType {
    Human,
    Bot,
}

impl ToString for ParticipantType {
    fn to_string(&self) -> String {
        match self {
            ParticipantType::Human => "Human".to_string(),
            ParticipantType::Bot => "Bot".to_string(),
        }
    }
}

impl From<String> for ParticipantType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Human" => ParticipantType::Human,
            "Bot" => ParticipantType::Bot,
            _ => ParticipantType::Human, // Default fallback
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchParticipant {
    pub participant_type: ParticipantType,
    pub participant_id: Uuid,
    pub queue_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub participant_type: ParticipantType,
    pub participant_id: Uuid,
    pub final_score: i32,
    pub mmr_delta: i32,
    pub placement: i32, // 1st, 2nd, 3rd, 4th place
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BotStatus {
    Online,
    Offline,
    Error,
}

impl ToString for BotStatus {
    fn to_string(&self) -> String {
        match self {
            BotStatus::Online => "Online".to_string(),
            BotStatus::Offline => "Offline".to_string(),
            BotStatus::Error => "Error".to_string(),
        }
    }
}

/// Queue configuration for different exchanges and routing keys
pub struct QueueConfig {
    pub outgoing_exchange: String,
    pub incoming_exchange: String,
    pub outgoing_routing_key: String,
    pub incoming_routing_key: String,
    pub queue_name: String,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            outgoing_exchange: "sth.matchmaking".to_string(),
            incoming_exchange: "sth.account.updates".to_string(),
            outgoing_routing_key: "queue.events".to_string(),
            incoming_routing_key: "account.updates".to_string(),
            queue_name: "sth-account-worker".to_string(),
        }
    }
}
