//! Account abstraction for Cougr game accounts.
//!
//! This module provides a unified interface for both Classic (G-address)
//! and Contract (C-address) Stellar accounts, enabling features like
//! session keys for gasless gameplay.
//!
//! ## Architecture
//!
//! - **`types`**: Core account types (`GameAction`, `SessionScope`, `SessionKey`, etc.)
//! - **`traits`**: `CougrAccount` and `SessionKeyProvider` traits
//! - **`classic`**: Classic Stellar account implementation
//! - **`contract`**: Contract account with session key support
//! - **`error`**: Account-specific error types
//! - **`testing`**: Mock account for unit testing
//!
//! ## Usage
//!
//! ```ignore
//! use cougr_core::accounts::{ClassicAccount, CougrAccount};
//!
//! let account = ClassicAccount::new(player_address);
//! account.authorize(&env, &action)?;
//! ```

pub mod batch;
pub mod classic;
pub mod contract;
pub mod error;
pub mod multi_device;
pub mod recovery;
pub mod storage;
#[cfg(any(test, feature = "testutils"))]
pub mod testing;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use batch::BatchBuilder;
pub use classic::ClassicAccount;
pub use contract::ContractAccount;
pub use error::AccountError;
pub use multi_device::{DeviceKey, DevicePolicy, MultiDeviceProvider};
pub use recovery::{Guardian, RecoveryConfig, RecoveryProvider, RecoveryRequest};
pub use storage::SessionStorage;
#[cfg(any(test, feature = "testutils"))]
pub use testing::MockAccount;
pub use traits::{CougrAccount, SessionKeyProvider};
pub use types::{AccountCapabilities, AuthMethod, GameAction, SessionKey, SessionScope};
