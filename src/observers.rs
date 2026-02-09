use crate::simple_world::{EntityId, SimpleWorld};
use alloc::vec::Vec;
use soroban_sdk::{Bytes, Env, Symbol};

/// The kind of component event that triggered an observer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentEventKind {
    Added,
    Removed,
}

/// A component event passed to observer functions.
#[derive(Debug, Clone)]
pub struct ComponentEvent {
    pub entity_id: EntityId,
    pub component_type: Symbol,
    pub kind: ComponentEventKind,
}

/// Observer function signature.
///
/// Receives the event details plus read-only access to the world
/// (after the mutation has been applied).
pub type ObserverFn = fn(event: &ComponentEvent, world: &SimpleWorld, env: &Env);

/// Registry of component observers that fire on component events.
///
/// Observers are runtime-only (not persisted) and must be re-registered
/// each contract invocation. They provide reactive patterns where systems
/// trigger immediately on component changes rather than polling.
///
/// # Example
/// ```ignore
/// fn on_position_added(event: &ComponentEvent, world: &SimpleWorld, env: &Env) {
///     // React to a position component being added
/// }
///
/// let mut registry = ObserverRegistry::new();
/// registry.on_add(symbol_short!("pos"), on_position_added);
/// ```
pub struct ObserverRegistry {
    observers: Vec<(Symbol, ComponentEventKind, ObserverFn)>,
}

impl ObserverRegistry {
    /// Create an empty observer registry.
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }

    /// Register an observer that fires when a component of the given type is added.
    pub fn on_add(&mut self, component_type: Symbol, observer: ObserverFn) {
        self.observers
            .push((component_type, ComponentEventKind::Added, observer));
    }

    /// Register an observer that fires when a component of the given type is removed.
    pub fn on_remove(&mut self, component_type: Symbol, observer: ObserverFn) {
        self.observers
            .push((component_type, ComponentEventKind::Removed, observer));
    }

    /// Fire all matching observers for the given event.
    pub fn fire(&self, event: &ComponentEvent, world: &SimpleWorld, env: &Env) {
        for (ctype, kind, observer) in &self.observers {
            if ctype == &event.component_type && kind == &event.kind {
                observer(event, world, env);
            }
        }
    }

    /// Returns the total number of registered observers.
    pub fn observer_count(&self) -> usize {
        self.observers.len()
    }
}

impl Default for ObserverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper around `SimpleWorld` that fires observers on component mutations.
///
/// Observers receive read-only access to the world **after** the mutation completes,
/// allowing them to see the updated state.
///
/// # Example
/// ```ignore
/// let env = Env::default();
/// let world = SimpleWorld::new(&env);
/// let mut observed = ObservedWorld::new(world);
/// observed.observers_mut().on_add(symbol_short!("pos"), my_observer);
/// observed.add_component(entity_id, symbol_short!("pos"), data, &env);
/// ```
pub struct ObservedWorld {
    world: SimpleWorld,
    observers: ObserverRegistry,
}

impl ObservedWorld {
    /// Wrap a `SimpleWorld` with an empty observer registry.
    pub fn new(world: SimpleWorld) -> Self {
        Self {
            world,
            observers: ObserverRegistry::new(),
        }
    }

    /// Wrap a `SimpleWorld` with a pre-configured observer registry.
    pub fn with_observers(world: SimpleWorld, observers: ObserverRegistry) -> Self {
        Self { world, observers }
    }

    /// Access the underlying `SimpleWorld`.
    pub fn world(&self) -> &SimpleWorld {
        &self.world
    }

    /// Mutably access the underlying `SimpleWorld`.
    pub fn world_mut(&mut self) -> &mut SimpleWorld {
        &mut self.world
    }

    /// Access the observer registry.
    pub fn observers(&self) -> &ObserverRegistry {
        &self.observers
    }

    /// Mutably access the observer registry.
    pub fn observers_mut(&mut self) -> &mut ObserverRegistry {
        &mut self.observers
    }

    /// Consume the wrapper and return the inner `SimpleWorld`.
    pub fn into_inner(self) -> SimpleWorld {
        self.world
    }

    /// Spawn a new entity (delegates to `SimpleWorld`).
    pub fn spawn_entity(&mut self) -> EntityId {
        self.world.spawn_entity()
    }

    /// Add a component, firing `on_add` observers after insertion.
    pub fn add_component(
        &mut self,
        entity_id: EntityId,
        component_type: Symbol,
        data: Bytes,
        env: &Env,
    ) {
        self.world
            .add_component(entity_id, component_type.clone(), data);
        let event = ComponentEvent {
            entity_id,
            component_type,
            kind: ComponentEventKind::Added,
        };
        self.observers.fire(&event, &self.world, env);
    }

    /// Remove a component, firing `on_remove` observers after removal.
    pub fn remove_component(
        &mut self,
        entity_id: EntityId,
        component_type: &Symbol,
        env: &Env,
    ) -> bool {
        let removed = self.world.remove_component(entity_id, component_type);
        if removed {
            let event = ComponentEvent {
                entity_id,
                component_type: component_type.clone(),
                kind: ComponentEventKind::Removed,
            };
            self.observers.fire(&event, &self.world, env);
        }
        removed
    }

    /// Get a component (delegates to `SimpleWorld`).
    pub fn get_component(&self, entity_id: EntityId, component_type: &Symbol) -> Option<Bytes> {
        self.world.get_component(entity_id, component_type)
    }

