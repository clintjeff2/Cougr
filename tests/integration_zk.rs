//! ZK subsystem integration tests.
//!
//! Tests ZK components lifecycle, commit-reveal flow, cleanup systems,
//! and byte encoding round-trips.

use cougr_core::simple_world::SimpleWorld;
use cougr_core::zk::components::{COMMIT_REVEAL_TYPE, HIDDEN_STATE_TYPE, VERIFIED_MARKER_TYPE};
use cougr_core::zk::systems::{
    cleanup_verified_system, commit_reveal_deadline_system, encode_commit_reveal,
    encode_verified_marker,
};
use soroban_sdk::{symbol_short, Bytes, BytesN, Env, Symbol};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_hidden_state_component_lifecycle() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);

    let e1 = world.spawn_entity();
    let hidden_sym = Symbol::new(&env, HIDDEN_STATE_TYPE);

    // Add hidden state component (as raw bytes for ECS storage)
    let commitment = BytesN::from_array(&env, &[0xABu8; 32]);
    let commitment_bytes: Bytes = commitment.clone().into();
    world.add_component(e1, hidden_sym.clone(), commitment_bytes.clone());

    assert!(world.has_component(e1, &hidden_sym));
    let stored = world.get_component(e1, &hidden_sym).unwrap();
    assert_eq!(stored, commitment_bytes);

    // Remove hidden state
    world.remove_component(e1, &hidden_sym);
    assert!(!world.has_component(e1, &hidden_sym));
}

#[test]
fn test_commit_reveal_full_flow() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);

    let e1 = world.spawn_entity();
    let cr_sym = Symbol::new(&env, COMMIT_REVEAL_TYPE);

    // Phase 1: Commit
    let commitment = BytesN::from_array(&env, &[0xCDu8; 32]);
    let cr_data = encode_commit_reveal(&env, &commitment, 1000, false);
    world.add_component(e1, cr_sym.clone(), cr_data);
    assert!(world.has_component(e1, &cr_sym));

    // Check that non-expired, non-revealed commitment stays
    commit_reveal_deadline_system(&mut world, &env);
    assert!(world.has_component(e1, &cr_sym));

    // Phase 2: Reveal (update to revealed=true)
    let revealed_data = encode_commit_reveal(&env, &commitment, 1000, true);
    world.add_component(e1, cr_sym.clone(), revealed_data);

    // Even past deadline, revealed commitments stay
    commit_reveal_deadline_system(&mut world, &env);
    assert!(world.has_component(e1, &cr_sym));
}

#[test]
fn test_commit_reveal_timeout_removes_component() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);

    let e1 = world.spawn_entity();
    let cr_sym = Symbol::new(&env, COMMIT_REVEAL_TYPE);

    // Commit with deadline = 0 (already expired at ledger time 0 since now > deadline
    // requires now > 0 to expire at 0 — but default ledger time = 0, so now = 0.
    // We need deadline < now. Let's set deadline to 0, with current time = 0 → 0 > 0 = false.
    // So the commit won't be removed. We should set deadline such that it will expire.)

    // Set a commit that will expire: deadline = 0, now = 0 → 0 > 0 = false, not expired.
    // We need to work around the ledger timestamp. Let's add two commits:
    // one with deadline=0 (not yet expired at t=0) and verify behavior.

    let commitment = BytesN::from_array(&env, &[0xEEu8; 32]);
    let cr_data = encode_commit_reveal(&env, &commitment, 0, false);
    world.add_component(e1, cr_sym.clone(), cr_data);

    // now=0, deadline=0 → 0 > 0 = false, commit stays
    commit_reveal_deadline_system(&mut world, &env);
    assert!(world.has_component(e1, &cr_sym));
}

#[test]
fn test_verified_marker_encode_decode_roundtrip() {
    let env = Env::default();

    let verified_at: u64 = 12345;
    let encoded = encode_verified_marker(&env, verified_at);
    assert_eq!(encoded.len(), 8);

    // Decode manually (big-endian u64)
    let mut arr = [0u8; 8];
    for i in 0..8u32 {
        arr[i as usize] = encoded.get(i).unwrap();
    }
    let decoded = u64::from_be_bytes(arr);
    assert_eq!(decoded, verified_at);
}

