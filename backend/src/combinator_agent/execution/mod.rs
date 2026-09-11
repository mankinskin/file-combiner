//! Combine backends: the production Copilot-agent-driven backend and the
//! deterministic fake backend used by integration tests.
mod fake_backend;
mod prompts;
mod rig_backend;

pub use fake_backend::{FakeCombineBackend, MockCombineBackend};
pub use rig_backend::RigCombineBackend;
