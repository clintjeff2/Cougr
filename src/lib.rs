#![no_std]
#![allow(unsafe_code)]

extern crate alloc;

use soroban_sdk::{symbol_short, Bytes, Symbol, Vec};

// Global allocator for WASM
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Macros must be declared before modules that use them
#[macro_use]
pub mod macros;

// Core ECS types adapted for Soroban
pub mod accounts;
pub mod change_tracker;
pub mod commands;
pub mod component;
pub mod components;
pub mod entity;
pub mod error;
pub mod event;
pub mod hooks;
pub mod observers;
pub mod plugin;
pub mod query;
pub mod resource;
pub mod scheduler;
pub mod simple_world;
pub mod storage;
pub mod system;
pub mod systems;
pub mod world;
pub mod zk;

// Re-export core types
pub use change_tracker::{ChangeTracker, TrackedWorld};
pub use commands::CommandQueue;
pub use component::{Component, ComponentId, ComponentStorage, ComponentTrait};
pub use components::Position;
pub use entity::{Entity, EntityId};
pub use error::{CougrError, CougrResult};
pub use event::{Event, EventReader, EventWriter};
pub use hooks::{HookRegistry, HookedWorld};
pub use observers::{ObservedWorld, ObserverRegistry};
pub use plugin::{Plugin, PluginApp};
pub use query::{Query, QueryState, SimpleQueryCache};
pub use resource::Resource;
pub use scheduler::{SimpleScheduler, SystemScheduler};
pub use simple_world::SimpleWorld;
pub use storage::{SparseStorage, Storage, TableStorage};
pub use system::{IntoSystem, System, SystemParam};
pub use systems::MovementSystem;
pub use world::World;

// Library functions for ECS operations
pub fn create_world() -> World {
    World::new()
}

pub fn spawn_entity(world: &mut World, components: Vec<Component>) -> EntityId {
    let entity = world.spawn(components);
    entity.id()
}

pub fn add_component(world: &mut World, entity_id: EntityId, component: Component) -> bool {
    world.add_component_to_entity(entity_id, component);
    true
}

pub fn remove_component(world: &mut World, entity_id: EntityId, component_type: Symbol) -> bool {
    world.remove_component_from_entity(entity_id, &component_type)
}

pub fn get_component(
    world: &World,
    entity_id: EntityId,
    component_type: Symbol,
) -> Option<Component> {
    world.get_component(entity_id, &component_type)
}

pub fn query_entities(
    world: &World,
    component_types: Vec<Symbol>,
    env: &soroban_sdk::Env,
) -> Vec<EntityId> {
    // Since we can't easily convert Vec<Symbol> to &[Symbol] in Soroban,
    // we'll need to restructure this. For now, return empty result.
    Vec::new(env)
}

// Predule for common types
pub mod prelude {
    pub use super::{
        component::{Component, ComponentId, ComponentStorage},
        entity::{Entity, EntityId},
        event::{Event, EventReader, EventWriter},
        query::{Query, QueryState},
        resource::Resource,
        storage::{SparseStorage, Storage, TableStorage},
        system::{IntoSystem, System, SystemParam},
        world::World,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_world_creation() {
        let _env = Env::default();
        let world = World::new();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_entity_spawn() {
        let _env = Env::default();
        let mut world = World::new();
        let entity = world.spawn_empty();
        assert_eq!(world.entity_count(), 1);
    }
}
