//! Archetype-based world implementation.
//!
//! Groups entities by component composition for efficient queries.
//! Systems that need specific component combinations only scan
//! matching archetypes, skipping unrelated entities entirely.
//!
//! # Trade-offs
//!
//! - **Fast queries**: O(matching_archetypes) instead of O(all_entities)
//! - **Costly add/remove**: Adding or removing a component migrates the
//!   entity between archetypes (data copy)
//! - Best for: 50+ entities, multi-component queries, stable compositions

use super::archetype::{Archetype, ArchetypeId};
use crate::simple_world::{EntityId, SimpleWorld};
use soroban_sdk::{contracttype, Bytes, Env, Map, Symbol, Vec};

/// World that groups entities by archetype for efficient queries.
///
/// Each unique combination of component types forms an archetype.
/// Entities with the same component set share an archetype, enabling
/// batch iteration without per-entity type checks.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ArchetypeWorld {
    /// Next entity ID to assign.
    pub next_entity_id: EntityId,
    /// Next archetype ID to assign.
    pub next_archetype_id: ArchetypeId,
    /// All archetypes, keyed by ID.
    pub archetypes: Map<ArchetypeId, Archetype>,
    /// Maps sorted component type lists to archetype IDs.
    pub archetype_index: Map<Vec<Symbol>, ArchetypeId>,
    /// Maps each entity to its current archetype.
    pub entity_archetype: Map<EntityId, ArchetypeId>,
    /// Version counter for cache invalidation.
    pub version: u64,
}

impl ArchetypeWorld {
    /// Create a new empty archetype world.
    pub fn new(env: &Env) -> Self {
        Self {
            next_entity_id: 1,
            next_archetype_id: 0,
            archetypes: Map::new(env),
            archetype_index: Map::new(env),
            entity_archetype: Map::new(env),
            version: 0,
        }
    }

    /// Spawn a new entity with no components.
    pub fn spawn_entity(&mut self) -> EntityId {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        id
    }

    /// Add a component to an entity, migrating to a new archetype if needed.
    pub fn add_component(
        &mut self,
        entity_id: EntityId,
        component_type: Symbol,
        data: Bytes,
        env: &Env,
    ) {
        self.version += 1;

        if let Some(arch_id) = self.entity_archetype.get(entity_id) {
            // Entity already has an archetype — need to migrate
            if let Some(mut arch) = self.archetypes.get(arch_id) {
                // Check if already has this component type
                if arch.has_component_type(&component_type) {
                    // Just update the data in place
                    arch.set_component(entity_id, component_type, data);
                    self.archetypes.set(arch_id, arch);
                    return;
                }

                // Build new component type list
                let new_types = self.build_new_types(&arch.component_types, &component_type, env);

                // Extract entity data from old archetype
                let mut extracted = arch.remove_entity(entity_id, env);
                extracted.set(component_type, data);
                self.archetypes.set(arch_id, arch);

                // Find or create target archetype
                let target_id = self.get_or_create_archetype(new_types, env);
                if let Some(mut target) = self.archetypes.get(target_id) {
                    target.add_entity(entity_id);
                    // Copy all extracted data
                    for key in extracted.keys().iter() {
                        if let Some(d) = extracted.get(key.clone()) {
                            target.set_component(entity_id, key, d);
                        }
                    }
                    self.archetypes.set(target_id, target);
                }
                self.entity_archetype.set(entity_id, target_id);
            }
        } else {
            // Entity has no archetype yet — create/find a single-component archetype
            let types = canonicalize_single(env, &component_type);
            let arch_id = self.get_or_create_archetype(types, env);
            if let Some(mut arch) = self.archetypes.get(arch_id) {
                arch.add_entity(entity_id);
                arch.set_component(entity_id, component_type, data);
                self.archetypes.set(arch_id, arch);
            }
            self.entity_archetype.set(entity_id, arch_id);
        }
    }

