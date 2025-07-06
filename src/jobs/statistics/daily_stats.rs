use crate::error::Result;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running generate-daily-stats job");

    let today = chrono::Utc::now().date_naive();

    // Gather basic platform statistics (exclude soft-deleted users)
    let total_users = entity::user::Entity::find()
        .filter(
            entity::user::Column::AccountStatus
                .eq("Active")
                .and(entity::user::Column::DeletedAt.is_null()),
        )
        .count(db)
        .await?;

    let total_bots = entity::bot::Entity::find().count(db).await?;

    let live_bots = entity::bot::Entity::find()
        .filter(entity::bot::Column::Live.eq(true))
        .count(db)
        .await?;

    let active_sessions = entity::user_session::Entity::find()
        .filter(entity::user_session::Column::Status.eq("Active"))
        .count(db)
        .await?;

    // Get queue statistics
    let queue_stats = get_queue_statistics(db).await?;

    // Get match statistics for today
    let match_stats = get_daily_match_statistics(db, today).await?;

    // Get private room statistics
    let room_stats = get_room_statistics(db).await?;

    // Get report statistics
    let report_stats = get_report_statistics(db).await?;

    // Log daily statistics
    info!("Daily platform statistics for {}:", today);
    info!("  Total active users: {}", total_users);
    info!("  Total bots: {} ({} live)", total_bots, live_bots);
    info!("  Active sessions: {}", active_sessions);
    info!("  Queue statistics: {:?}", queue_stats);
    info!("  Match statistics: {:?}", match_stats);
    info!("  Room statistics: {:?}", room_stats);
    info!("  Report statistics: {:?}", report_stats);

    // In a real implementation, you would store these in a daily_statistics table
    // or send them to a monitoring system like Prometheus/Grafana

    Ok(())
}

#[derive(Debug)]
struct QueueStatistics {
    total_waiting: u64,
    waiting_humans: u64,
    waiting_bots: u64,
    average_wait_time_minutes: Option<f64>,
}

#[derive(Debug)]
struct MatchStatistics {
    total_matches_today: u64,
    completed_matches_today: u64,
    average_match_duration_minutes: Option<f64>,
    unique_players_today: u64,
}

#[derive(Debug)]
struct RoomStatistics {
    total_rooms: u64,
    open_rooms: u64,
    in_game_rooms: u64,
    average_room_occupancy: Option<f64>,
}

#[derive(Debug)]
struct ReportStatistics {
    pending_reports: u64,
    reports_today: u64,
    reports_by_severity: (u64, u64, u64, u64), // Low, Medium, High, Critical
}

async fn get_queue_statistics(db: &sea_orm::DatabaseConnection) -> Result<QueueStatistics> {
    let total_waiting = entity::queue::Entity::find()
        .filter(entity::queue::Column::Status.eq("Waiting"))
        .count(db)
        .await?;

    let waiting_humans = entity::queue::Entity::find()
        .filter(
            entity::queue::Column::Status
                .eq("Waiting")
                .and(entity::queue::Column::ParticipantType.eq("Human")),
        )
        .count(db)
        .await?;

    let waiting_bots = entity::queue::Entity::find()
        .filter(
            entity::queue::Column::Status
                .eq("Waiting")
                .and(entity::queue::Column::ParticipantType.eq("Bot")),
        )
        .count(db)
        .await?;

    // Calculate average wait time (simplified)
    let waiting_entries = entity::queue::Entity::find()
        .filter(entity::queue::Column::Status.eq("Waiting"))
        .all(db)
        .await?;

    let average_wait_time_minutes = if !waiting_entries.is_empty() {
        let now = chrono::Utc::now();
        let total_wait_minutes: i64 = waiting_entries
            .iter()
            .map(|entry| (now - entry.joined_at.with_timezone(&chrono::Utc)).num_minutes())
            .sum();
        Some(total_wait_minutes as f64 / waiting_entries.len() as f64)
    } else {
        None
    };

    Ok(QueueStatistics {
        total_waiting,
        waiting_humans,
        waiting_bots,
        average_wait_time_minutes,
    })
}

