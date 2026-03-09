//! # react-view
//!
//! View layer for the React agent framework. Provides read-only projection
//! and materialization of thread logs into display-ready structures.
//!
//! This crate enforces the architectural boundary: code that depends on
//! `react-view` is explicitly in the "view / presentation" tier. It may
//! read from core types but MUST NOT drive control flow.
//!
//! ## Hierarchy
//!
//! ```text
//! ControlState (react-core)  ← first-class source of truth
//!     ↓
//! ThreadLog (react-core)     ← immutable audit log
//!     ↓
//! ViewCache / Projection (react-view)  ← disposable display concern
//! ```

mod projection;
pub use projection::build_thread_events_from_log;

mod materialization;
pub use materialization::apply_step_to_state;

mod types;
pub use types::{
    ThreadItemError, ThreadItemKind, ThreadItemState, ThreadItemStatus, ThreadLogViewCache,
    THREAD_STATE_SCHEMA_VERSION,
};

pub use react_core::session::{
    ThreadEvent, ThreadEventKind, ThreadEventStatus, ThreadLog, ThreadStep,
};
