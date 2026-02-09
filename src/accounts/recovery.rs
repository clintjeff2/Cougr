use alloc::vec::Vec;
use soroban_sdk::{contracttype, Address, Env};

use super::error::AccountError;

/// A guardian that can participate in account recovery.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Guardian {
    /// The guardian's Stellar address.
    pub address: Address,
    /// Ledger timestamp when the guardian was added.
    pub added_at: u64,
}

/// A pending account recovery request.
#[contracttype]
#[derive(Clone, Debug)]
pub struct RecoveryRequest {
    /// The proposed new owner address.
    pub new_owner: Address,
    /// Addresses of guardians that have approved so far.
    pub approvals: soroban_sdk::Vec<Address>,
    /// Ledger timestamp when recovery was initiated.
    pub initiated_at: u64,
    /// Earliest ledger timestamp when recovery can be executed.
    pub timelock_until: u64,
    /// Whether this request has been cancelled.
    pub cancelled: bool,
}

/// Configuration for the recovery mechanism.
#[contracttype]
#[derive(Clone, Debug)]
pub struct RecoveryConfig {
    /// Number of guardians required to approve a recovery.
    pub threshold: u32,
    /// Ledger duration to wait after threshold is met before execution.
    pub timelock_period: u64,
    /// Maximum number of guardians allowed.
    pub max_guardians: u32,
}

/// Trait for account types that support social recovery.
pub trait RecoveryProvider {
    /// Add a guardian to the account.
    fn add_guardian(&mut self, env: &Env, guardian: Address) -> Result<(), AccountError>;

    /// Remove a guardian from the account.
    fn remove_guardian(&mut self, env: &Env, guardian: &Address) -> Result<(), AccountError>;

    /// Initiate account recovery to a new owner.
    fn initiate_recovery(
        &mut self,
        env: &Env,
        new_owner: Address,
    ) -> Result<RecoveryRequest, AccountError>;

    /// Approve an active recovery request as a guardian.
    fn approve_recovery(&mut self, env: &Env, guardian: &Address) -> Result<(), AccountError>;

    /// Cancel the active recovery request.
    fn cancel_recovery(&mut self, env: &Env) -> Result<(), AccountError>;

    /// Execute recovery after timelock has passed and threshold is met.
    /// Returns the new owner address.
    fn execute_recovery(&mut self, env: &Env) -> Result<Address, AccountError>;

    /// Returns the number of guardians.
    fn guardian_count(&self) -> usize;

    /// Returns the recovery configuration.
    fn recovery_config(&self) -> &RecoveryConfig;
}

/// A basic implementation of `RecoveryProvider` for in-memory use.
///
/// Guardians and recovery state are stored in memory. For persistent
/// storage, use Soroban contract storage.
pub struct RecoverableAccount {
    address: Address,
    guardians: Vec<Guardian>,
    config: RecoveryConfig,
    active_request: Option<RecoveryRequest>,
}

impl RecoverableAccount {
    /// Create a new recoverable account with the given config.
    pub fn new(address: Address, config: RecoveryConfig) -> Self {
        Self {
            address,
            guardians: Vec::new(),
            config,
            active_request: None,
        }
    }

    /// Returns the account address.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Returns the active recovery request, if any.
    pub fn active_request(&self) -> Option<&RecoveryRequest> {
        self.active_request.as_ref()
    }
}

impl RecoveryProvider for RecoverableAccount {
    fn add_guardian(&mut self, env: &Env, guardian: Address) -> Result<(), AccountError> {
        // Check max guardians
        if self.guardians.len() as u32 >= self.config.max_guardians {
            return Err(AccountError::MaxGuardiansReached);
        }

        // Check for duplicates
        for g in &self.guardians {
            if g.address == guardian {
                return Err(AccountError::GuardianAlreadyExists);
            }
        }

        self.guardians.push(Guardian {
            address: guardian,
            added_at: env.ledger().timestamp(),
        });
        Ok(())
    }

    fn remove_guardian(&mut self, _env: &Env, guardian: &Address) -> Result<(), AccountError> {
        let initial_len = self.guardians.len();
        self.guardians.retain(|g| &g.address != guardian);
        if self.guardians.len() == initial_len {
            return Err(AccountError::InvalidScope);
        }
        Ok(())
    }

    fn initiate_recovery(
        &mut self,
        env: &Env,
        new_owner: Address,
    ) -> Result<RecoveryRequest, AccountError> {
        if self.active_request.is_some() {
            return Err(AccountError::RecoveryAlreadyActive);
        }

        let now = env.ledger().timestamp();
        let request = RecoveryRequest {
            new_owner,
            approvals: soroban_sdk::Vec::new(env),
            initiated_at: now,
            timelock_until: now + self.config.timelock_period,
            cancelled: false,
        };
        self.active_request = Some(request.clone());
        Ok(request)
    }

    fn approve_recovery(&mut self, _env: &Env, guardian: &Address) -> Result<(), AccountError> {
        let request = self
            .active_request
            .as_mut()
            .ok_or(AccountError::RecoveryNotInitiated)?;

        if request.cancelled {
            return Err(AccountError::RecoveryNotInitiated);
        }

        // Verify guardian is valid
        let is_guardian = self.guardians.iter().any(|g| &g.address == guardian);
        if !is_guardian {
            return Err(AccountError::Unauthorized);
        }

        // Check not already approved
        for i in 0..request.approvals.len() {
            if let Some(addr) = request.approvals.get(i) {
                if &addr == guardian {
                    return Ok(()); // already approved, idempotent
                }
            }
        }

        request.approvals.push_back(guardian.clone());
        Ok(())
    }

