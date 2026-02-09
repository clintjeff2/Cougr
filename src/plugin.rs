use crate::hooks::{HookRegistry, OnAddHook, OnRemoveHook};
use crate::scheduler::SimpleScheduler;
use crate::simple_world::SimpleWorld;
use alloc::vec::Vec;
use soroban_sdk::{Env, Symbol};

/// A plugin that configures systems, hooks, and initial world state.
///
/// Plugins provide a modular way to compose game functionality.
/// Each plugin gets access to a `PluginApp` builder during `build()`.
///
/// # Example
/// ```ignore
/// struct PhysicsPlugin;
///
/// impl Plugin for PhysicsPlugin {
///     fn name(&self) -> &'static str { "physics" }
///     fn build(&self, app: &mut PluginApp) {
///         app.add_system("gravity", gravity_system);
///         app.add_system("collision", collision_system);
///     }
/// }
/// ```
pub trait Plugin {
    /// Returns the unique name of this plugin.
    fn name(&self) -> &'static str;

    /// Configure the app with this plugin's systems, hooks, and state.
    fn build(&self, app: &mut PluginApp);
}

/// Application builder that ties together `SimpleWorld`, `SimpleScheduler`,
/// and `HookRegistry` for modular game configuration.
///
/// # Example
/// ```ignore
/// let env = Env::default();
/// let mut app = PluginApp::new(&env);
/// app.add_plugin(PhysicsPlugin);
/// app.add_plugin(ScoringPlugin);
/// app.run(&env);
/// let world = app.into_world();
/// ```
pub struct PluginApp {
    world: SimpleWorld,
    scheduler: SimpleScheduler,
    hooks: HookRegistry,
    plugins_registered: Vec<&'static str>,
}

impl PluginApp {
    /// Create a new application with empty world, scheduler, and hooks.
    pub fn new(env: &Env) -> Self {
        Self {
            world: SimpleWorld::new(env),
            scheduler: SimpleScheduler::new(),
            hooks: HookRegistry::new(),
            plugins_registered: Vec::new(),
        }
    }

    /// Create a new application wrapping an existing world.
    pub fn with_world(world: SimpleWorld) -> Self {
        Self {
            world,
            scheduler: SimpleScheduler::new(),
            hooks: HookRegistry::new(),
            plugins_registered: Vec::new(),
        }
    }

    /// Register a plugin. Duplicate plugins (same name) are skipped.
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        let name = plugin.name();
        if !self.has_plugin(name) {
            self.plugins_registered.push(name);
            plugin.build(self);
        }
        self
    }

    /// Add a system to the scheduler.
    pub fn add_system(
        &mut self,
        name: &'static str,
        system: fn(&mut SimpleWorld, &Env),
    ) -> &mut Self {
        self.scheduler.add_system(name, system);
        self
    }

    /// Register an `on_add` component hook.
    pub fn add_hook_on_add(&mut self, component_type: Symbol, hook: OnAddHook) -> &mut Self {
        self.hooks.on_add(component_type, hook);
        self
    }

    /// Register an `on_remove` component hook.
    pub fn add_hook_on_remove(&mut self, component_type: Symbol, hook: OnRemoveHook) -> &mut Self {
        self.hooks.on_remove(component_type, hook);
        self
    }

    /// Access the underlying `SimpleWorld`.
    pub fn world(&self) -> &SimpleWorld {
        &self.world
    }

    /// Mutably access the underlying `SimpleWorld`.
    pub fn world_mut(&mut self) -> &mut SimpleWorld {
        &mut self.world
    }

    /// Access the scheduler.
    pub fn scheduler(&self) -> &SimpleScheduler {
        &self.scheduler
    }

    /// Access the hook registry.
    pub fn hooks(&self) -> &HookRegistry {
        &self.hooks
    }

    /// Run all registered systems in order.
    pub fn run(&mut self, env: &Env) {
        self.scheduler.run_all(&mut self.world, env);
    }

    /// Consume the app and return the inner `SimpleWorld` for persistence.
    pub fn into_world(self) -> SimpleWorld {
        self.world
    }

    /// Returns the number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins_registered.len()
    }

    /// Check if a plugin with the given name has been registered.
    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins_registered.contains(&name)
    }

    /// Returns the number of registered systems.
    pub fn system_count(&self) -> usize {
        self.scheduler.system_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, Bytes, Env};

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            "test_plugin"
        }

        fn build(&self, app: &mut PluginApp) {
            app.add_system("test_system", test_system_fn);
        }
    }

    struct HookPlugin;

    impl Plugin for HookPlugin {
        fn name(&self) -> &'static str {
            "hook_plugin"
        }

        fn build(&self, app: &mut PluginApp) {
            app.add_hook_on_add(symbol_short!("pos"), noop_add_hook);
        }
    }

    fn test_system_fn(world: &mut SimpleWorld, env: &Env) {
        let e = world.spawn_entity();
        let data = Bytes::from_array(env, &[0xFF]);
        world.add_component(e, symbol_short!("marker"), data);
    }

    fn noop_add_hook(
        _entity_id: crate::simple_world::EntityId,
        _component_type: &Symbol,
        _data: &Bytes,
    ) {
    }

    #[test]
    fn test_plugin_app_new() {
        let env = Env::default();
        let app = PluginApp::new(&env);
        assert_eq!(app.plugin_count(), 0);
        assert_eq!(app.system_count(), 0);
    }

    #[test]
    fn test_add_plugin() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(TestPlugin);

        assert_eq!(app.plugin_count(), 1);
        assert!(app.has_plugin("test_plugin"));
        assert_eq!(app.system_count(), 1);
    }

    #[test]
    fn test_duplicate_plugin_skipped() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(TestPlugin);
        app.add_plugin(TestPlugin); // duplicate

        assert_eq!(app.plugin_count(), 1);
        assert_eq!(app.system_count(), 1); // should not double
    }

    #[test]
    fn test_plugin_configures_hooks() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(HookPlugin);

        assert_eq!(app.plugin_count(), 1);
        assert_eq!(app.hooks().add_hook_count(), 1);
    }

    #[test]
    fn test_run_executes_systems() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(TestPlugin);
        app.run(&env);

        // test_system_fn spawns entity 1 with "marker"
        assert!(app.world().has_component(1, &symbol_short!("marker")));
    }

    #[test]
    fn test_into_world() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(TestPlugin);
        app.run(&env);

        let world = app.into_world();
        assert!(world.has_component(1, &symbol_short!("marker")));
    }

    #[test]
    fn test_with_world() {
        let env = Env::default();
        let mut world = SimpleWorld::new(&env);
        let e1 = world.spawn_entity();
        let data = Bytes::from_array(&env, &[1]);
        world.add_component(e1, symbol_short!("pre"), data);

        let app = PluginApp::with_world(world);
        assert!(app.world().has_component(e1, &symbol_short!("pre")));
    }

    #[test]
    fn test_add_system_directly() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_system("direct", test_system_fn);

        assert_eq!(app.system_count(), 1);
        assert_eq!(app.plugin_count(), 0); // no plugin used
    }

    #[test]
    fn test_multiple_plugins() {
        let env = Env::default();
        let mut app = PluginApp::new(&env);
        app.add_plugin(TestPlugin);
        app.add_plugin(HookPlugin);

        assert_eq!(app.plugin_count(), 2);
        assert!(app.has_plugin("test_plugin"));
        assert!(app.has_plugin("hook_plugin"));
    }
}
