use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialSyncJob {
    pub connection_id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
}

/// Enqueue an initial-sync job for a freshly-connected POS provider.
pub fn enqueue_initial_sync(job: InitialSyncJob) {
    tracing::info!(
        connection_id = %job.connection_id,
        provider = %job.provider,
        "Enqueuing initial POS sync job"
    );
    tokio::spawn(async move {
        // TODO: integrate with apalis once the job runner is wired
        tracing::debug!(connection_id = %job.connection_id, "Initial sync stub (no-op)");
    });
}