    /// Remove a component from an entity, migrating to a smaller archetype.
    pub fn remove_component(
        &mut self,
        entity_id: EntityId,
        component_type: &Symbol,
        env: &Env,
    ) -> bool {
        let arch_id = match self.entity_archetype.get(entity_id) {
            Some(id) => id,
            None => return false,
        };

        let mut arch = match self.archetypes.get(arch_id) {
            Some(a) => a,
            None => return false,
        };

        if !arch.has_component_type(component_type) {
            return false;
        }

        self.version += 1;

        // Build new type list without this component
        let mut new_type_list: alloc::vec::Vec<Symbol> = alloc::vec::Vec::new();
        for i in 0..arch.component_types.len() {
            if let Some(t) = arch.component_types.get(i) {
                if &t != component_type {
                    new_type_list.push(t);
                }
            }
        }

        // Extract entity data from old archetype
        let mut extracted = arch.remove_entity(entity_id, env);
        extracted.remove(component_type.clone());
        self.archetypes.set(arch_id, arch);

        if new_type_list.is_empty() {
            // No components left, entity has no archetype
            self.entity_archetype.remove(entity_id);
        } else {
            // Find or create target archetype
            let new_types = vec_from_slice(env, &new_type_list);
            let target_id = self.get_or_create_archetype(new_types, env);
            if let Some(mut target) = self.archetypes.get(target_id) {
                target.add_entity(entity_id);
                for key in extracted.keys().iter() {
                    if let Some(d) = extracted.get(key.clone()) {
                        target.set_component(entity_id, key, d);
                    }
                }
                self.archetypes.set(target_id, target);
            }
            self.entity_archetype.set(entity_id, target_id);
        }

        true
    }

    /// Get a component's data for an entity.
    pub fn get_component(&self, entity_id: EntityId, component_type: &Symbol) -> Option<Bytes> {
        let arch_id = self.entity_archetype.get(entity_id)?;
        let arch = self.archetypes.get(arch_id)?;
        arch.get_component(entity_id, component_type)
    }

    /// Check if an entity has a specific component.
    pub fn has_component(&self, entity_id: EntityId, component_type: &Symbol) -> bool {
        if let Some(arch_id) = self.entity_archetype.get(entity_id) {
            if let Some(arch) = self.archetypes.get(arch_id) {
                return arch.has_component_type(component_type);
            }
        }
        false
    }

    /// Despawn an entity, removing it from its archetype.
    pub fn despawn_entity(&mut self, entity_id: EntityId, env: &Env) {
        if let Some(arch_id) = self.entity_archetype.get(entity_id) {
            if let Some(mut arch) = self.archetypes.get(arch_id) {
                arch.remove_entity(entity_id, env);
                self.archetypes.set(arch_id, arch);
            }
            self.entity_archetype.remove(entity_id);
        }
        self.version += 1;
    }

    /// Query for entities that have all required components.
    ///
    /// Scans only matching archetypes, not all entities.
    pub fn query(&self, required_components: &[Symbol], env: &Env) -> Vec<EntityId> {
        let mut results = Vec::new(env);

        for key in self.archetypes.keys().iter() {
            if let Some(arch) = self.archetypes.get(key) {
                if arch.matches(required_components) {
                    for i in 0..arch.entities.len() {
                        if let Some(eid) = arch.entities.get(i) {
                            results.push_back(eid);
                        }
                    }
                }
            }
        }

        results
    }

    /// Returns the current version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Convert to a SimpleWorld (for interoperability).
    pub fn to_simple_world(&self, env: &Env) -> SimpleWorld {
        let mut world = SimpleWorld::new(env);
        world.next_entity_id = self.next_entity_id;

        for arch_key in self.archetypes.keys().iter() {
            if let Some(arch) = self.archetypes.get(arch_key) {
                for i in 0..arch.entities.len() {
                    if let Some(eid) = arch.entities.get(i) {
                        for j in 0..arch.component_types.len() {
                            if let Some(ct) = arch.component_types.get(j) {
                                if let Some(data) = arch.get_component(eid, &ct) {
                                    world.add_component(eid, ct, data);
                                }
                            }
                        }
                    }
                }
            }
        }

        world.version = self.version;
        world
    }

