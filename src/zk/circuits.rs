use soroban_sdk::{BytesN, Env, Vec};

use super::error::ZKError;
use super::groth16::verify_groth16;
use super::types::{Groth16Proof, Scalar, VerificationKey};

/// Movement verification circuit interface.
///
/// Verifies that a player's move is valid (within maximum allowed distance)
/// without revealing the full game state. The circuit's public inputs are:
/// `[from_x, from_y, to_x, to_y, max_distance]`.
///
/// # Example
/// ```ignore
/// let circuit = MovementCircuit::new(vk, 10);
/// let valid = circuit.verify_move(&env, &proof, 0, 0, 3, 4)?;
/// ```
pub struct MovementCircuit {
    pub vk: VerificationKey,
    pub max_distance: u32,
}

impl MovementCircuit {
    /// Create a new movement circuit with the given verification key and max distance.
    pub fn new(vk: VerificationKey, max_distance: u32) -> Self {
        Self { vk, max_distance }
    }

    /// Verify a move from (from_x, from_y) to (to_x, to_y).
    ///
    /// The proof must demonstrate that the move is within `max_distance`.
    /// Public inputs are encoded as: `[from_x, from_y, to_x, to_y, max_distance]`.
    pub fn verify_move(
        &self,
        env: &Env,
        proof: &Groth16Proof,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    ) -> Result<bool, ZKError> {
        let public_inputs = Self::encode_inputs(env, from_x, from_y, to_x, to_y, self.max_distance);
        verify_groth16(env, &self.vk, proof, &public_inputs)
    }

    /// Encode movement parameters as BN254 scalars for the circuit.
    fn encode_inputs(
        env: &Env,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        max_distance: u32,
    ) -> alloc::vec::Vec<Scalar> {
        alloc::vec![
            Self::i32_to_scalar(env, from_x),
            Self::i32_to_scalar(env, from_y),
            Self::i32_to_scalar(env, to_x),
            Self::i32_to_scalar(env, to_y),
            Self::u32_to_scalar(env, max_distance),
        ]
    }

    fn i32_to_scalar(env: &Env, val: i32) -> Scalar {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_le_bytes();
        bytes[..4].copy_from_slice(&val_bytes);
        Scalar {
            bytes: BytesN::from_array(env, &bytes),
        }
    }

    fn u32_to_scalar(env: &Env, val: u32) -> Scalar {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_le_bytes();
        bytes[..4].copy_from_slice(&val_bytes);
        Scalar {
            bytes: BytesN::from_array(env, &bytes),
        }
    }
}

/// Combat verification circuit interface.
///
/// Verifies damage calculation without revealing hidden player stats.
/// Public inputs: `[attacker_commitment, defender_commitment, damage_result]`.
pub struct CombatCircuit {
    pub vk: VerificationKey,
}

impl CombatCircuit {
    /// Create a new combat circuit with the given verification key.
    pub fn new(vk: VerificationKey) -> Self {
        Self { vk }
    }

    /// Verify a damage calculation.
    ///
    /// The proof demonstrates that `damage_result` was correctly computed
    /// from the hidden stats of the attacker and defender.
    pub fn verify_damage(
        &self,
        env: &Env,
        proof: &Groth16Proof,
        attacker_commitment: &BytesN<32>,
        defender_commitment: &BytesN<32>,
        damage_result: u32,
    ) -> Result<bool, ZKError> {
        let public_inputs = alloc::vec![
            Scalar {
                bytes: attacker_commitment.clone(),
            },
            Scalar {
                bytes: defender_commitment.clone(),
            },
            Self::u32_to_scalar(env, damage_result),
        ];
        verify_groth16(env, &self.vk, proof, &public_inputs)
    }

    fn u32_to_scalar(env: &Env, val: u32) -> Scalar {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_le_bytes();
        bytes[..4].copy_from_slice(&val_bytes);
        Scalar {
            bytes: BytesN::from_array(env, &bytes),
        }
    }
}

/// Inventory verification circuit interface.
///
/// Proves a player has a specific item without revealing the full inventory.
/// Public inputs: `[inventory_root, item_id]`.
pub struct InventoryCircuit {
    pub vk: VerificationKey,
}

impl InventoryCircuit {
    /// Create a new inventory circuit with the given verification key.
    pub fn new(vk: VerificationKey) -> Self {
        Self { vk }
    }

    /// Verify that an inventory contains a specific item.
    ///
    /// The proof demonstrates knowledge of a Merkle path from the item
    /// to the inventory root.
    pub fn verify_has_item(
        &self,
        env: &Env,
        proof: &Groth16Proof,
        inventory_root: &BytesN<32>,
        item_id: u32,
    ) -> Result<bool, ZKError> {
        let public_inputs = alloc::vec![
            Scalar {
                bytes: inventory_root.clone(),
            },
            Self::u32_to_scalar(env, item_id),
        ];
        verify_groth16(env, &self.vk, proof, &public_inputs)
    }

