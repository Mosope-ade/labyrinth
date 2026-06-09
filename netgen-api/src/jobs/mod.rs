pub mod pipeline;

use std::{collections::HashMap, sync::Arc};

use tokio::sync::broadcast;
use uuid::Uuid;

// ─── Progress events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Started { job_id: Uuid },
    Progress { job_id: Uuid, step: String, percent: u8 },
    Complete { job_id: Uuid },
    Failed { job_id: Uuid, error: String },
}

// ─── JobQueue ─────────────────────────────────────────────────────────────────

/// Manages per-project broadcast channels for WebSocket progress streaming.
///
/// Producers call `emit()`. WebSocket handlers call `subscribe()`.
pub struct JobQueue {
    /// Maps project_id → broadcast sender. Lazily created per project.
    channels: tokio::sync::Mutex<HashMap<Uuid, broadcast::Sender<JobEvent>>>,
    channel_capacity: usize,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            channels: tokio::sync::Mutex::new(HashMap::new()),
            channel_capacity: 64,
        }
    }

    /// Subscribe to events for a project. Creates the channel if it doesn't exist.
    pub async fn subscribe(&self, project_id: Uuid) -> broadcast::Receiver<JobEvent> {
        let mut map = self.channels.lock().await;
        map.entry(project_id)
            .or_insert_with(|| broadcast::channel(self.channel_capacity).0)
            .subscribe()
    }

    /// Emit an event to all subscribers of a project.
    /// If no subscribers exist the event is silently dropped.
    pub async fn emit(&self, project_id: Uuid, event: JobEvent) {
        let map = self.channels.lock().await;
        if let Some(tx) = map.get(&project_id) {
            let _ = tx.send(event); // ignore SendError — no subscribers is fine
        }
    }

    /// Clean up the channel for a project once all jobs are done.
    pub async fn remove(&self, project_id: Uuid) {
        let mut map = self.channels.lock().await;
        map.remove(&project_id);
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Convenience wrappers ────────────────────────────────────────────────────

/// Emits a progress sequence for a multi-step pipeline task.
/// Callers provide step names and call the returned handle after each step.
pub struct ProgressEmitter {
    project_id: Uuid,
    job_id: Uuid,
    queue: Arc<JobQueue>,
    steps: Vec<String>,
    current: usize,
}

impl ProgressEmitter {
    pub fn new(
        project_id: Uuid,
        job_id: Uuid,
        queue: Arc<JobQueue>,
        steps: Vec<String>,
    ) -> Self {
        Self {
            project_id,
            job_id,
            queue,
            steps,
            current: 0,
        }
    }

    /// Emit progress for the next step. Steps are consumed in order.
    pub async fn advance(&mut self) {
        if self.current >= self.steps.len() {
            return;
        }
        let step = self.steps[self.current].clone();
        let percent = ((self.current + 1) as f32 / self.steps.len() as f32 * 100.0) as u8;
        self.queue
            .emit(
                self.project_id,
                JobEvent::Progress {
                    job_id: self.job_id,
                    step,
                    percent,
                },
            )
            .await;
        self.current += 1;
    }

    pub async fn complete(self) {
        self.queue
            .emit(self.project_id, JobEvent::Complete { job_id: self.job_id })
            .await;
    }

    pub async fn fail(self, error: String) {
        self.queue
            .emit(
                self.project_id,
                JobEvent::Failed {
                    job_id: self.job_id,
                    error,
                },
            )
            .await;
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribe_receives_emitted_events() {
        let queue = JobQueue::new();
        let project_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        let mut rx = queue.subscribe(project_id).await;

        queue
            .emit(project_id, JobEvent::Started { job_id })
            .await;

        let event = rx.try_recv().expect("expected event");
        assert!(matches!(event, JobEvent::Started { .. }));
    }

    #[tokio::test]
    async fn emit_without_subscriber_does_not_panic() {
        let queue = JobQueue::new();
        let project_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        // No subscriber — should silently drop
        queue.emit(project_id, JobEvent::Complete { job_id }).await;
    }

    #[tokio::test]
    async fn progress_emitter_advances_steps() {
        let queue = Arc::new(JobQueue::new());
        let project_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        let mut rx = queue.subscribe(project_id).await;

        let mut emitter = ProgressEmitter::new(
            project_id,
            job_id,
            queue,
            vec!["graph".into(), "security".into(), "export".into()],
        );

        emitter.advance().await;
        emitter.advance().await;
        emitter.advance().await;
        emitter.complete().await;

        // Drain all events
        let mut events = vec![];
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert_eq!(events.len(), 4); // 3 progress + 1 complete

        // Last event is Complete
        assert!(matches!(events.last(), Some(JobEvent::Complete { .. })));
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive_events() {
        let queue = JobQueue::new();
        let project_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        let mut rx1 = queue.subscribe(project_id).await;
        let mut rx2 = queue.subscribe(project_id).await;

        queue
            .emit(project_id, JobEvent::Complete { job_id })
            .await;

        assert!(matches!(rx1.try_recv(), Ok(JobEvent::Complete { .. })));
        assert!(matches!(rx2.try_recv(), Ok(JobEvent::Complete { .. })));
    }

    #[tokio::test]
    async fn remove_cleans_up_channel() {
        let queue = JobQueue::new();
        let project_id = Uuid::new_v4();
        let _ = queue.subscribe(project_id).await;
        queue.remove(project_id).await;
        assert!(queue.channels.lock().await.get(&project_id).is_none());
    }
}
