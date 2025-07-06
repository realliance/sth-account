use crate::error::Result;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use std::collections::HashMap;
use tracing::info;

const MMR_K_FACTOR: f64 = 32.0; // Standard Elo K-factor
const INITIAL_MMR: i32 = 1000;

pub async fn run(db: &sea_orm::DatabaseConnection) -> Result<()> {
    info!("Running mmr-recalculation job");

    // Recalculate MMR for all users
    let updated_users = recalculate_user_mmr(db).await?;

    // Recalculate MMR for all bots
    let updated_bots = recalculate_bot_mmr(db).await?;

    info!("MMR recalculation job completed");
    info!("  Users with updated MMR: {}", updated_users);
    info!("  Bots with updated MMR: {}", updated_bots);

    Ok(())
}

async fn recalculate_user_mmr(db: &sea_orm::DatabaseConnection) -> Result<u64> {
    info!("Recalculating MMR for all users from match history");

    // Get all active users (excluding soft-deleted)
    let users = entity::user::Entity::find()
        .filter(
            entity::user::Column::AccountStatus
                .eq("Active")
                .and(entity::user::Column::DeletedAt.is_null()),
        )
        .all(db)
        .await?;

    // Initialize MMR tracking
    let mut user_mmrs: HashMap<uuid::Uuid, i32> = HashMap::new();
    for user in &users {
        user_mmrs.insert(user.id, INITIAL_MMR);
    }

    // Get all completed matches ordered by completion time
    let matches = entity::r#match::Entity::find()
        .filter(entity::r#match::Column::CompletedAt.is_not_null())
        .all(db)
        .await?;

    let mut sorted_matches = matches;
    sorted_matches.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));

    // Process each match to update MMRs
    for match_data in sorted_matches {
        let participants = get_match_participants(&match_data);
        let user_participants: Vec<_> = participants
            .iter()
            .filter(|p| user_mmrs.contains_key(&p.id))
            .collect();

        if user_participants.len() < 2 {
            continue; // Need at least 2 users to calculate MMR changes
        }

        // Calculate MMR changes based on placements
        let mmr_changes = calculate_mmr_changes(&user_participants, &user_mmrs);

        // Apply MMR changes
        for (user_id, change) in mmr_changes {
            if let Some(current_mmr) = user_mmrs.get_mut(&user_id) {
                *current_mmr = (*current_mmr + change).max(100); // Minimum MMR of 100
            }
        }
    }

    // Update user MMRs in database and track peak MMR
    let mut updated_count = 0;
    for user in users {
        if let Some(&new_mmr) = user_mmrs.get(&user.id) {
            // Update user's current MMR
            let mut user_active: entity::user::ActiveModel = user.clone().into();
            user_active.matchmaking_rank = Set(new_mmr);
            user_active.update(db).await?;

            // Update peak MMR in statistics if this is higher
            let existing_stats = entity::user_statistics::Entity::find_by_id(user.id)
                .one(db)
                .await?;

            if let Some(stats) = existing_stats {
                let current_peak = stats.peak_mmr.unwrap_or(INITIAL_MMR);
                if new_mmr > current_peak {
                    let mut stats_active: entity::user_statistics::ActiveModel = stats.into();
                    stats_active.peak_mmr = Set(Some(new_mmr));
                    stats_active.update(db).await?;
                }
            }

            updated_count += 1;
        }
    }

    Ok(updated_count)
}

async fn recalculate_bot_mmr(db: &sea_orm::DatabaseConnection) -> Result<u64> {
    info!("Recalculating MMR for all bots from match history");

    // Get all bots
    let bots = entity::bot::Entity::find().all(db).await?;

    // Initialize MMR tracking
    let mut bot_mmrs: HashMap<uuid::Uuid, i32> = HashMap::new();
    for bot in &bots {
        bot_mmrs.insert(bot.id, INITIAL_MMR);
    }

    // Get all completed matches ordered by completion time
    let matches = entity::r#match::Entity::find()
        .filter(entity::r#match::Column::CompletedAt.is_not_null())
        .all(db)
        .await?;

    let mut sorted_matches = matches;
    sorted_matches.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));

    // Process each match to update MMRs
    for match_data in sorted_matches {
        let participants = get_match_participants(&match_data);
        let bot_participants: Vec<_> = participants
            .iter()
            .filter(|p| bot_mmrs.contains_key(&p.id))
            .collect();

        if bot_participants.len() < 2 {
            continue; // Need at least 2 bots to calculate MMR changes
        }

        // Calculate MMR changes based on placements
        let mmr_changes = calculate_mmr_changes(&bot_participants, &bot_mmrs);

        // Apply MMR changes
        for (bot_id, change) in mmr_changes {
            if let Some(current_mmr) = bot_mmrs.get_mut(&bot_id) {
                *current_mmr = (*current_mmr + change).max(100); // Minimum MMR of 100
            }
        }
    }

    // Update bot MMRs in database and track peak MMR
    let mut updated_count = 0;
    for bot in bots {
        if let Some(&new_mmr) = bot_mmrs.get(&bot.id) {
            // Update bot's current MMR
            let mut bot_active: entity::bot::ActiveModel = bot.clone().into();
            bot_active.matchmaking_rank = Set(new_mmr);
            bot_active.update(db).await?;

            // Update peak MMR in statistics if this is higher
            let existing_stats = entity::bot_statistics::Entity::find_by_id(bot.id)
                .one(db)
                .await?;

            if let Some(stats) = existing_stats {
                let current_peak = stats.peak_mmr.unwrap_or(INITIAL_MMR);
                if new_mmr > current_peak {
                    let mut stats_active: entity::bot_statistics::ActiveModel = stats.into();
                    stats_active.peak_mmr = Set(Some(new_mmr));
                    stats_active.update(db).await?;
                }
            }

            updated_count += 1;
        }
    }

    Ok(updated_count)
}

