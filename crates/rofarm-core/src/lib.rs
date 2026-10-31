#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod account;
pub mod automation;
pub mod client;
pub mod diagnostics;
pub mod error;
pub mod events;
pub mod fleet;
pub mod ids;
pub mod import;
pub mod lane;
pub mod launch;
pub mod library;
pub mod metrics;
pub mod notice;
pub mod schedule;
pub mod scene;
pub mod search;
pub mod service;
pub mod settings;
pub mod store;
pub mod undo;
pub mod window;

pub use error::{CoreError, ErrorPulse, Result};
pub use events::{Actor, DomainEvent, NewEvent, StoredEvent};
pub use fleet::{FleetState, FleetSummary, SessionRecord};
pub use service::{AccountPatch, ClientExitOutcome, FleetService, NewAccount, QueuedBatch};
pub use store::{EventStore, JsonlEventStore, MemoryEventStore};