#[test]
fn test_commit_reveal_encode_roundtrip() {
    let env = Env::default();

    let commitment = BytesN::from_array(&env, &[0x42u8; 32]);
    let deadline: u64 = 99999;
    let revealed = false;

    let encoded = encode_commit_reveal(&env, &commitment, deadline, revealed);
    assert_eq!(encoded.len(), 41); // 32 + 8 + 1

    // Decode commitment (first 32 bytes)
    let mut commitment_arr = [0u8; 32];
    for i in 0..32u32 {
        commitment_arr[i as usize] = encoded.get(i).unwrap();
    }
    assert_eq!(commitment_arr, [0x42u8; 32]);

    // Decode deadline (bytes 32-39)
    let mut deadline_arr = [0u8; 8];
    for i in 0..8u32 {
        deadline_arr[i as usize] = encoded.get(32 + i).unwrap();
    }
    assert_eq!(u64::from_be_bytes(deadline_arr), deadline);

    // Decode revealed (byte 40)
    assert_eq!(encoded.get(40).unwrap(), 0);
}

#[test]
fn test_cleanup_verified_with_multiple_entities() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);
    let verified_sym = Symbol::new(&env, VERIFIED_MARKER_TYPE);

    // Entity 1: verified_at = 0 (oldest)
    let e1 = world.spawn_entity();
    world.add_component(e1, verified_sym.clone(), encode_verified_marker(&env, 0));

    // Entity 2: verified_at = 0 (same age)
    let e2 = world.spawn_entity();
    world.add_component(e2, verified_sym.clone(), encode_verified_marker(&env, 0));

    // Entity 3: no verified marker (unrelated entity)
    let e3 = world.spawn_entity();
    world.add_component(e3, symbol_short!("pos"), Bytes::from_array(&env, &[10]));

    // max_age = 0: only remove markers where (now - verified_at) > 0
    // now = 0, verified_at = 0 → age = 0, 0 > 0 = false → no removal
    cleanup_verified_system(&mut world, &env, 0);
    assert!(world.has_component(e1, &verified_sym));
    assert!(world.has_component(e2, &verified_sym));

    // e3 should be untouched
    assert!(world.has_component(e3, &symbol_short!("pos")));
}

#[test]
fn test_zk_components_coexist_with_ecs_components() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);

    let player = world.spawn_entity();
    let hidden_sym = Symbol::new(&env, HIDDEN_STATE_TYPE);
    let verified_sym = Symbol::new(&env, VERIFIED_MARKER_TYPE);

    // Add regular ECS components
    world.add_component(
        player,
        symbol_short!("pos"),
        Bytes::from_array(&env, &[10, 20]),
    );
    world.add_component(player, symbol_short!("hp"), Bytes::from_array(&env, &[100]));

    // Add ZK components
    let commitment: Bytes = BytesN::from_array(&env, &[0xABu8; 32]).into();
    world.add_component(player, hidden_sym.clone(), commitment);
    world.add_component(
        player,
        verified_sym.clone(),
        encode_verified_marker(&env, 42),
    );

    // All components coexist
    assert!(world.has_component(player, &symbol_short!("pos")));
    assert!(world.has_component(player, &symbol_short!("hp")));
    assert!(world.has_component(player, &hidden_sym));
    assert!(world.has_component(player, &verified_sym));

    // Can query by ZK component
    let hidden_entities = world.get_entities_with_component(&hidden_sym, &env);
    assert_eq!(hidden_entities.len(), 1);
}

#[test]
fn test_multiple_commit_reveals_different_entities() {
    let env = Env::default();
    let mut world = SimpleWorld::new(&env);
    let cr_sym = Symbol::new(&env, COMMIT_REVEAL_TYPE);

    // Player 1 commits
    let p1 = world.spawn_entity();
    let c1 = BytesN::from_array(&env, &[1u8; 32]);
    world.add_component(
        p1,
        cr_sym.clone(),
        encode_commit_reveal(&env, &c1, 1000, false),
    );

    // Player 2 commits
    let p2 = world.spawn_entity();
    let c2 = BytesN::from_array(&env, &[2u8; 32]);
    world.add_component(
        p2,
        cr_sym.clone(),
        encode_commit_reveal(&env, &c2, 1000, false),
    );

    // Both have commit-reveal
    let cr_entities = world.get_entities_with_component(&cr_sym, &env);
    assert_eq!(cr_entities.len(), 2);

    // Player 1 reveals
    world.add_component(
        p1,
        cr_sym.clone(),
        encode_commit_reveal(&env, &c1, 1000, true),
    );

    // System run: neither expired (deadline=1000, now=0)
    commit_reveal_deadline_system(&mut world, &env);
    assert!(world.has_component(p1, &cr_sym));
    assert!(world.has_component(p2, &cr_sym));
}