    /// Check if an entity has a component (delegates to `SimpleWorld`).
    pub fn has_component(&self, entity_id: EntityId, component_type: &Symbol) -> bool {
        self.world.has_component(entity_id, component_type)
    }

    /// Despawn an entity, firing `on_remove` observers for each component.
    pub fn despawn_entity(&mut self, entity_id: EntityId, env: &Env) {
        if let Some(types) = self.world.entity_components.get(entity_id) {
            for i in 0..types.len() {
                if let Some(t) = types.get(i) {
                    let event = ComponentEvent {
                        entity_id,
                        component_type: t,
                        kind: ComponentEventKind::Removed,
                    };
                    self.observers.fire(&event, &self.world, env);
                }
            }
        }
        self.world.despawn_entity(entity_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};
    use soroban_sdk::symbol_short;

    static ADD_COUNT: AtomicU32 = AtomicU32::new(0);
    static REMOVE_COUNT: AtomicU32 = AtomicU32::new(0);

    fn counting_add_observer(_event: &ComponentEvent, _world: &SimpleWorld, _env: &Env) {
        ADD_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    fn counting_remove_observer(_event: &ComponentEvent, _world: &SimpleWorld, _env: &Env) {
        REMOVE_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    fn noop_observer(_event: &ComponentEvent, _world: &SimpleWorld, _env: &Env) {}

    #[test]
    fn test_registry_new() {
        let registry = ObserverRegistry::new();
        assert_eq!(registry.observer_count(), 0);
    }

    #[test]
    fn test_registry_register() {
        let env = Env::default();
        let mut registry = ObserverRegistry::new();
        registry.on_add(symbol_short!("pos"), noop_observer);
        registry.on_remove(symbol_short!("pos"), noop_observer);
        assert_eq!(registry.observer_count(), 2);
    }

    #[test]
    fn test_observer_fires_on_add() {
        let env = Env::default();
        ADD_COUNT.store(0, Ordering::Relaxed);

        let world = SimpleWorld::new(&env);
        let mut observed = ObservedWorld::new(world);
        observed
            .observers_mut()
            .on_add(symbol_short!("pos"), counting_add_observer);

        let e1 = observed.spawn_entity();
        let data = Bytes::from_array(&env, &[1, 2, 3]);
        observed.add_component(e1, symbol_short!("pos"), data, &env);

        assert_eq!(ADD_COUNT.load(Ordering::Relaxed), 1);
        assert!(observed.has_component(e1, &symbol_short!("pos")));
    }

    #[test]
    fn test_observer_fires_on_remove() {
        let env = Env::default();
        REMOVE_COUNT.store(0, Ordering::Relaxed);

        let world = SimpleWorld::new(&env);
        let mut observed = ObservedWorld::new(world);
        observed
            .observers_mut()
            .on_remove(symbol_short!("pos"), counting_remove_observer);

        let e1 = observed.spawn_entity();
        let data = Bytes::from_array(&env, &[1]);
        observed.add_component(e1, symbol_short!("pos"), data, &env);

        assert!(observed.remove_component(e1, &symbol_short!("pos"), &env));
        assert_eq!(REMOVE_COUNT.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_observer_not_fired_for_wrong_type() {
        let env = Env::default();
        ADD_COUNT.store(0, Ordering::Relaxed);

        let world = SimpleWorld::new(&env);
        let mut observed = ObservedWorld::new(world);
        observed
            .observers_mut()
            .on_add(symbol_short!("vel"), counting_add_observer);

        let e1 = observed.spawn_entity();
        let data = Bytes::from_array(&env, &[1]);
        // Add "pos" but observer is for "vel"
        observed.add_component(e1, symbol_short!("pos"), data, &env);

        assert_eq!(ADD_COUNT.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_observed_world_despawn() {
        let env = Env::default();
        let before = REMOVE_COUNT.load(Ordering::Relaxed);

        let world = SimpleWorld::new(&env);
        let mut observed = ObservedWorld::new(world);
        observed
            .observers_mut()
            .on_remove(symbol_short!("a"), counting_remove_observer);
        observed
            .observers_mut()
            .on_remove(symbol_short!("b"), counting_remove_observer);

        let e1 = observed.spawn_entity();
        let data = Bytes::from_array(&env, &[1]);
        observed.add_component(e1, symbol_short!("a"), data.clone(), &env);
        observed.add_component(e1, symbol_short!("b"), data, &env);

        observed.despawn_entity(e1, &env);
        // Both components should trigger remove observers
        let after = REMOVE_COUNT.load(Ordering::Relaxed);
        assert_eq!(after - before, 2);
    }

    #[test]
    fn test_observed_world_into_inner() {
        let env = Env::default();
        let world = SimpleWorld::new(&env);
        let mut observed = ObservedWorld::new(world);

        let e1 = observed.spawn_entity();
        let data = Bytes::from_array(&env, &[1]);
        observed.add_component(e1, symbol_short!("test"), data, &env);

        let inner = observed.into_inner();
        assert!(inner.has_component(e1, &symbol_short!("test")));
    }

    #[test]
    fn test_with_observers_constructor() {
        let env = Env::default();
        let world = SimpleWorld::new(&env);

        let mut observers = ObserverRegistry::new();
        observers.on_add(symbol_short!("pos"), noop_observer);

        let observed = ObservedWorld::with_observers(world, observers);
        assert_eq!(observed.observers().observer_count(), 1);
    }
}
