//! Merkle tree utilities for on-chain state verification.
//!
//! Provides SHA256-based Merkle tree construction, inclusion proofs,
//! and sparse Merkle tree for key-value state spaces.
//!
//! # Architecture
//!
//! - **`tree`**: In-memory Merkle tree construction from leaves
//! - **`proof`**: On-chain proof types and verification
//! - **`sparse`**: Sparse Merkle tree for large state spaces
//!
//! Trees are computed in-memory; only the root is stored on-chain.
//! Proofs are compact and can be verified on-chain.

pub mod proof;
pub mod sparse;
pub mod tree;

pub use proof::{verify_inclusion, OnChainMerkleProof};
pub use sparse::SparseMerkleTree;
pub use tree::{MerkleProof, MerkleTree};
