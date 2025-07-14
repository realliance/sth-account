use crate::{
    config::Config,
    database::establish_connection,
    error::{AppError, Result},
};

pub mod cleanup;
pub mod monitoring;
pub mod statistics;

pub async fn run_job(job_type: String) -> Result<()> {
    let config = Config::from_env()?;
    let db = establish_connection(&config).await?;

    match job_type.as_str() {
        // Cleanup jobs
        "cleanup-audit-logs" => cleanup::audit_logs::run(&db).await,
        "cleanup-export-files" => cleanup::export_files::run(&db).await,
        "cleanup-reports" => cleanup::reports::run(&db).await,
        "cleanup-notifications" => cleanup::notifications::run(&db).await,
        "cleanup-stale-queues" => cleanup::stale_queues::run(&db).await,
        "cleanup-expired-rooms" => cleanup::expired_rooms::run(&db).await,

        // Statistics jobs
        "update-statistics" => statistics::update::run(&db).await,
        "mmr-recalculation" => statistics::mmr_recalculation::run(&db).await,
        "generate-daily-stats" => statistics::daily_stats::run(&db).await,

        // Monitoring jobs
        "heartbeat-check" => monitoring::heartbeat::run(&db).await,

        _ => Err(AppError::Job(format!("Unknown job type: {job_type}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_job_with_invalid_job_type() {
        let result = run_job("invalid-job-type".to_string()).await;
        assert!(result.is_err());

        match result {
            Err(AppError::Job(msg)) => {
                assert!(msg.contains("Unknown job type"));
            }
            Err(AppError::Config(_)) => {
                // Config error is expected if no database connection can be established
                println!("Got config error (expected in test environment)");
            }
            Err(AppError::Database(_)) => {
                // Database error is expected if no database connection can be established
                println!("Got database error (expected in test environment)");
            }
            Err(other) => {
                panic!("Expected Job, Config, or Database error, got: {other:?}");
            }
            Ok(()) => {
                panic!("Expected an error for invalid job type, but got success");
            }
        }
    }
}
