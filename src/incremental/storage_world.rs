//! World backed by Soroban persistent storage with incremental writes.
//!
//! Unlike `SimpleWorld` which holds all state in a single `#[contracttype]`
//! struct, `StorageWorld` stores each entity and component as separate
//! persistent storage entries. Only dirty (modified) data is written
//! on `flush()`, reducing gas costs for partial updates.
//!
//! # Usage
//!
//! ```ignore
//! let mut world = StorageWorld::load_metadata(&env);
//! world.load_entity(&env, entity_id);
//! world.add_component(&env, entity_id, sym, data);
//! world.flush(&env); // only writes changed entries
//! ```
//!
//! # Trade-offs
//!
//! - **Cheaper writes**: Only modified entities/components are written.
//! - **Costlier full scans**: Querying all entities requires loading them
//!   individually (no single-Map iteration).
//! - Best suited for games where systems operate on known entities.

use super::dirty_tracker::DirtyTracker;
use super::keys;
use crate::error::CougrError;
use crate::simple_world::EntityId;
use crate::simple_world::SimpleWorld;
use alloc::vec::Vec;
use soroban_sdk::{contracttype, Bytes, Env, Map, Symbol};

/// Metadata stored as a single persistent entry.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WorldMetadata {
    /// Next entity ID to assign.
    pub next_entity_id: EntityId,
    /// Version counter for cache invalidation.
    pub version: u64,
    /// Total number of live entities.
    pub entity_count: u32,
    /// List of all live entity IDs.
    pub entity_ids: soroban_sdk::Vec<EntityId>,
}

/// Cached entity data loaded from persistent storage.
struct LoadedEntity {
    entity_id: EntityId,
    component_types: soroban_sdk::Vec<Symbol>,
}

/// Cached component data loaded from persistent storage.
struct LoadedComponent {
    entity_id: EntityId,
    component_type: Symbol,
    data: Bytes,
}

/// World that reads/writes individual entities from Soroban persistent storage.
///
/// Only dirty state is written back on `flush()`. Entities must be explicitly
/// loaded before they can be queried or modified.
pub struct StorageWorld {
    metadata: WorldMetadata,
    loaded_entities: Vec<LoadedEntity>,
    loaded_components: Vec<LoadedComponent>,
    dirty: DirtyTracker,
}

impl StorageWorld {
    /// Load world metadata from persistent storage.
    ///
    /// If no metadata exists, creates a fresh world.
    pub fn load_metadata(env: &Env) -> Self {
        let key = keys::meta_key(env);
        let metadata: WorldMetadata =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(WorldMetadata {
                    next_entity_id: 1,
                    version: 0,
                    entity_count: 0,
                    entity_ids: soroban_sdk::Vec::new(env),
                });

        Self {
            metadata,
            loaded_entities: Vec::new(),
            loaded_components: Vec::new(),
            dirty: DirtyTracker::new(),
        }
    }

    /// Load a single entity's component list and component data from storage.
    pub fn load_entity(&mut self, env: &Env, entity_id: EntityId) -> Result<(), CougrError> {
        // Check if already loaded
        for le in &self.loaded_entities {
            if le.entity_id == entity_id {
                return Ok(());
            }
        }

        let key = keys::entity_key(env, entity_id);
        let component_types: soroban_sdk::Vec<Symbol> = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(CougrError::EntityNotFound)?;

        // Load each component's data
        for i in 0..component_types.len() {
            if let Some(ct) = component_types.get(i) {
                let ckey = keys::component_key(env, entity_id, &ct);
                if let Some(data) = env.storage().persistent().get::<_, Bytes>(&ckey) {
                    self.loaded_components.push(LoadedComponent {
                        entity_id,
                        component_type: ct,
                        data,
                    });
                }
            }
        }

        self.loaded_entities.push(LoadedEntity {
            entity_id,
            component_types,
        });

        Ok(())
    }

