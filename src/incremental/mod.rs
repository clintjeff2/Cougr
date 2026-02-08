//! Incremental serialization for per-entity persistent storage.
//!
//! Unlike `SimpleWorld` which stores all ECS state in a single
//! `#[contracttype]` struct, the incremental module splits state
//! into per-entity storage entries. Only dirty (modified) data is
//! written on `flush()`, reducing gas costs for partial updates.
//!
//! # Modules
//!
//! - **`keys`**: Storage key construction helpers
//! - **`dirty_tracker`**: Tracks which entries need flushing
//! - **`storage_world`**: The `StorageWorld` implementation
//!
//! # Trade-offs
//!
//! - Cheaper writes (only modified entries are flushed)
//! - Costlier full scans (no single-Map iteration)
//! - Best suited for games where systems operate on known entities

pub mod dirty_tracker;
pub mod keys;
pub mod storage_world;

pub use dirty_tracker::DirtyTracker;
pub use storage_world::{StorageWorld, WorldMetadata};