    fn u32_to_scalar(env: &Env, val: u32) -> Scalar {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_le_bytes();
        bytes[..4].copy_from_slice(&val_bytes);
        Scalar {
            bytes: BytesN::from_array(env, &bytes),
        }
    }
}

/// Turn sequence verification circuit interface.
///
/// Proves a sequence of game actions was executed in valid order
/// with valid state transitions.
/// Public inputs: `[initial_state_hash, final_state_hash, action_count]`.
pub struct TurnSequenceCircuit {
    pub vk: VerificationKey,
}

impl TurnSequenceCircuit {
    /// Create a new turn sequence circuit with the given verification key.
    pub fn new(vk: VerificationKey) -> Self {
        Self { vk }
    }

    /// Verify a sequence of turns.
    pub fn verify_sequence(
        &self,
        env: &Env,
        proof: &Groth16Proof,
        initial_state: &BytesN<32>,
        final_state: &BytesN<32>,
        action_count: u32,
    ) -> Result<bool, ZKError> {
        let public_inputs = alloc::vec![
            Scalar {
                bytes: initial_state.clone(),
            },
            Scalar {
                bytes: final_state.clone(),
            },
            Self::u32_to_scalar(env, action_count),
        ];
        verify_groth16(env, &self.vk, proof, &public_inputs)
    }

    fn u32_to_scalar(env: &Env, val: u32) -> Scalar {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_le_bytes();
        bytes[..4].copy_from_slice(&val_bytes);
        Scalar {
            bytes: BytesN::from_array(env, &bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{BytesN, Env};

    use super::super::types::{G1Point, G2Point};

    fn make_vk(env: &Env, ic_count: u32) -> VerificationKey {
        let g1 = G1Point {
            bytes: BytesN::from_array(env, &[0u8; 64]),
        };
        let g2 = G2Point {
            bytes: BytesN::from_array(env, &[0u8; 128]),
        };
        let mut ic = Vec::new(env);
        for _ in 0..ic_count {
            ic.push_back(g1.clone());
        }
        VerificationKey {
            alpha: g1,
            beta: g2.clone(),
            gamma: g2.clone(),
            delta: g2,
            ic,
        }
    }

    #[test]
    fn test_movement_circuit_creation() {
        let env = Env::default();
        let vk = make_vk(&env, 6); // 5 public inputs + 1
        let circuit = MovementCircuit::new(vk, 10);
        assert_eq!(circuit.max_distance, 10);
    }

    #[test]
    fn test_movement_circuit_wrong_ic_length() {
        let env = Env::default();
        let vk = make_vk(&env, 1); // wrong: needs 6 for 5 inputs
        let circuit = MovementCircuit::new(vk, 10);

        let g1 = G1Point {
            bytes: BytesN::from_array(&env, &[0u8; 64]),
        };
        let g2 = G2Point {
            bytes: BytesN::from_array(&env, &[0u8; 128]),
        };
        let proof = Groth16Proof {
            a: g1.clone(),
            b: g2,
            c: g1,
        };

        let result = circuit.verify_move(&env, &proof, 0, 0, 3, 4);
        assert_eq!(result, Err(ZKError::InvalidVerificationKey));
    }

    #[test]
    fn test_combat_circuit_creation() {
        let env = Env::default();
        let vk = make_vk(&env, 4);
        let circuit = CombatCircuit::new(vk);
        // Just verify it was created
        assert_eq!(circuit.vk.ic.len(), 4);
    }

    #[test]
    fn test_inventory_circuit_creation() {
        let env = Env::default();
        let vk = make_vk(&env, 3);
        let circuit = InventoryCircuit::new(vk);
        assert_eq!(circuit.vk.ic.len(), 3);
    }

    #[test]
    fn test_turn_sequence_circuit_creation() {
        let env = Env::default();
        let vk = make_vk(&env, 4);
        let circuit = TurnSequenceCircuit::new(vk);
        assert_eq!(circuit.vk.ic.len(), 4);
    }

    #[test]
    fn test_scalar_encoding_u32() {
        let env = Env::default();
        let scalar = MovementCircuit::u32_to_scalar(&env, 42);
        assert_eq!(scalar.bytes.len(), 32);
    }

    #[test]
    fn test_scalar_encoding_i32() {
        let env = Env::default();
        let scalar = MovementCircuit::i32_to_scalar(&env, -1);
        assert_eq!(scalar.bytes.len(), 32);
    }
}
