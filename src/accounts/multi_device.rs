use alloc::vec::Vec;
use soroban_sdk::{contracttype, BytesN, Env, Symbol};

use super::error::AccountError;

/// A registered device key with metadata.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceKey {
    /// Unique identifier for this device key.
    pub key_id: BytesN<32>,
    /// Human-readable device name (e.g., "phone", "laptop").
    pub device_name: Symbol,
    /// Ledger timestamp when the device was registered.
    pub registered_at: u64,
    /// Ledger timestamp of the last use.
    pub last_used: u64,
    /// Whether this device key is currently active.
    pub is_active: bool,
}

/// Policy for multi-device management.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DevicePolicy {
    /// Maximum number of devices that can be registered.
    pub max_devices: u32,
    /// Number of ledger slots of inactivity before auto-revoke.
    /// Set to 0 to disable auto-revoke.
    pub auto_revoke_after: u64,
}

/// Trait for account types that support multi-device key management.
pub trait MultiDeviceProvider {
    /// Register a new device key.
    fn register_device(
        &mut self,
        env: &Env,
        key_id: BytesN<32>,
        device_name: Symbol,
    ) -> Result<DeviceKey, AccountError>;

    /// Revoke a device key by its ID.
    fn revoke_device(&mut self, env: &Env, key_id: &BytesN<32>) -> Result<(), AccountError>;

    /// List all registered device keys (active and inactive).
    fn list_devices(&self) -> &[DeviceKey];

    /// Returns the number of active devices.
    fn active_device_count(&self) -> usize;

    /// Update the last_used timestamp for a device.
    fn update_last_used(&mut self, env: &Env, key_id: &BytesN<32>) -> Result<(), AccountError>;

    /// Set the device management policy.
    fn set_policy(&mut self, policy: DevicePolicy);

    /// Get the current device policy.
    fn policy(&self) -> &DevicePolicy;

    /// Revoke devices that have been inactive beyond the policy's auto_revoke_after.
    /// Returns the number of devices revoked.
    fn cleanup_inactive(&mut self, env: &Env) -> u32;
}

/// A basic in-memory implementation of multi-device management.
pub struct DeviceManager {
    devices: Vec<DeviceKey>,
    device_policy: DevicePolicy,
}

impl DeviceManager {
    /// Create a new device manager with the given policy.
    pub fn new(policy: DevicePolicy) -> Self {
        Self {
            devices: Vec::new(),
            device_policy: policy,
        }
    }

    /// Create a device manager with a default policy.
    pub fn with_defaults() -> Self {
        Self {
            devices: Vec::new(),
            device_policy: DevicePolicy {
                max_devices: 5,
                auto_revoke_after: 0,
            },
        }
    }
}

impl MultiDeviceProvider for DeviceManager {
    fn register_device(
        &mut self,
        env: &Env,
        key_id: BytesN<32>,
        device_name: Symbol,
    ) -> Result<DeviceKey, AccountError> {
        // Check device limit
        let active_count = self.active_device_count() as u32;
        if active_count >= self.device_policy.max_devices {
            return Err(AccountError::DeviceLimitReached);
        }

        // Check for duplicate key_id
        for d in &self.devices {
            if d.key_id == key_id && d.is_active {
                return Err(AccountError::DeviceLimitReached);
            }
        }

        let now = env.ledger().timestamp();
        let device = DeviceKey {
            key_id,
            device_name,
            registered_at: now,
            last_used: now,
            is_active: true,
        };
        self.devices.push(device.clone());
        Ok(device)
    }

    fn revoke_device(&mut self, _env: &Env, key_id: &BytesN<32>) -> Result<(), AccountError> {
        for d in &mut self.devices {
            if &d.key_id == key_id && d.is_active {
                d.is_active = false;
                return Ok(());
            }
        }
        Err(AccountError::DeviceNotFound)
    }

    fn list_devices(&self) -> &[DeviceKey] {
        &self.devices
    }

    fn active_device_count(&self) -> usize {
        self.devices.iter().filter(|d| d.is_active).count()
    }

    fn update_last_used(&mut self, env: &Env, key_id: &BytesN<32>) -> Result<(), AccountError> {
        let now = env.ledger().timestamp();
        for d in &mut self.devices {
            if &d.key_id == key_id && d.is_active {
                d.last_used = now;
                return Ok(());
            }
        }
        Err(AccountError::DeviceNotFound)
    }

    fn set_policy(&mut self, policy: DevicePolicy) {
        self.device_policy = policy;
    }

    fn policy(&self) -> &DevicePolicy {
        &self.device_policy
    }