    /// Load multiple entities from storage.
    pub fn load_entities(&mut self, env: &Env, entity_ids: &[EntityId]) -> Result<(), CougrError> {
        for &eid in entity_ids {
            self.load_entity(env, eid)?;
        }
        Ok(())
    }

    /// Spawn a new entity (assigns ID but doesn't write to storage until flush).
    pub fn spawn_entity(&mut self, env: &Env) -> EntityId {
        let id = self.metadata.next_entity_id;
        self.metadata.next_entity_id += 1;
        self.metadata.entity_count += 1;
        self.metadata.entity_ids.push_back(id);

        // Add to loaded set with empty component list
        self.loaded_entities.push(LoadedEntity {
            entity_id: id,
            component_types: soroban_sdk::Vec::new(env),
        });

        self.dirty.mark_new_entity(id);
        self.dirty.mark_entity_dirty(id);
        id
    }

    /// Add a component to an entity.
    pub fn add_component(
        &mut self,
        env: &Env,
        entity_id: EntityId,
        component_type: Symbol,
        data: Bytes,
    ) {
        // Update the loaded entity's component list
        let mut found_entity = false;
        for le in &mut self.loaded_entities {
            if le.entity_id == entity_id {
                found_entity = true;
                // Add type if not already present
                let mut has_type = false;
                for i in 0..le.component_types.len() {
                    if let Some(t) = le.component_types.get(i) {
                        if t == component_type {
                            has_type = true;
                            break;
                        }
                    }
                }
                if !has_type {
                    le.component_types.push_back(component_type.clone());
                }
                break;
            }
        }

        if !found_entity {
            // Auto-add to loaded set
            let mut types = soroban_sdk::Vec::new(env);
            types.push_back(component_type.clone());
            self.loaded_entities.push(LoadedEntity {
                entity_id,
                component_types: types,
            });
        }

        // Update or add the loaded component data
        let mut updated = false;
        for lc in &mut self.loaded_components {
            if lc.entity_id == entity_id && lc.component_type == component_type {
                lc.data = data.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            self.loaded_components.push(LoadedComponent {
                entity_id,
                component_type: component_type.clone(),
                data,
            });
        }

        self.metadata.version += 1;
        self.dirty.mark_entity_dirty(entity_id);
        self.dirty.mark_component_dirty(entity_id, component_type);
        self.dirty.mark_meta_dirty();
    }

    /// Get a component's data from loaded state.
    pub fn get_component(&self, entity_id: EntityId, component_type: &Symbol) -> Option<Bytes> {
        for lc in &self.loaded_components {
            if lc.entity_id == entity_id && &lc.component_type == component_type {
                return Some(lc.data.clone());
            }
        }
        None
    }

    /// Check if a loaded entity has a component.
    pub fn has_component(&self, entity_id: EntityId, component_type: &Symbol) -> bool {
        for lc in &self.loaded_components {
            if lc.entity_id == entity_id && &lc.component_type == component_type {
                return true;
            }
        }
        false
    }

    /// Remove a component from an entity.
    pub fn remove_component(&mut self, entity_id: EntityId, component_type: &Symbol) -> bool {
        // Remove from loaded components
        let initial_len = self.loaded_components.len();
        self.loaded_components
            .retain(|lc| !(lc.entity_id == entity_id && &lc.component_type == component_type));
        let removed = self.loaded_components.len() < initial_len;

        if removed {
            // Update entity's component type list
            for le in &mut self.loaded_entities {
                if le.entity_id == entity_id {
                    let env = le.component_types.env().clone();
                    let mut new_types = soroban_sdk::Vec::new(&env);
                    for i in 0..le.component_types.len() {
                        if let Some(t) = le.component_types.get(i) {
                            if &t != component_type {
                                new_types.push_back(t);
                            }
                        }
                    }
                    le.component_types = new_types;
                    break;
                }
            }

            self.metadata.version += 1;
            self.dirty.mark_entity_dirty(entity_id);
            self.dirty
                .mark_component_dirty(entity_id, component_type.clone());
            self.dirty.mark_meta_dirty();
        }

        removed
    }

    /// Despawn an entity, removing all its components.
    pub fn despawn_entity(&mut self, entity_id: EntityId) {
        // Remove all loaded components for this entity
        self.loaded_components
            .retain(|lc| lc.entity_id != entity_id);

        // Remove from loaded entities
        self.loaded_entities.retain(|le| le.entity_id != entity_id);

        // Remove from metadata entity list
        let env = self.metadata.entity_ids.env().clone();
        let mut new_ids = soroban_sdk::Vec::new(&env);
        for i in 0..self.metadata.entity_ids.len() {
            if let Some(eid) = self.metadata.entity_ids.get(i) {
                if eid != entity_id {
                    new_ids.push_back(eid);
                }
            }
        }
        self.metadata.entity_ids = new_ids;
        self.metadata.entity_count = self.metadata.entity_count.saturating_sub(1);

        self.metadata.version += 1;
        self.dirty.mark_despawned(entity_id);
        self.dirty.mark_meta_dirty();
    }

    /// Flush all dirty state to Soroban persistent storage.
    ///
    /// Only writes entries that have been modified since the last flush.
    pub fn flush(&mut self, env: &Env) {
        if !self.dirty.is_dirty() {
            return;
        }

        // Write metadata if dirty
        if self.dirty.is_meta_dirty() {
            let key = keys::meta_key(env);
            env.storage().persistent().set(&key, &self.metadata);
        }

        // Write dirty entity component lists
        for &eid in self.dirty.dirty_entities() {
            for le in &self.loaded_entities {
                if le.entity_id == eid {
                    let key = keys::entity_key(env, eid);
                    env.storage().persistent().set(&key, &le.component_types);
                    break;
                }
            }
        }

        // Write dirty components
        for (eid, ct) in self.dirty.dirty_components() {
            // Check if this is for a despawned entity
            if self.dirty.despawned().contains(eid) {
                continue;
            }
            for lc in &self.loaded_components {
                if lc.entity_id == *eid && &lc.component_type == ct {
                    let key = keys::component_key(env, *eid, ct);
                    env.storage().persistent().set(&key, &lc.data);
                    break;
                }
            }
        }

        // Handle despawned entities: remove all their storage entries
        for &eid in self.dirty.despawned() {
            let ekey = keys::entity_key(env, eid);
            // Load the entity's component types to know what to remove
            if let Some(types) = env
                .storage()
                .persistent()
                .get::<_, soroban_sdk::Vec<Symbol>>(&ekey)
            {
                for i in 0..types.len() {
                    if let Some(ct) = types.get(i) {
                        let ckey = keys::component_key(env, eid, &ct);
                        env.storage().persistent().remove(&ckey);
                    }
                }
            }
            env.storage().persistent().remove(&ekey);
        }

        self.dirty.clear();
    }

    /// Returns the current version counter.
    pub fn version(&self) -> u64 {
        self.metadata.version
    }

    /// Returns the next entity ID.
    pub fn next_entity_id(&self) -> EntityId {
        self.metadata.next_entity_id
    }

    /// Returns the number of live entities.
    pub fn entity_count(&self) -> u32 {
        self.metadata.entity_count
    }

    /// Returns all known entity IDs from metadata.
    pub fn entity_ids(&self) -> &soroban_sdk::Vec<EntityId> {
        &self.metadata.entity_ids
    }

    /// Convert loaded state to a `SimpleWorld`.
    ///
    /// Useful for running systems that expect `SimpleWorld`.
    pub fn to_simple_world(&self, env: &Env) -> SimpleWorld {
        let mut world = SimpleWorld::new(env);
        world.next_entity_id = self.metadata.next_entity_id;
        world.version = self.metadata.version;

        for le in &self.loaded_entities {
            // Add each component from loaded data
            for i in 0..le.component_types.len() {
                if let Some(ct) = le.component_types.get(i) {
                    if let Some(data) = self.get_component(le.entity_id, &ct) {
                        // Use add_component but don't double-increment version
                        world.components.set((le.entity_id, ct.clone()), data);
                    }
                }
            }
            world
                .entity_components
                .set(le.entity_id, le.component_types.clone());
        }

        // Restore version (add_component increments it)
        world.version = self.metadata.version;
        world
    }

    /// Create a StorageWorld from a SimpleWorld (for migration).
    ///
    /// Marks everything as dirty so the next `flush()` writes all state.
    pub fn from_simple_world(world: &SimpleWorld, env: &Env) -> Self {
        let mut entity_ids = soroban_sdk::Vec::new(env);
        let mut loaded_entities = Vec::new();
        let mut loaded_components = Vec::new();
        let mut dirty = DirtyTracker::new();

        let mut entity_count: u32 = 0;

        for eid in world.entity_components.keys().iter() {
            entity_ids.push_back(eid);
            entity_count += 1;

            if let Some(types) = world.entity_components.get(eid) {
                for i in 0..types.len() {
                    if let Some(ct) = types.get(i) {
                        if let Some(data) = world.get_component(eid, &ct) {
                            loaded_components.push(LoadedComponent {
                                entity_id: eid,
                                component_type: ct.clone(),
                                data,
                            });
                            dirty.mark_component_dirty(eid, ct);
                        }
                    }
                }
                loaded_entities.push(LoadedEntity {
                    entity_id: eid,
                    component_types: types,
                });
                dirty.mark_entity_dirty(eid);
                dirty.mark_new_entity(eid);
            }
        }

        dirty.mark_meta_dirty();

        Self {
            metadata: WorldMetadata {
                next_entity_id: world.next_entity_id,
                version: world.version,
                entity_count,
                entity_ids,
            },
            loaded_entities,
            loaded_components,
            dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, symbol_short, Env};

    // Dummy contract for persistent storage context
    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    #[test]
    fn test_load_fresh_metadata() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let world = StorageWorld::load_metadata(&env);
            assert_eq!(world.next_entity_id(), 1);
            assert_eq!(world.version(), 0);
            assert_eq!(world.entity_count(), 0);
        });
    }

    #[test]
    fn test_spawn_entity() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let e1 = world.spawn_entity(&env);
            let e2 = world.spawn_entity(&env);
            assert_eq!(e1, 1);
            assert_eq!(e2, 2);
            assert_eq!(world.entity_count(), 2);
            assert_eq!(world.next_entity_id(), 3);
        });
    }

    #[test]
    fn test_add_and_get_component() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let eid = world.spawn_entity(&env);

            let data = Bytes::from_array(&env, &[1, 2, 3, 4]);
            world.add_component(&env, eid, symbol_short!("pos"), data.clone());

            assert!(world.has_component(eid, &symbol_short!("pos")));
            assert_eq!(world.get_component(eid, &symbol_short!("pos")), Some(data));
            assert!(!world.has_component(eid, &symbol_short!("vel")));
        });
    }

    #[test]
    fn test_remove_component() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let eid = world.spawn_entity(&env);

            let data = Bytes::from_array(&env, &[1]);
            world.add_component(&env, eid, symbol_short!("pos"), data);

            assert!(world.remove_component(eid, &symbol_short!("pos")));
            assert!(!world.has_component(eid, &symbol_short!("pos")));
            assert!(!world.remove_component(eid, &symbol_short!("pos")));
        });
    }

    #[test]
    fn test_despawn_entity() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let eid = world.spawn_entity(&env);

            let data = Bytes::from_array(&env, &[1]);
            world.add_component(&env, eid, symbol_short!("pos"), data.clone());
            world.add_component(&env, eid, symbol_short!("vel"), data);

            world.despawn_entity(eid);
            assert!(!world.has_component(eid, &symbol_short!("pos")));
            assert!(!world.has_component(eid, &symbol_short!("vel")));
            assert_eq!(world.entity_count(), 0);
        });
    }

    #[test]
    fn test_flush_and_reload() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            // Create and flush
            {
                let mut world = StorageWorld::load_metadata(&env);
                let eid = world.spawn_entity(&env);
                let data = Bytes::from_array(&env, &[42, 43]);
                world.add_component(&env, eid, symbol_short!("pos"), data);
                world.flush(&env);
            }

            // Reload and verify
            {
                let mut world = StorageWorld::load_metadata(&env);
                assert_eq!(world.next_entity_id(), 2);
                assert_eq!(world.entity_count(), 1);

                world.load_entity(&env, 1).unwrap();
                assert!(world.has_component(1, &symbol_short!("pos")));
                let data = world.get_component(1, &symbol_short!("pos")).unwrap();
                assert_eq!(data, Bytes::from_array(&env, &[42, 43]));
            }
        });
    }

    #[test]
    fn test_flush_despawn_removes_storage() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            // Create, flush, then despawn and flush
            {
                let mut world = StorageWorld::load_metadata(&env);
                let eid = world.spawn_entity(&env);
                let data = Bytes::from_array(&env, &[1]);
                world.add_component(&env, eid, symbol_short!("pos"), data);
                world.flush(&env);
            }

            {
                let mut world = StorageWorld::load_metadata(&env);
                world.load_entity(&env, 1).unwrap();
                world.despawn_entity(1);
                world.flush(&env);
            }

            // Verify entity is gone
            {
                let mut world = StorageWorld::load_metadata(&env);
                assert_eq!(world.entity_count(), 0);
                let result = world.load_entity(&env, 1);
                assert_eq!(result, Err(CougrError::EntityNotFound));
            }
        });
    }

    #[test]
    fn test_incremental_update() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            // Create two entities
            {
                let mut world = StorageWorld::load_metadata(&env);
                let e1 = world.spawn_entity(&env);
                let e2 = world.spawn_entity(&env);
                world.add_component(
                    &env,
                    e1,
                    symbol_short!("pos"),
                    Bytes::from_array(&env, &[10]),
                );
                world.add_component(
                    &env,
                    e2,
                    symbol_short!("pos"),
                    Bytes::from_array(&env, &[20]),
                );
                world.flush(&env);
            }

            // Update only entity 1
            {
                let mut world = StorageWorld::load_metadata(&env);
                world.load_entity(&env, 1).unwrap();
                world.add_component(
                    &env,
                    1,
                    symbol_short!("pos"),
                    Bytes::from_array(&env, &[99]),
                );
                world.flush(&env);
            }

            // Verify entity 1 updated, entity 2 unchanged
            {
                let mut world = StorageWorld::load_metadata(&env);
                world.load_entity(&env, 1).unwrap();
                world.load_entity(&env, 2).unwrap();

                assert_eq!(
                    world.get_component(1, &symbol_short!("pos")),
                    Some(Bytes::from_array(&env, &[99]))
                );
                assert_eq!(
                    world.get_component(2, &symbol_short!("pos")),
                    Some(Bytes::from_array(&env, &[20]))
                );
            }
        });
    }

    #[test]
    fn test_version_tracking() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            assert_eq!(world.version(), 0);

            let eid = world.spawn_entity(&env);
            // spawn doesn't increment version, add_component does
            world.add_component(
                &env,
                eid,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[1]),
            );
            let v1 = world.version();
            assert!(v1 > 0);

            world.remove_component(eid, &symbol_short!("pos"));
            assert!(world.version() > v1);
        });
    }

    #[test]
    fn test_to_simple_world() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let e1 = world.spawn_entity(&env);
            let e2 = world.spawn_entity(&env);
            world.add_component(
                &env,
                e1,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[1, 2]),
            );
            world.add_component(
                &env,
                e2,
                symbol_short!("vel"),
                Bytes::from_array(&env, &[3, 4]),
            );

            let simple = world.to_simple_world(&env);
            assert!(simple.has_component(e1, &symbol_short!("pos")));
            assert!(simple.has_component(e2, &symbol_short!("vel")));
            assert!(!simple.has_component(e1, &symbol_short!("vel")));
        });
    }

    #[test]
    fn test_from_simple_world() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            // Create a SimpleWorld with some data
            let mut simple = SimpleWorld::new(&env);
            let e1 = simple.spawn_entity();
            let e2 = simple.spawn_entity();
            simple.add_component(e1, symbol_short!("pos"), Bytes::from_array(&env, &[10, 20]));
            simple.add_component(e2, symbol_short!("vel"), Bytes::from_array(&env, &[30, 40]));

            // Convert to StorageWorld
            let mut storage = StorageWorld::from_simple_world(&simple, &env);
            assert_eq!(storage.entity_count(), 2);
            assert!(storage.has_component(e1, &symbol_short!("pos")));
            assert!(storage.has_component(e2, &symbol_short!("vel")));

            // Flush and reload to verify persistence
            storage.flush(&env);

            let mut reloaded = StorageWorld::load_metadata(&env);
            reloaded.load_entity(&env, e1).unwrap();
            reloaded.load_entity(&env, e2).unwrap();
            assert_eq!(
                reloaded.get_component(e1, &symbol_short!("pos")),
                Some(Bytes::from_array(&env, &[10, 20]))
            );
            assert_eq!(
                reloaded.get_component(e2, &symbol_short!("vel")),
                Some(Bytes::from_array(&env, &[30, 40]))
            );
        });
    }

    #[test]
    fn test_load_multiple_entities() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            {
                let mut world = StorageWorld::load_metadata(&env);
                for i in 0..5u8 {
                    let eid = world.spawn_entity(&env);
                    world.add_component(
                        &env,
                        eid,
                        symbol_short!("data"),
                        Bytes::from_array(&env, &[i]),
                    );
                }
                world.flush(&env);
            }

            {
                let mut world = StorageWorld::load_metadata(&env);
                world.load_entities(&env, &[1, 2, 3, 4, 5]).unwrap();
                for i in 1..=5u32 {
                    assert!(world.has_component(i, &symbol_short!("data")));
                }
            }
        });
    }

    #[test]
    fn test_no_flush_when_clean() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            // Flush on a clean world should be a no-op
            world.flush(&env);
            assert_eq!(world.version(), 0);
        });
    }

    #[test]
    fn test_multiple_components_per_entity() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let eid = world.spawn_entity(&env);

            world.add_component(
                &env,
                eid,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[1]),
            );
            world.add_component(
                &env,
                eid,
                symbol_short!("vel"),
                Bytes::from_array(&env, &[2]),
            );
            world.add_component(
                &env,
                eid,
                symbol_short!("hp"),
                Bytes::from_array(&env, &[3]),
            );

            assert!(world.has_component(eid, &symbol_short!("pos")));
            assert!(world.has_component(eid, &symbol_short!("vel")));
            assert!(world.has_component(eid, &symbol_short!("hp")));

            // Flush, reload, verify
            world.flush(&env);

            let mut reloaded = StorageWorld::load_metadata(&env);
            reloaded.load_entity(&env, eid).unwrap();
            assert_eq!(
                reloaded.get_component(eid, &symbol_short!("pos")),
                Some(Bytes::from_array(&env, &[1]))
            );
            assert_eq!(
                reloaded.get_component(eid, &symbol_short!("vel")),
                Some(Bytes::from_array(&env, &[2]))
            );
            assert_eq!(
                reloaded.get_component(eid, &symbol_short!("hp")),
                Some(Bytes::from_array(&env, &[3]))
            );
        });
    }

    #[test]
    fn test_update_existing_component() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        env.as_contract(&contract_id, || {
            let mut world = StorageWorld::load_metadata(&env);
            let eid = world.spawn_entity(&env);

            world.add_component(
                &env,
                eid,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[10]),
            );
            // Overwrite
            world.add_component(
                &env,
                eid,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[99]),
            );

            assert_eq!(
                world.get_component(eid, &symbol_short!("pos")),
                Some(Bytes::from_array(&env, &[99]))
            );
        });
    }
}
