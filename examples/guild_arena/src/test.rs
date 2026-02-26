use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Env};

fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GuildArenaContract, ());
    (env, contract_id)
}

fn register_test_player(env: &Env, contract_id: &Address, player: &Address, guardians: &[Address]) {
    let client = GuildArenaContractClient::new(env, contract_id);
    let guardian_vec = Vec::from_slice(env, guardians);
    client.register_player(player, &guardian_vec, &2, &604800);
}

fn add_device_for_player(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    device: &Address,
    level: DevicePolicyLevel,
) {
    let client = GuildArenaContractClient::new(env, contract_id);
    let policy = DevicePolicyEntry { level };
    client.add_device(player, device, &policy);
}

#[test]
fn test_register_player_with_guardians() {
    let (env, contract_id) = setup_env();
    let player = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);

    register_test_player(&env, &contract_id, &player, &[g1, g2, g3]);

    let client = GuildArenaContractClient::new(&env, &contract_id);
    let profile = client.get_player(&player);
    assert_eq!(profile.fighter.health, 100);
    assert_eq!(profile.record.rating, 1200);
    assert_eq!(profile.record.wins, 0);
}

#[test]
fn test_add_and_remove_device() {
    let (env, contract_id) = setup_env();
    let player = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let desktop = Address::generate(&env);
    let mobile = Address::generate(&env);

    register_test_player(&env, &contract_id, &player, &[g1, g2, g3]);
    add_device_for_player(
        &env,
        &contract_id,
        &player,
        &desktop,
        DevicePolicyLevel::Full,
    );
    add_device_for_player(
        &env,
        &contract_id,
        &player,
        &mobile,
        DevicePolicyLevel::PlayOnly,
    );

    let client = GuildArenaContractClient::new(&env, &contract_id);
    client.remove_device(&player, &mobile);
}

#[test]
fn test_multi_device_play() {
    let (env, contract_id) = setup_env();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let desktop = Address::generate(&env);
    let mobile = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player1,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    register_test_player(&env, &contract_id, &player2, &[g1, g2, g3]);
    add_device_for_player(
        &env,
        &contract_id,
        &player1,
        &desktop,
        DevicePolicyLevel::Full,
    );
    add_device_for_player(
        &env,
        &contract_id,
        &player2,
        &mobile,
        DevicePolicyLevel::PlayOnly,
    );

    let client = GuildArenaContractClient::new(&env, &contract_id);
    client.start_match(&desktop);
    client.start_match(&mobile);

    let arena = client.get_match();
    assert_eq!(arena.status, MatchStatus::InProgress);
}

#[test]
fn test_combat_full_match() {
    let (env, contract_id) = setup_env();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player1,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    register_test_player(&env, &contract_id, &player2, &[g1, g2, g3]);
    add_device_for_player(&env, &contract_id, &player1, &dev1, DevicePolicyLevel::Full);
    add_device_for_player(&env, &contract_id, &player2, &dev2, DevicePolicyLevel::Full);

    let client = GuildArenaContractClient::new(&env, &contract_id);
    client.start_match(&dev1);
    client.start_match(&dev2);

    let mut finished = false;
    for _ in 0..50 {
        let result = client.submit_action(&dev1, &CombatAction::Attack);
        if result.finished {
            finished = true;
            break;
        }
        let result = client.submit_action(&dev2, &CombatAction::Attack);
        if result.finished {
            finished = true;
            break;
        }
    }

    assert!(finished);
    let arena = client.get_match();
    assert_eq!(arena.status, MatchStatus::Finished);
}

#[test]
fn test_rating_updates_after_match() {
    let (env, contract_id) = setup_env();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player1,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    register_test_player(&env, &contract_id, &player2, &[g1, g2, g3]);
    add_device_for_player(&env, &contract_id, &player1, &dev1, DevicePolicyLevel::Full);
    add_device_for_player(&env, &contract_id, &player2, &dev2, DevicePolicyLevel::Full);

    let client = GuildArenaContractClient::new(&env, &contract_id);
    client.start_match(&dev1);
    client.start_match(&dev2);

    for _ in 0..50 {
        let result = client.submit_action(&dev1, &CombatAction::Special);
        if result.finished {
            break;
        }
        let result = client.submit_action(&dev2, &CombatAction::Defend);
        if result.finished {
            break;
        }
    }

    let arena = client.get_match();
    let winner_profile = client.get_player(&arena.winner);
    assert!(winner_profile.record.wins > 0);
    assert!(winner_profile.record.rating > 1200);
}