    fn cleanup_inactive(&mut self, env: &Env) -> u32 {
        if self.device_policy.auto_revoke_after == 0 {
            return 0;
        }

        let now = env.ledger().timestamp();
        let threshold = self.device_policy.auto_revoke_after;
        let mut revoked: u32 = 0;

        for d in &mut self.devices {
            if d.is_active && now.saturating_sub(d.last_used) > threshold {
                d.is_active = false;
                revoked += 1;
            }
        }

        revoked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, Env};

    fn default_policy() -> DevicePolicy {
        DevicePolicy {
            max_devices: 3,
            auto_revoke_after: 0,
        }
    }

    #[test]
    fn test_register_device() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy());

        let key_id = BytesN::from_array(&env, &[1u8; 32]);
        let device = manager
            .register_device(&env, key_id, symbol_short!("phone"))
            .unwrap();

        assert!(device.is_active);
        assert_eq!(manager.active_device_count(), 1);
    }

    #[test]
    fn test_device_limit() {
        let env = Env::default();
        let policy = DevicePolicy {
            max_devices: 2,
            auto_revoke_after: 0,
        };
        let mut manager = DeviceManager::new(policy);

        manager
            .register_device(
                &env,
                BytesN::from_array(&env, &[1u8; 32]),
                symbol_short!("dev1"),
            )
            .unwrap();
        manager
            .register_device(
                &env,
                BytesN::from_array(&env, &[2u8; 32]),
                symbol_short!("dev2"),
            )
            .unwrap();

        let result = manager.register_device(
            &env,
            BytesN::from_array(&env, &[3u8; 32]),
            symbol_short!("dev3"),
        );
        assert_eq!(result, Err(AccountError::DeviceLimitReached));
    }

    #[test]
    fn test_revoke_device() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy());
        let key_id = BytesN::from_array(&env, &[1u8; 32]);

        manager
            .register_device(&env, key_id.clone(), symbol_short!("phone"))
            .unwrap();
        manager.revoke_device(&env, &key_id).unwrap();

        assert_eq!(manager.active_device_count(), 0);
        assert_eq!(manager.list_devices().len(), 1); // still in list, just inactive
    }

    #[test]
    fn test_revoke_nonexistent() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy());
        let key_id = BytesN::from_array(&env, &[99u8; 32]);

        let result = manager.revoke_device(&env, &key_id);
        assert_eq!(result, Err(AccountError::DeviceNotFound));
    }

    #[test]
    fn test_update_last_used() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy());
        let key_id = BytesN::from_array(&env, &[1u8; 32]);

        manager
            .register_device(&env, key_id.clone(), symbol_short!("phone"))
            .unwrap();
        manager.update_last_used(&env, &key_id).unwrap();

        // Should not error
        let devices = manager.list_devices();
        assert_eq!(devices.len(), 1);
    }

    #[test]
    fn test_update_last_used_nonexistent() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy());
        let key_id = BytesN::from_array(&env, &[99u8; 32]);

        let result = manager.update_last_used(&env, &key_id);
        assert_eq!(result, Err(AccountError::DeviceNotFound));
    }

    #[test]
    fn test_cleanup_inactive_disabled() {
        let env = Env::default();
        let mut manager = DeviceManager::new(default_policy()); // auto_revoke_after = 0

        manager
            .register_device(
                &env,
                BytesN::from_array(&env, &[1u8; 32]),
                symbol_short!("phone"),
            )
            .unwrap();

        let revoked = manager.cleanup_inactive(&env);
        assert_eq!(revoked, 0);
        assert_eq!(manager.active_device_count(), 1);
    }

    #[test]
    fn test_set_policy() {
        let mut manager = DeviceManager::new(default_policy());
        let new_policy = DevicePolicy {
            max_devices: 10,
            auto_revoke_after: 500,
        };
        manager.set_policy(new_policy);
        assert_eq!(manager.policy().max_devices, 10);
        assert_eq!(manager.policy().auto_revoke_after, 500);
    }

    #[test]
    fn test_with_defaults() {
        let manager = DeviceManager::with_defaults();
        assert_eq!(manager.policy().max_devices, 5);
        assert_eq!(manager.active_device_count(), 0);
    }

    #[test]
    fn test_revoked_device_allows_new_registration() {
        let env = Env::default();
        let policy = DevicePolicy {
            max_devices: 1,
            auto_revoke_after: 0,
        };
        let mut manager = DeviceManager::new(policy);

        let key1 = BytesN::from_array(&env, &[1u8; 32]);
        manager
            .register_device(&env, key1.clone(), symbol_short!("old"))
            .unwrap();
        manager.revoke_device(&env, &key1).unwrap();

        // Should be able to register again since active count is 0
        let key2 = BytesN::from_array(&env, &[2u8; 32]);
        manager
            .register_device(&env, key2, symbol_short!("new"))
            .unwrap();
        assert_eq!(manager.active_device_count(), 1);
    }
}
