//! Archetype-aware query utilities.
//!
//! Provides cached archetype queries with version-based invalidation,
//! and queries with both required and excluded component types.

use super::world::ArchetypeWorld;
use crate::simple_world::EntityId;
use soroban_sdk::{Env, Symbol, Vec};

/// Cached archetype query with version-based invalidation.
///
/// Caches query results and re-executes only when the world version changes.
/// Follows the same pattern as `SimpleQueryCache` from `src/query.rs`.
pub struct ArchetypeQueryCache {
    required: alloc::vec::Vec<Symbol>,
    cached_version: u64,
    cached_results: Option<Vec<EntityId>>,
}

impl ArchetypeQueryCache {
    /// Create a new cache for the given required components.
    pub fn new(required: alloc::vec::Vec<Symbol>) -> Self {
        Self {
            required,
            cached_version: u64::MAX,
            cached_results: None,
        }
    }

    /// Execute the query, using cached results if the world hasn't changed.
    pub fn execute(&mut self, world: &ArchetypeWorld, env: &Env) -> Vec<EntityId> {
        if self.cached_version == world.version() {
            if let Some(ref results) = self.cached_results {
                return results.clone();
            }
        }

        let results = world.query(&self.required, env);
        self.cached_version = world.version();
        self.cached_results = Some(results.clone());
        results
    }

    /// Force cache invalidation.
    pub fn invalidate(&mut self) {
        self.cached_version = u64::MAX;
        self.cached_results = None;
    }
}

/// Query with both required and excluded component types.
///
/// Returns entities that have ALL required components and NONE of the
/// excluded components.
pub fn archetype_query(
    world: &ArchetypeWorld,
    required: &[Symbol],
    excluded: &[Symbol],
    env: &Env,
) -> Vec<EntityId> {
    let candidates = world.query(required, env);

    if excluded.is_empty() {
        return candidates;
    }

    let mut results = Vec::new(env);
    for i in 0..candidates.len() {
        if let Some(eid) = candidates.get(i) {
            let mut exclude = false;
            for ex in excluded {
                if world.has_component(eid, ex) {
                    exclude = true;
                    break;
                }
            }
            if !exclude {
                results.push_back(eid);
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, Bytes, Env};

    #[test]
    fn test_archetype_query_cache() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        let e1 = world.spawn_entity();
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );

        let mut cache = ArchetypeQueryCache::new(alloc::vec![symbol_short!("pos")]);
        let results = cache.execute(&world, &env);
        assert_eq!(results.len(), 1);

        // Cache hit (same version)
        let results2 = cache.execute(&world, &env);
        assert_eq!(results2.len(), 1);

        // Add another entity, cache miss
        let e2 = world.spawn_entity();
        world.add_component(
            e2,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[2]),
            &env,
        );
        let results3 = cache.execute(&world, &env);
        assert_eq!(results3.len(), 2);
    }

    #[test]
    fn test_archetype_query_cache_invalidate() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        let e1 = world.spawn_entity();
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );

        let mut cache = ArchetypeQueryCache::new(alloc::vec![symbol_short!("pos")]);
        cache.execute(&world, &env);
        cache.invalidate();

        // After invalidation, should re-execute
        let results = cache.execute(&world, &env);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_archetype_query_with_exclusions() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        let e1 = world.spawn_entity();
        let e2 = world.spawn_entity();

        // e1: pos
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );
        // e2: pos + dead
        world.add_component(
            e2,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[2]),
            &env,
        );
        world.add_component(
            e2,
            symbol_short!("dead"),
            Bytes::from_array(&env, &[1]),
            &env,
        );

        // Query: has pos, not dead
        let results = archetype_query(
            &world,
            &[symbol_short!("pos")],
            &[symbol_short!("dead")],
            &env,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0), Some(e1));
    }

    #[test]
    fn test_archetype_query_no_exclusions() {
        let env = Env::default();
        let mut world = ArchetypeWorld::new(&env);

        let e1 = world.spawn_entity();
        world.add_component(
            e1,
            symbol_short!("pos"),
            Bytes::from_array(&env, &[1]),
            &env,
        );

        let results = archetype_query(&world, &[symbol_short!("pos")], &[], &env);
        assert_eq!(results.len(), 1);
    }
}
