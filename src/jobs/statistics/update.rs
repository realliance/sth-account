use crate::error::Result;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use std::collections::HashMap;
use tracing::info;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running update-statistics job");

    // Update user statistics
    let updated_users = update_user_statistics(db).await?;

    // Update bot statistics
    let updated_bots = update_bot_statistics(db).await?;

    info!(
        "Statistics update complete - updated {} users and {} bots",
        updated_users, updated_bots
    );
    Ok(())
}

async fn update_user_statistics(db: &sea_orm::DatabaseConnection) -> Result<u64> {
    info!("Updating user statistics from match data");

    // Get all active users (excluding soft-deleted)
    let users = entity::user::Entity::find()
        .filter(
            entity::user::Column::AccountStatus
                .eq("Active")
                .and(entity::user::Column::DeletedAt.is_null()),
        )
        .all(db)
        .await?;

    let mut updated_count = 0;

    for user in users {
        // Calculate statistics from matches
        let stats = calculate_user_stats_from_matches(db, user.id).await?;

        // Find or create user statistics record
        let existing_stats = entity::user_statistics::Entity::find_by_id(user.id)
            .one(db)
            .await?;

        if let Some(existing) = existing_stats {
            // Update existing statistics
            let existing_peak = existing.peak_mmr;
            let mut active_model: entity::user_statistics::ActiveModel = existing.into();
            active_model.total_games = Set(stats.total_games);
            active_model.wins = Set(stats.wins);
            active_model.second_place = Set(stats.second_place);
            active_model.third_place = Set(stats.third_place);
            active_model.fourth_place = Set(stats.fourth_place);
            active_model.average_score = Set(stats.average_score);
            active_model.peak_mmr = Set(stats
                .peak_mmr
                .or(existing_peak)
                .or(Some(user.matchmaking_rank)));
            active_model.current_streak = Set(stats.current_streak);
            active_model.last_game_at = Set(stats.last_game_at);

            active_model.update(db).await?;
        } else {
            // Create new statistics record
            let new_stats = entity::user_statistics::ActiveModel {
                user_id: Set(user.id),
                total_games: Set(stats.total_games),
                wins: Set(stats.wins),
                second_place: Set(stats.second_place),
                third_place: Set(stats.third_place),
                fourth_place: Set(stats.fourth_place),
                average_score: Set(stats.average_score),
                peak_mmr: Set(stats.peak_mmr.or(Some(user.matchmaking_rank))),
                current_streak: Set(stats.current_streak),
                last_game_at: Set(stats.last_game_at),
            };

            new_stats.insert(db).await?;
        }

        updated_count += 1;
    }

    Ok(updated_count)
}

async fn update_bot_statistics(db: &sea_orm::DatabaseConnection) -> Result<u64> {
    info!("Updating bot statistics from match data");

    // Get all bots
    let bots = entity::bot::Entity::find().all(db).await?;

    let mut updated_count = 0;

    for bot in bots {
        // Calculate statistics from matches
        let stats = calculate_bot_stats_from_matches(db, bot.id).await?;

        // Find or create bot statistics record
        let existing_stats = entity::bot_statistics::Entity::find_by_id(bot.id)
            .one(db)
            .await?;

        if let Some(existing) = existing_stats {
            // Update existing statistics
            let existing_peak = existing.peak_mmr;
            let mut active_model: entity::bot_statistics::ActiveModel = existing.into();
            active_model.total_games = Set(stats.total_games);
            active_model.wins = Set(stats.wins);
            active_model.second_place = Set(stats.second_place);
            active_model.third_place = Set(stats.third_place);
            active_model.fourth_place = Set(stats.fourth_place);
            active_model.average_score = Set(stats.average_score);
            active_model.peak_mmr = Set(stats
                .peak_mmr
                .or(existing_peak)
                .or(Some(bot.matchmaking_rank)));
            active_model.current_streak = Set(stats.current_streak);
            active_model.last_game_at = Set(stats.last_game_at);
            active_model.uptime_percentage = Set(stats.uptime_percentage);

            active_model.update(db).await?;
        } else {
            // Create new statistics record
            let new_stats = entity::bot_statistics::ActiveModel {
                bot_id: Set(bot.id),
                total_games: Set(stats.total_games),
                wins: Set(stats.wins),
                second_place: Set(stats.second_place),
                third_place: Set(stats.third_place),
                fourth_place: Set(stats.fourth_place),
                average_score: Set(stats.average_score),
                peak_mmr: Set(stats.peak_mmr.or(Some(bot.matchmaking_rank))),
                current_streak: Set(stats.current_streak),
                last_game_at: Set(stats.last_game_at),
                uptime_percentage: Set(stats.uptime_percentage),
            };

            new_stats.insert(db).await?;
        }

        updated_count += 1;
    }

    Ok(updated_count)
}