#[test]
#[should_panic(expected = "insufficient device permissions")]
fn test_device_policy_enforcement() {
    let (env, contract_id) = setup_env();
    let player = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let mobile = Address::generate(&env);

    register_test_player(&env, &contract_id, &player, &[g1, g2, g3]);
    add_device_for_player(
        &env,
        &contract_id,
        &player,
        &mobile,
        DevicePolicyLevel::PlayOnly,
    );

    let _client = GuildArenaContractClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        GuildArenaContract::require_admin_permission(&env, &mobile);
    });
}

#[test]
fn test_full_recovery_flow() {
    let (env, contract_id) = setup_env();
    let player = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let new_key = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player,
        &[g1.clone(), g2.clone(), g3.clone()],
    );

    let client = GuildArenaContractClient::new(&env, &contract_id);

    let profile_before = client.get_player(&player);
    assert_eq!(profile_before.record.rating, 1200);

    client.initiate_recovery(&g1, &player, &new_key);
    client.approve_recovery(&g2, &player, &new_key);

    env.ledger().with_mut(|li| {
        li.timestamp = 604800 + 1;
    });

    client.finalize_recovery(&player);
}

#[test]
#[should_panic]
fn test_recovery_insufficient_approvals() {
    let (env, contract_id) = setup_env();
    let player = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player,
        &[g1.clone(), g2.clone(), g3.clone()],
    );

    let client = GuildArenaContractClient::new(&env, &contract_id);
    let new_key = Address::generate(&env);
    client.initiate_recovery(&g1, &player, &new_key);

    client.finalize_recovery(&player);
}

#[test]
fn test_recovery_preserves_game_state() {
    let (env, contract_id) = setup_env();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let new_key = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player1,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    register_test_player(
        &env,
        &contract_id,
        &player2,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    add_device_for_player(&env, &contract_id, &player1, &dev1, DevicePolicyLevel::Full);
    add_device_for_player(&env, &contract_id, &player2, &dev2, DevicePolicyLevel::Full);

    let client = GuildArenaContractClient::new(&env, &contract_id);

    client.start_match(&dev1);
    client.start_match(&dev2);

    for _ in 0..50 {
        let result = client.submit_action(&dev1, &CombatAction::Special);
        if result.finished {
            break;
        }
        let result = client.submit_action(&dev2, &CombatAction::Defend);
        if result.finished {
            break;
        }
    }

    let profile_before = client.get_player(&player1);
    let rating_before = profile_before.record.rating;
    let wins_before = profile_before.record.wins;
    let losses_before = profile_before.record.losses;

    client.initiate_recovery(&g1, &player1, &new_key);
    client.approve_recovery(&g2, &player1, &new_key);

    env.ledger().with_mut(|li| {
        li.timestamp = 604800 + 1;
    });

    client.finalize_recovery(&player1);

    let profile_after = client.get_player(&new_key);
    assert_eq!(profile_after.record.rating, rating_before);
    assert_eq!(profile_after.record.wins, wins_before);
    assert_eq!(profile_after.record.losses, losses_before);
}

#[test]
fn test_special_attack_damage() {
    let (env, contract_id) = setup_env();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    register_test_player(
        &env,
        &contract_id,
        &player1,
        &[g1.clone(), g2.clone(), g3.clone()],
    );
    register_test_player(&env, &contract_id, &player2, &[g1, g2, g3]);
    add_device_for_player(&env, &contract_id, &player1, &dev1, DevicePolicyLevel::Full);
    add_device_for_player(&env, &contract_id, &player2, &dev2, DevicePolicyLevel::Full);

    let client = GuildArenaContractClient::new(&env, &contract_id);
    client.start_match(&dev1);
    client.start_match(&dev2);

    let result = client.submit_action(&dev1, &CombatAction::Special);
    assert!(result.defender_hp < 100);
    let special_damage = 100 - result.defender_hp;

    let _result2 = client.submit_action(&dev2, &CombatAction::Defend);
    let defend_damage = 100u32.saturating_sub(result.challenger_hp);

    assert!(special_damage > defend_damage || defend_damage == 0);
}
