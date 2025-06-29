use crate::error::Result;
use tracing::info;

pub async fn run_worker() -> Result<()> {
    info!("Worker mode not yet implemented");
    // TODO: Implement AMQP message processing
    Ok(())
}