#[derive(Debug, Default)]
struct UserStats {
    total_games: i32,
    wins: i32,
    second_place: i32,
    third_place: i32,
    fourth_place: i32,
    average_score: Option<Decimal>,
    peak_mmr: Option<i32>,
    current_streak: i32,
    last_game_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Default)]
struct BotStats {
    total_games: i32,
    wins: i32,
    second_place: i32,
    third_place: i32,
    fourth_place: i32,
    average_score: Option<Decimal>,
    peak_mmr: Option<i32>,
    current_streak: i32,
    last_game_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    uptime_percentage: Option<Decimal>,
}

async fn calculate_user_stats_from_matches(
    db: &sea_orm::DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<UserStats> {
    // Get all matches where this user participated
    let matches = entity::r#match::Entity::find()
        .filter(
            entity::r#match::Column::Participant1Id
                .eq(user_id)
                .or(entity::r#match::Column::Participant2Id.eq(user_id))
                .or(entity::r#match::Column::Participant3Id.eq(user_id))
                .or(entity::r#match::Column::Participant4Id.eq(user_id)),
        )
        .all(db)
        .await?;

    let mut stats = UserStats::default();
    stats.total_games = matches.len() as i32;

    if matches.is_empty() {
        return Ok(stats);
    }

    let mut scores = Vec::new();
    let mut placement_counts = HashMap::new();
    let mut streak_count = 0;
    let mut last_was_win = false;

    // Sort matches by completion time to calculate streak
    let mut sorted_matches = matches.clone();
    sorted_matches.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));

    for match_data in &sorted_matches {
        // Find which participant position this user was in and get their score
        let (score, placement) = if match_data.participant1_id == user_id {
            (
                match_data.participant1_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant1_score.unwrap_or(0)),
            )
        } else if match_data.participant2_id == user_id {
            (
                match_data.participant2_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant2_score.unwrap_or(0)),
            )
        } else if match_data.participant3_id == user_id {
            (
                match_data.participant3_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant3_score.unwrap_or(0)),
            )
        } else if match_data.participant4_id == Some(user_id) {
            (
                match_data.participant4_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant4_score.unwrap_or(0)),
            )
        } else {
            continue;
        };

        scores.push(score);
        *placement_counts.entry(placement).or_insert(0) += 1;

        // Update streak
        let is_win = placement == 1;
        if is_win {
            if last_was_win {
                streak_count += 1;
            } else {
                streak_count = 1;
                last_was_win = true;
            }
        } else if !last_was_win {
            streak_count -= 1;
        } else {
            streak_count = -1;
            last_was_win = false;
        }

        if let Some(completed_at) = match_data.completed_at {
            stats.last_game_at = Some(completed_at);
        }
    }

    stats.wins = placement_counts.get(&1).copied().unwrap_or(0);
    stats.second_place = placement_counts.get(&2).copied().unwrap_or(0);
    stats.third_place = placement_counts.get(&3).copied().unwrap_or(0);
    stats.fourth_place = placement_counts.get(&4).copied().unwrap_or(0);
    stats.current_streak = streak_count;

    // Calculate average score
    if !scores.is_empty() {
        let sum: i32 = scores.iter().sum();
        stats.average_score =
            Some(Decimal::new(sum as i64, 0) / Decimal::new(scores.len() as i64, 0));
    }

    Ok(stats)
}

