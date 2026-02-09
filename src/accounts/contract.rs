use soroban_sdk::{Address, BytesN, Env};

use super::error::AccountError;
use super::storage::SessionStorage;
use super::traits::{CougrAccount, SessionKeyProvider};
use super::types::{AccountCapabilities, GameAction, SessionKey, SessionScope};

/// A Contract Stellar account (C-address).
///
/// Wraps a contract address and provides full account abstraction
/// including session key management. Session keys are persisted via
/// [`SessionStorage`] so they survive across contract invocations.
pub struct ContractAccount {
    address: Address,
}

impl ContractAccount {
    /// Create a new Contract account wrapper.
    pub fn new(address: Address) -> Self {
        Self { address }
    }

    /// Returns the number of active session keys.
    pub fn session_count(&self, env: &Env) -> usize {
        SessionStorage::load_all(env, &self.address).len() as usize
    }
}

impl CougrAccount for ContractAccount {
    fn address(&self) -> &Address {
        &self.address
    }

    fn capabilities(&self) -> AccountCapabilities {
        AccountCapabilities {
            can_batch: true,
            has_session_keys: true,
            has_social_recovery: true,
            has_passkey_auth: true,
        }
    }

    fn authorize(&self, env: &Env, action: &GameAction) -> Result<(), AccountError> {
        // Check if any active session key covers this action
        let keys = SessionStorage::load_all(env, &self.address);
        for i in 0..keys.len() {
            if let Some(key) = keys.get(i) {
                let mut found = false;
                for j in 0..key.scope.allowed_actions.len() {
                    if key.scope.allowed_actions.get(j).unwrap() == action.system_name {
                        found = true;
                        break;
                    }
                }
                if found && key.operations_used < key.scope.max_operations {
                    return Ok(());
                }
            }
        }
        // Fallback to require_auth
        self.address.require_auth();
        Ok(())
    }
}

impl SessionKeyProvider for ContractAccount {
    fn create_session(
        &mut self,
        env: &Env,
        scope: SessionScope,
    ) -> Result<SessionKey, AccountError> {
        let key = SessionKey {
            key_id: BytesN::from_array(env, &[0u8; 32]), // placeholder key ID
            scope,
            created_at: env.ledger().timestamp(),
            operations_used: 0,
        };
        SessionStorage::store(env, &self.address, &key);
        Ok(key)
    }

    fn validate_session(&self, env: &Env, key: &SessionKey) -> Result<bool, AccountError> {
        let now = env.ledger().timestamp();

        // Check expiration
        if now >= key.scope.expires_at {
            return Ok(false);
        }

        // Check operation limit
        if key.operations_used >= key.scope.max_operations {
            return Ok(false);
        }

        Ok(true)
    }

    fn revoke_session(&mut self, env: &Env, key_id: &BytesN<32>) -> Result<(), AccountError> {
        if !SessionStorage::remove(env, &self.address, key_id) {
            return Err(AccountError::InvalidScope);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, symbol_short, testutils::Address as _, vec, Env};

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    #[test]
    fn test_contract_account_creation() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let account = ContractAccount::new(addr.clone());
            assert_eq!(*account.address(), addr);
            assert_eq!(account.session_count(&env), 0);
        });
    }

    #[test]
    fn test_contract_account_capabilities() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let account = ContractAccount::new(addr);
        let caps = account.capabilities();
        assert!(caps.can_batch);
        assert!(caps.has_session_keys);
        assert!(caps.has_social_recovery);
    }

    #[test]
    fn test_create_session() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let scope = SessionScope {
                allowed_actions: vec![&env, symbol_short!("move")],
                max_operations: 100,
                expires_at: 99999,
            };

            let key = account.create_session(&env, scope).unwrap();
            assert_eq!(key.operations_used, 0);
            assert_eq!(account.session_count(&env), 1);
        });
    }

    #[test]
    fn test_validate_session_active() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let scope = SessionScope {
                allowed_actions: vec![&env, symbol_short!("move")],
                max_operations: 100,
                expires_at: 99999,
            };

            let key = account.create_session(&env, scope).unwrap();
            assert_eq!(account.validate_session(&env, &key), Ok(true));
        });
    }

    #[test]
    fn test_validate_session_expired() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let scope = SessionScope {
                allowed_actions: vec![&env, symbol_short!("move")],
                max_operations: 100,
                expires_at: 0, // Already expired
            };

            let key = account.create_session(&env, scope).unwrap();
            assert_eq!(account.validate_session(&env, &key), Ok(false));
        });
    }

    #[test]
    fn test_validate_session_ops_exhausted() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let scope = SessionScope {
                allowed_actions: vec![&env, symbol_short!("move")],
                max_operations: 0, // No operations allowed
                expires_at: 99999,
            };

            let key = account.create_session(&env, scope).unwrap();
            assert_eq!(account.validate_session(&env, &key), Ok(false));
        });
    }

    #[test]
    fn test_revoke_session() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let scope = SessionScope {
                allowed_actions: vec![&env, symbol_short!("move")],
                max_operations: 100,
                expires_at: 99999,
            };

            let key = account.create_session(&env, scope).unwrap();
            assert_eq!(account.session_count(&env), 1);

            account.revoke_session(&env, &key.key_id).unwrap();
            assert_eq!(account.session_count(&env), 0);
        });
    }

    #[test]
    fn test_revoke_nonexistent_session() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let addr = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut account = ContractAccount::new(addr);

            let fake_id = BytesN::from_array(&env, &[99u8; 32]);
            assert_eq!(
                account.revoke_session(&env, &fake_id),
                Err(AccountError::InvalidScope)
            );
        });
    }
}