    /// Create an ArchetypeWorld from a SimpleWorld (for migration).
    pub fn from_simple_world(simple: &SimpleWorld, env: &Env) -> Self {
        let mut world = Self::new(env);
        world.next_entity_id = simple.next_entity_id;

        for eid in simple.entity_components.keys().iter() {
            if let Some(types) = simple.entity_components.get(eid) {
                // Add each component
                for i in 0..types.len() {
                    if let Some(ct) = types.get(i) {
                        if let Some(data) = simple.get_component(eid, &ct) {
                            world.add_component(eid, ct, data, env);
                        }
                    }
                }
            }
        }

        world.version = simple.version;
        world
    }

    /// Find or create an archetype for the given component types.
    fn get_or_create_archetype(&mut self, component_types: Vec<Symbol>, env: &Env) -> ArchetypeId {
        if let Some(existing) = self.archetype_index.get(component_types.clone()) {
            return existing;
        }

        let id = self.next_archetype_id;
        self.next_archetype_id += 1;

        let arch = Archetype::new(env, id, component_types.clone());
        self.archetypes.set(id, arch);
        self.archetype_index.set(component_types, id);
        id
    }

    /// Build a new sorted component type list by adding one type.
    fn build_new_types(&self, existing: &Vec<Symbol>, new_type: &Symbol, env: &Env) -> Vec<Symbol> {
        let mut types: alloc::vec::Vec<Symbol> = alloc::vec::Vec::new();
        for i in 0..existing.len() {
            if let Some(t) = existing.get(i) {
                types.push(t);
            }
        }
        types.push(new_type.clone());
        // Sort to canonicalize using Symbol's PartialOrd
        types.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        vec_from_slice(env, &types)
    }
}

/// Create a canonical single-component type list.
fn canonicalize_single(env: &Env, component_type: &Symbol) -> Vec<Symbol> {
    let mut v = Vec::new(env);
    v.push_back(component_type.clone());
    v
}