#[derive(Debug, Clone)]
struct Participant {
    id: uuid::Uuid,
    score: i32,
    placement: i32,
}

fn get_match_participants(match_data: &entity::r#match::Model) -> Vec<Participant> {
    let mut participants = Vec::new();

    // Collect all participants with their scores
    let mut scores_and_ids = Vec::new();

    scores_and_ids.push((
        match_data.participant1_id,
        match_data.participant1_score.unwrap_or(0),
    ));
    scores_and_ids.push((
        match_data.participant2_id,
        match_data.participant2_score.unwrap_or(0),
    ));
    scores_and_ids.push((
        match_data.participant3_id,
        match_data.participant3_score.unwrap_or(0),
    ));
    if let Some(id) = match_data.participant4_id {
        scores_and_ids.push((id, match_data.participant4_score.unwrap_or(0)));
    }

    // Sort by score (descending) to determine placements
    scores_and_ids.sort_by(|a, b| b.1.cmp(&a.1));

    // Create participants with placements
    for (placement, (id, score)) in scores_and_ids.into_iter().enumerate() {
        participants.push(Participant {
            id,
            score,
            placement: (placement + 1) as i32,
        });
    }

    participants
}

fn calculate_mmr_changes(
    participants: &[&Participant],
    current_mmrs: &HashMap<uuid::Uuid, i32>,
) -> HashMap<uuid::Uuid, i32> {
    let mut changes = HashMap::new();

    if participants.len() < 2 {
        return changes;
    }

    // For simplicity, use a basic ELO-like system
    // Compare each participant against the average of all others
    let participant_count = participants.len() as f64;

    for participant in participants {
        let current_mmr = current_mmrs
            .get(&participant.id)
            .copied()
            .unwrap_or(INITIAL_MMR) as f64;

        // Calculate expected score based on placement
        // First place gets 1.0, last place gets 0.0, others interpolated
        let actual_score =
            (participant_count - participant.placement as f64 + 1.0) / participant_count;

        // Calculate average opponent MMR
        let opponent_mmr_sum: i32 = participants
            .iter()
            .filter(|p| p.id != participant.id)
            .map(|p| current_mmrs.get(&p.id).copied().unwrap_or(INITIAL_MMR))
            .sum();
        let avg_opponent_mmr = opponent_mmr_sum as f64 / (participant_count - 1.0);

        // Calculate expected score using ELO formula
        let expected_score = 1.0 / (1.0 + 10.0_f64.powf((avg_opponent_mmr - current_mmr) / 400.0));

        // Calculate MMR change
        let mmr_change = (MMR_K_FACTOR * (actual_score - expected_score)) as i32;

        changes.insert(participant.id, mmr_change);
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_get_match_participants() {
        let match_data = entity::r#match::Model {
            id: Uuid::new_v4(),
            lobby_id: Uuid::new_v4(),
            game_history_id: Uuid::new_v4(),
            started_at: chrono::Utc::now().into(),
            completed_at: Some(chrono::Utc::now().into()),
            participant1_type: "Human".to_string(),
            participant1_id: Uuid::new_v4(),
            participant1_score: Some(25000),
            participant1_mmr_delta: Some(10),
            participant2_type: "Human".to_string(),
            participant2_id: Uuid::new_v4(),
            participant2_score: Some(20000),
            participant2_mmr_delta: Some(5),
            participant3_type: "Bot".to_string(),
            participant3_id: Uuid::new_v4(),
            participant3_score: Some(15000),
            participant3_mmr_delta: Some(-5),
            participant4_type: Some("Bot".to_string()),
            participant4_id: Some(Uuid::new_v4()),
            participant4_score: Some(10000),
            participant4_mmr_delta: Some(-10),
        };

        let participants = get_match_participants(&match_data);

        assert_eq!(participants.len(), 4);
        assert_eq!(participants[0].placement, 1); // Highest score
        assert_eq!(participants[0].score, 25000);
        assert_eq!(participants[3].placement, 4); // Lowest score
        assert_eq!(participants[3].score, 10000);
    }

    #[test]
    fn test_calculate_mmr_changes() {
        let participant1 = Participant {
            id: Uuid::new_v4(),
            score: 25000,
            placement: 1,
        };
        let participant2 = Participant {
            id: Uuid::new_v4(),
            score: 20000,
            placement: 2,
        };

        let participants = vec![&participant1, &participant2];
        let mut current_mmrs = HashMap::new();
        current_mmrs.insert(participant1.id, 1000);
        current_mmrs.insert(participant2.id, 1200); // Higher rated player loses

        let changes = calculate_mmr_changes(&participants, &current_mmrs);

        assert!(changes.contains_key(&participant1.id));
        assert!(changes.contains_key(&participant2.id));

        // Winner should gain MMR, loser should lose MMR
        let winner_change = changes.get(&participant1.id).unwrap();
        let loser_change = changes.get(&participant2.id).unwrap();

        assert!(*winner_change > 0);
        assert!(*loser_change < 0);
    }
}