    fn cancel_recovery(&mut self, _env: &Env) -> Result<(), AccountError> {
        let request = self
            .active_request
            .as_mut()
            .ok_or(AccountError::RecoveryNotInitiated)?;

        request.cancelled = true;
        self.active_request = None;
        Ok(())
    }

    fn execute_recovery(&mut self, env: &Env) -> Result<Address, AccountError> {
        let request = self
            .active_request
            .as_ref()
            .ok_or(AccountError::RecoveryNotInitiated)?;

        if request.cancelled {
            return Err(AccountError::RecoveryNotInitiated);
        }

        // Check threshold
        if request.approvals.len() < self.config.threshold {
            return Err(AccountError::ThresholdNotMet);
        }

        // Check timelock
        let now = env.ledger().timestamp();
        if now < request.timelock_until {
            return Err(AccountError::TimelockNotExpired);
        }

        let new_owner = request.new_owner.clone();
        self.address = new_owner.clone();
        self.active_request = None;
        Ok(new_owner)
    }

    fn guardian_count(&self) -> usize {
        self.guardians.len()
    }

    fn recovery_config(&self) -> &RecoveryConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn default_config() -> RecoveryConfig {
        RecoveryConfig {
            threshold: 2,
            timelock_period: 100,
            max_guardians: 5,
        }
    }

    #[test]
    fn test_add_guardian() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian).unwrap();
        assert_eq!(account.guardian_count(), 1);
    }

    #[test]
    fn test_add_duplicate_guardian() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian.clone()).unwrap();
        let result = account.add_guardian(&env, guardian);
        assert_eq!(result, Err(AccountError::GuardianAlreadyExists));
    }

    #[test]
    fn test_max_guardians() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let config = RecoveryConfig {
            threshold: 1,
            timelock_period: 0,
            max_guardians: 2,
        };
        let mut account = RecoverableAccount::new(addr, config);

        account.add_guardian(&env, Address::generate(&env)).unwrap();
        account.add_guardian(&env, Address::generate(&env)).unwrap();
        let result = account.add_guardian(&env, Address::generate(&env));
        assert_eq!(result, Err(AccountError::MaxGuardiansReached));
    }

    #[test]
    fn test_remove_guardian() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian.clone()).unwrap();
        account.remove_guardian(&env, &guardian).unwrap();
        assert_eq!(account.guardian_count(), 0);
    }

    #[test]
    fn test_initiate_recovery() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        let request = account.initiate_recovery(&env, new_owner).unwrap();
        assert!(!request.cancelled);
        assert_eq!(request.approvals.len(), 0);
    }

    #[test]
    fn test_double_initiate_fails() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();
        let result = account.initiate_recovery(&env, Address::generate(&env));
        assert!(matches!(result, Err(AccountError::RecoveryAlreadyActive)));
    }

    #[test]
    fn test_approve_recovery() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian1 = Address::generate(&env);
        let guardian2 = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian1.clone()).unwrap();
        account.add_guardian(&env, guardian2.clone()).unwrap();
        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();

        account.approve_recovery(&env, &guardian1).unwrap();
        account.approve_recovery(&env, &guardian2).unwrap();

        let request = account.active_request().unwrap();
        assert_eq!(request.approvals.len(), 2);
    }

    #[test]
    fn test_approve_non_guardian_fails() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let non_guardian = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();

        let result = account.approve_recovery(&env, &non_guardian);
        assert_eq!(result, Err(AccountError::Unauthorized));
    }

    #[test]
    fn test_cancel_recovery() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();
        account.cancel_recovery(&env).unwrap();
        assert!(account.active_request().is_none());
    }

    #[test]
    fn test_execute_recovery_threshold_not_met() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian1 = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian1.clone()).unwrap();
        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();
        account.approve_recovery(&env, &guardian1).unwrap();

        // Only 1 approval, threshold is 2
        let result = account.execute_recovery(&env);
        assert_eq!(result, Err(AccountError::ThresholdNotMet));
    }

    #[test]
    fn test_execute_recovery_timelock() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let guardian1 = Address::generate(&env);
        let guardian2 = Address::generate(&env);
        let mut account = RecoverableAccount::new(addr, default_config());

        account.add_guardian(&env, guardian1.clone()).unwrap();
        account.add_guardian(&env, guardian2.clone()).unwrap();
        account
            .initiate_recovery(&env, Address::generate(&env))
            .unwrap();
        account.approve_recovery(&env, &guardian1).unwrap();
        account.approve_recovery(&env, &guardian2).unwrap();

        // Timelock not expired (timestamp 0, timelock_until = 100)
        let result = account.execute_recovery(&env);
        assert_eq!(result, Err(AccountError::TimelockNotExpired));
    }

    #[test]
    fn test_recovery_config() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let config = default_config();
        let account = RecoverableAccount::new(addr, config);

        assert_eq!(account.recovery_config().threshold, 2);
        assert_eq!(account.recovery_config().timelock_period, 100);
        assert_eq!(account.recovery_config().max_guardians, 5);
    }
}
