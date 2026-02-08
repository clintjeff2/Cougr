//! Zero-knowledge proof support for Cougr.
//!
//! This module provides ergonomic wrappers around Stellar Protocol 25 (X-Ray)
//! cryptographic host functions for use in on-chain game verification.
//!
//! ## Architecture
//!
//! - **`types`**: Core ZK types (`G1Point`, `G2Point`, `Scalar`, `Groth16Proof`, `VerificationKey`)
//! - **`crypto`**: Low-level BN254 and Poseidon wrappers
//! - **`groth16`**: Groth16 proof verification
//! - **`error`**: ZK-specific error types
//! - **`testing`**: Mock types for unit testing without real proofs
//!
//! ## Usage
//!
//! ```ignore
//! use cougr_core::zk::{crypto, groth16, types::*};
//!
//! // Verify a Groth16 proof on-chain
//! let result = groth16::verify_groth16(&env, &vk, &proof, &public_inputs);
//! ```

pub mod circuits;
pub mod components;
pub mod crypto;
pub mod error;
pub mod groth16;
pub mod merkle;
pub mod systems;
pub mod testing;
pub mod types;

// Re-export commonly used items
pub use circuits::{CombatCircuit, InventoryCircuit, MovementCircuit, TurnSequenceCircuit};
pub use components::{CommitReveal, HiddenState, ProofSubmission, VerifiedMarker};
pub use error::ZKError;
pub use groth16::verify_groth16;
pub use merkle::{verify_inclusion, MerkleProof, MerkleTree, OnChainMerkleProof, SparseMerkleTree};
pub use systems::{
    cleanup_verified_system, commit_reveal_deadline_system, encode_commit_reveal,
    encode_verified_marker, verify_proofs_system,
};
pub use types::{G1Point, G2Point, Groth16Proof, Scalar, VerificationKey};
