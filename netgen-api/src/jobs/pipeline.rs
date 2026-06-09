use std::sync::Arc;

use uuid::Uuid;

use super::{JobQueue, ProgressEmitter};

/// Steps performed when generating a full network from an intent.
pub const GENERATION_STEPS: &[&str] = &[
    "rule_engine",
    "security_policy",
    "addressing",
    "config_templates",
    "verification",
    "gns3_export",
    "svg_diagram",
    "ip_plan_csv",
    "documentation",
];

/// Emits progress events as each pipeline stage completes.
///
/// This is called from a spawned tokio task so it runs off the request thread.
/// The caller is responsible for actually running the pipeline; this only
/// streams progress. On success call `emitter.complete()`, on error call
/// `emitter.fail(msg)`.
pub fn generation_emitter(
    project_id: Uuid,
    job_id: Uuid,
    queue: Arc<JobQueue>,
) -> ProgressEmitter {
    let steps = GENERATION_STEPS.iter().map(|s| s.to_string()).collect();
    ProgressEmitter::new(project_id, job_id, queue, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_steps_are_nonempty() {
        assert!(!GENERATION_STEPS.is_empty());
    }

    #[test]
    fn all_steps_are_snake_case_nonempty() {
        for step in GENERATION_STEPS {
            assert!(!step.is_empty());
            assert!(step.chars().all(|c| c.is_alphanumeric() || c == '_'));
        }
    }

    #[tokio::test]
    async fn generation_emitter_produces_correct_step_count() {
        let queue = Arc::new(JobQueue::new());
        let project_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        let mut rx = queue.subscribe(project_id).await;
        let mut emitter = generation_emitter(project_id, job_id, queue);

        for _ in GENERATION_STEPS {
            emitter.advance().await;
        }
        emitter.complete().await;

        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, GENERATION_STEPS.len() + 1); // steps + Complete
    }
}
