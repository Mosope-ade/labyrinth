//! Prepared, bounded generation requests and atomic PostgreSQL/pgmq admission.
mod prepare;
mod store;
pub use prepare::{
    AdmissionContext, PreparedAdmission, engine_revision_id, resolve_generation_input,
};
pub use store::{AdmissionError, AdmissionResult, JobState, admit};

mod reads;
pub use reads::{JobStatus, PublishedRevision, catalog_for_admission, status};