/// Convert a `alloc::vec::Vec<Symbol>` to a `soroban_sdk::Vec<Symbol>`.
fn vec_from_slice(env: &Env, items: &[Symbol]) -> Vec<Symbol> {
    let mut v = Vec::new(env);
    for item in items {
        v.push_back(item.clone());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, Env};

    #[test]
    fn test_new_world() {
        let env = Env::default();
        let world = ArchetypeWorld::new(&env);
        assert_eq!(world.next_entity_id, 1);
        assert_eq!(world.version(), 0);
    }

    #[test]
    fn test_spawn_entity() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();
        let e2 = world.spawn_entity();
        assert_eq!(e1, 1);
        assert_eq!(e2, 2);
    }

    #[test]
    fn test_add_single_component() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1, 2]),
            &env,
        );

        assert!(world.has_component(e1, &symbol_short!("pos")));
        assert_eq!(
            world.get_component(e1, &symbol_short!("pos")),
            Some(Bytes::from_array(&env, &[1, 2]))
        );
    }

    #[test]
    fn test_add_multiple_components() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );

        assert!(world.has_component(e1, &symbol_short!("pos")));
        assert!(world.has_component(e1, &symbol_short!("vel")));
    }

    #[test]
    fn test_entities_share_archetype() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();
        let e2 = world.spawn_entity();

        // Both get pos then vel — same archetype
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );
        world.add_component(
            e2,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[3]),
            &env,
        );
        world.add_component(
            e2,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[4]),
            &env,
        );

        let a1 = world.entity_archetype.get(e1).unwrap();
        let a2 = world.entity_archetype.get(e2).unwrap();
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_query() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        let e1 = world.spawn_entity();
        let e2 = world.spawn_entity();
        let e3 = world.spawn_entity();

        // e1: pos + vel
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );
        // e2: pos only
        world.add_component(
            e2,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[3]),
            &env,
        );
        // e3: vel only
        world.add_component(
            e3,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[4]),
            &env,
        );

        // Query pos: e1, e2
        let with_pos = world.query(&[symbol_short!("pos")], &env);
        assert_eq!(with_pos.len(), 2);

        // Query vel: e1, e3
        let with_vel = world.query(&[symbol_short!("vel")], &env);
        assert_eq!(with_vel.len(), 2);

        // Query pos + vel: only e1
        let with_both = world.query(&[symbol_short!("pos"), symbol_short!("vel")], &env);
        assert_eq!(with_both.len(), 1);
        assert_eq!(with_both.get(0), Some(e1));
    }

    #[test]
    fn test_remove_component() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );

        assert!(world.remove_component(e1, &symbol_short!("vel"), &env));
        assert!(!world.has_component(e1, &symbol_short!("vel")));
        assert!(world.has_component(e1, &symbol_short!("pos")));

        // Data should be preserved for remaining component
        assert_eq!(
            world.get_component(e1, &symbol_short!("pos")),
            Some(Bytes::from_array(&env, &[1]))
        );
    }

    #[test]
    fn test_remove_last_component() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        assert!(world.remove_component(e1, &symbol_short!("pos"), &env));

        // Entity has no archetype now
        assert!(world.entity_archetype.get(e1).is_none());
        assert!(!world.has_component(e1, &symbol_short!("pos")));
    }

    #[test]
    fn test_remove_nonexistent_component() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        assert!(!world.remove_component(e1, &symbol_short!("pos"), &env));
    }

    #[test]
    fn test_despawn_entity() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );

        world.despawn_entity(e1, &env);
        assert!(!world.has_component(e1, &symbol_short!("pos")));
        assert!(!world.has_component(e1, &symbol_short!("vel")));
        assert!(world.entity_archetype.get(e1).is_none());
    }

    #[test]
    fn test_update_existing_component() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[99]),
            &env,
        );

        assert_eq!(
            world.get_component(e1, &symbol_short!("pos")),
            Some(Bytes::from_array(&env, &[99]))
        );
    }

    #[test]
    fn test_version_tracking() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        assert_eq!(world.version(), 0);

        let e1 = world.spawn_entity();
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        let v1 = world.version();
        assert!(v1 > 0);

        world.remove_component(e1, &symbol_short!("pos"), &env);
        assert!(world.version() > v1);
    }

    #[test]
    fn test_to_simple_world() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);
        let e1 = world.spawn_entity();
        let e2 = world.spawn_entity();

        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        world.add_component(
            e2,
            symbol_short!("vel"),
            Bytes::from_array(&env, &[2]),
            &env,
        );

        let simple = world.to_simple_world(&env);
        assert!(simple.has_component(e1, &symbol_short!("pos")));
        assert!(simple.has_component(e2, &symbol_short!("vel")));
        assert!(!simple.has_component(e1, &symbol_short!("vel")));
    }

    #[test]
    fn test_from_simple_world() {
        let env = Env::default();
        let mut simple = SimpleWorld::new(&env);
        let e1 = simple.spawn_entity();
        let e2 = simple.spawn_entity();

        simple.add_component(e1, symbol_short!("pos"), Bytes::from_array(&env, &[10]));
        simple.add_component(e1, symbol_short!("vel"), Bytes::from_array(&env, &[20]));
        simple.add_component(e2, symbol_short!("pos"), Bytes::from_array(&env, &[30]));

        let arch_world = ArchetypeWorld::from_simple_world(&simple, &env);
        assert!(arch_world.has_component(e1, &symbol_short!("pos")));
        assert!(arch_world.has_component(e1, &symbol_short!("vel")));
        assert!(arch_world.has_component(e2, &symbol_short!("pos")));
        assert!(!arch_world.has_component(e2, &symbol_short!("vel")));

        // Query should work
        let with_both = arch_world.query(&[symbol_short!("pos"), symbol_short!("vel")], &env);
        assert_eq!(with_both.len(), 1);
    }

    #[test]
    fn test_many_entities_same_archetype() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        for _ in 0..20 {
            let eid = world.spawn_entity();
            world.add_component(
                eid,
                symbol_short!("pos"),
                Bytes::from_array(&env, &[1]),
                &env,
            );
            world.add_component(
                eid,
                symbol_short!("vel"),
                Bytes::from_array(&env, &[2]),
                &env,
            );
        }

        let results = world.query(&[symbol_short!("pos"), symbol_short!("vel")], &env);
        assert_eq!(results.len(), 20);
    }
}
