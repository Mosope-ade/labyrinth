//! Prepared, bounded generation requests and atomic PostgreSQL/pgmq admission.
mod prepare;
mod store;
pub use prepare::{AdmissionContext, PreparedAdmission, engine_revision_id};
pub use store::{AdmissionError, AdmissionResult, JobState, admit};