async fn calculate_bot_stats_from_matches(
    db: &sea_orm::DatabaseConnection,
    bot_id: uuid::Uuid,
) -> Result<BotStats> {
    // Similar to user stats but for bots
    let matches = entity::r#match::Entity::find()
        .filter(
            entity::r#match::Column::Participant1Id
                .eq(bot_id)
                .or(entity::r#match::Column::Participant2Id.eq(bot_id))
                .or(entity::r#match::Column::Participant3Id.eq(bot_id))
                .or(entity::r#match::Column::Participant4Id.eq(bot_id)),
        )
        .all(db)
        .await?;

    let mut stats = BotStats::default();
    stats.total_games = matches.len() as i32;

    if matches.is_empty() {
        return Ok(stats);
    }

    let mut scores = Vec::new();
    let mut placement_counts = HashMap::new();
    let mut streak_count = 0;
    let mut last_was_win = false;

    let mut sorted_matches = matches.clone();
    sorted_matches.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));

    for match_data in &sorted_matches {
        let (score, placement) = if match_data.participant1_id == bot_id {
            (
                match_data.participant1_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant1_score.unwrap_or(0)),
            )
        } else if match_data.participant2_id == bot_id {
            (
                match_data.participant2_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant2_score.unwrap_or(0)),
            )
        } else if match_data.participant3_id == bot_id {
            (
                match_data.participant3_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant3_score.unwrap_or(0)),
            )
        } else if match_data.participant4_id == Some(bot_id) {
            (
                match_data.participant4_score.unwrap_or(0),
                get_placement_from_scores(match_data, match_data.participant4_score.unwrap_or(0)),
            )
        } else {
            continue;
        };

        scores.push(score);
        *placement_counts.entry(placement).or_insert(0) += 1;

        let is_win = placement == 1;
        if is_win {
            if last_was_win {
                streak_count += 1;
            } else {
                streak_count = 1;
                last_was_win = true;
            }
        } else if !last_was_win {
            streak_count -= 1;
        } else {
            streak_count = -1;
            last_was_win = false;
        }

        if let Some(completed_at) = match_data.completed_at {
            stats.last_game_at = Some(completed_at);
        }
    }

    stats.wins = placement_counts.get(&1).copied().unwrap_or(0);
    stats.second_place = placement_counts.get(&2).copied().unwrap_or(0);
    stats.third_place = placement_counts.get(&3).copied().unwrap_or(0);
    stats.fourth_place = placement_counts.get(&4).copied().unwrap_or(0);
    stats.current_streak = streak_count;

    if !scores.is_empty() {
        let sum: i32 = scores.iter().sum();
        stats.average_score =
            Some(Decimal::new(sum as i64, 0) / Decimal::new(scores.len() as i64, 0));
    }

    // Calculate uptime percentage (simplified - assumes 95% uptime for active bots)
    stats.uptime_percentage = Some(Decimal::new(95, 0));

    Ok(stats)
}

fn get_placement_from_scores(match_data: &entity::r#match::Model, target_score: i32) -> i32 {
    let mut scores = vec![
        match_data.participant1_score.unwrap_or(0),
        match_data.participant2_score.unwrap_or(0),
        match_data.participant3_score.unwrap_or(0),
    ];

    if let Some(p4_score) = match_data.participant4_score {
        scores.push(p4_score);
    }

    scores.sort_by(|a, b| b.cmp(a)); // Sort descending (highest score first)

    // Find the placement (1-indexed)
    for (index, score) in scores.iter().enumerate() {
        if *score == target_score {
            return (index + 1) as i32;
        }
    }

    // Fallback if not found
    4
}