async fn get_daily_match_statistics(
    db: &sea_orm::DatabaseConnection,
    today: chrono::NaiveDate,
) -> Result<MatchStatistics> {
    let start_of_day = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = today.and_hms_opt(23, 59, 59).unwrap().and_utc();

    let total_matches_today = entity::r#match::Entity::find()
        .filter(
            entity::r#match::Column::StartedAt
                .gte(start_of_day)
                .and(entity::r#match::Column::StartedAt.lt(end_of_day)),
        )
        .count(db)
        .await?;

    let completed_matches_today = entity::r#match::Entity::find()
        .filter(
            entity::r#match::Column::CompletedAt
                .gte(start_of_day)
                .and(entity::r#match::Column::CompletedAt.lt(end_of_day)),
        )
        .count(db)
        .await?;

    // Calculate average match duration for completed matches today
    let completed_matches = entity::r#match::Entity::find()
        .filter(
            entity::r#match::Column::CompletedAt
                .gte(start_of_day)
                .and(entity::r#match::Column::CompletedAt.lt(end_of_day)),
        )
        .all(db)
        .await?;

    let average_match_duration_minutes = if !completed_matches.is_empty() {
        let total_duration_minutes: i64 = completed_matches
            .iter()
            .map(|m| (m.completed_at.unwrap_or(m.started_at) - m.started_at).num_minutes())
            .sum();
        Some(total_duration_minutes as f64 / completed_matches.len() as f64)
    } else {
        None
    };

    // Count unique players who played today
    let mut unique_players = std::collections::HashSet::new();
    for match_data in &completed_matches {
        unique_players.insert(match_data.participant1_id);
        unique_players.insert(match_data.participant2_id);
        unique_players.insert(match_data.participant3_id);
        if let Some(id) = match_data.participant4_id {
            unique_players.insert(id);
        }
    }

    Ok(MatchStatistics {
        total_matches_today,
        completed_matches_today,
        average_match_duration_minutes,
        unique_players_today: unique_players.len() as u64,
    })
}

async fn get_room_statistics(db: &sea_orm::DatabaseConnection) -> Result<RoomStatistics> {
    let total_rooms = entity::private_room::Entity::find().count(db).await?;

    let open_rooms = entity::private_room::Entity::find()
        .filter(entity::private_room::Column::Status.eq("Open"))
        .count(db)
        .await?;

    let in_game_rooms = entity::private_room::Entity::find()
        .filter(entity::private_room::Column::Status.eq("InGame"))
        .count(db)
        .await?;

    // Calculate average room occupancy
    let open_rooms_data = entity::private_room::Entity::find()
        .filter(entity::private_room::Column::Status.eq("Open"))
        .all(db)
        .await?;

    let average_room_occupancy = if !open_rooms_data.is_empty() {
        let mut total_occupancy = 0u64;

        for room in &open_rooms_data {
            let participant_count = entity::room_participants::Entity::find()
                .filter(
                    entity::room_participants::Column::RoomId
                        .eq(room.id)
                        .and(entity::room_participants::Column::LeftAt.is_null()),
                )
                .count(db)
                .await?;

            total_occupancy += participant_count;
        }

        Some(total_occupancy as f64 / open_rooms_data.len() as f64)
    } else {
        None
    };

    Ok(RoomStatistics {
        total_rooms,
        open_rooms,
        in_game_rooms,
        average_room_occupancy,
    })
}

async fn get_report_statistics(db: &sea_orm::DatabaseConnection) -> Result<ReportStatistics> {
    let pending_reports = entity::report::Entity::find()
        .filter(entity::report::Column::Status.eq("Active"))
        .count(db)
        .await?;

    let today = chrono::Utc::now().date_naive();
    let start_of_day = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = today.and_hms_opt(23, 59, 59).unwrap().and_utc();

    let reports_today = entity::report::Entity::find()
        .filter(
            entity::report::Column::CreatedAt
                .gte(start_of_day)
                .and(entity::report::Column::CreatedAt.lt(end_of_day)),
        )
        .count(db)
        .await?;

    // Count reports by severity
    let low_severity = entity::report::Entity::find()
        .filter(
            entity::report::Column::Status
                .eq("Active")
                .and(entity::report::Column::Severity.eq("Low")),
        )
        .count(db)
        .await?;

    let medium_severity = entity::report::Entity::find()
        .filter(
            entity::report::Column::Status
                .eq("Active")
                .and(entity::report::Column::Severity.eq("Medium")),
        )
        .count(db)
        .await?;

    let high_severity = entity::report::Entity::find()
        .filter(
            entity::report::Column::Status
                .eq("Active")
                .and(entity::report::Column::Severity.eq("High")),
        )
        .count(db)
        .await?;

    let critical_severity = entity::report::Entity::find()
        .filter(
            entity::report::Column::Status
                .eq("Active")
                .and(entity::report::Column::Severity.eq("Critical")),
        )
        .count(db)
        .await?;

    Ok(ReportStatistics {
        pending_reports,
        reports_today,
        reports_by_severity: (
            low_severity,
            medium_severity,
            high_severity,
            critical_severity,
        ),
    })
}
