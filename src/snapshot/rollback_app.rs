//! Extension trait that provides ergonomic methods for registering rollback plugins.
//!
//! [`RollbackApp`] is the primary API for users who want to opt specific
//! [`Component`] or [`Resource`] types into rollback and/or checksum tracking.
//! It is implemented for [`App`] and delegates to the low-level snapshot plugins.
//!
//! # Example
//! ```rust,ignore
//! app.rollback_component_with_clone::<Transform>();
//! app.checksum_component_with_hash::<Health>();
//! ```

use crate::snapshot::{
    CloneStrategy, ComponentChecksumPlugin, ComponentMapEntitiesPlugin, ComponentSnapshotPlugin,
    ResourceChecksumPlugin, ResourceSnapshotPlugin,
};
use bevy::{
    ecs::{
        component::{Immutable, Mutable},
        entity::MapEntities,
    },
    prelude::*,
};
use std::hash::Hash;

use super::{
    CloneEqStrategy, CopyEqStrategy, CopyStrategy, ImmutableComponentSnapshotPlugin,
    ReflectStrategy, ResourceMapEntitiesPlugin, RollbackEntitiesPlugin, RollbackEq,
    RollbackRegistration, SkipAutoRegistration,
};

/// Extension trait to ergonomically add rollback plugins to Bevy Apps
pub trait RollbackApp {
    /// Registers a component type for saving and loading from the world. This
    /// uses [`Copy`] based snapshots for rollback.
    fn rollback_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Copy;

    /// Registers an immutable component type for saving and loading from the world. This
    /// uses [`Copy`] based snapshots for rollback.
    fn rollback_immutable_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Copy;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`Copy`] based snapshots for rollback.
    fn rollback_resource_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Copy;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Clone;

    /// [`rollback_component_with_copy`](Self::rollback_component_with_copy) for
    /// a [`RollbackEq`] type: a load leaves a component that already holds the
    /// snapshot's bits alone, so it is not marked changed.
    fn rollback_component_with_copy_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Copy + RollbackEq;

    /// [`rollback_component_with_clone`](Self::rollback_component_with_clone)
    /// for a [`RollbackEq`] type, as
    /// [`rollback_component_with_copy_eq`](Self::rollback_component_with_copy_eq).
    fn rollback_component_with_clone_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Clone + RollbackEq;

    /// An immutable component for a [`RollbackEq`] type: a load does not
    /// re-insert, and fires no hook for, a component that already holds the
    /// snapshot's bits.
    fn rollback_immutable_component_with_copy_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Copy + RollbackEq;

    /// As [`rollback_immutable_component_with_copy_eq`](Self::rollback_immutable_component_with_copy_eq),
    /// with [`Clone`].
    fn rollback_immutable_component_with_clone_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Clone + RollbackEq;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_immutable_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Clone;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Clone;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`reflection`](`Reflect`) based snapshots for rollback.
    ///
    /// NOTE: Unlike previous versions of `bevy_ggrs`, this will no longer automatically
    /// apply entity mapping through the [`MapEntities`](`bevy::ecs::entity::MapEntities`) trait.
    /// If you require this behavior, see [`ComponentMapEntitiesPlugin`].
    fn rollback_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Reflect + FromWorld;

    /// Registers an immutable component type for saving and loading from the world. This
    /// uses [`reflection`](`Reflect`) based snapshots for rollback.
    ///
    /// NOTE: Unlike previous versions of `bevy_ggrs`, this will no longer automatically
    /// apply entity mapping through the [`MapEntities`](`bevy::ecs::entity::MapEntities`) trait.
    /// If you require this behavior, see [`ComponentMapEntitiesPlugin`].
    fn rollback_immutable_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Reflect + FromWorld;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`reflection`](`Reflect`) based snapshots for rollback.
    ///
    /// NOTE: Unlike previous versions of `bevy_ggrs`, this will no longer automatically
    /// apply entity mapping through the [`MapEntities`](`bevy::ecs::entity::MapEntities`) trait.
    /// If you require this behavior, see [`ComponentMapEntitiesPlugin`].
    fn rollback_resource_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Reflect + FromWorld;

    /// Adds a component type to the checksum generation pipeline using [`Hash`].
    fn checksum_component_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Component + Hash;

    /// Updates a component after rollback using [`MapEntities`].
    fn update_component_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + MapEntities;

    /// Adds a resource type to the checksum generation pipeline using [`Hash`].
    fn checksum_resource_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Hash;

    /// Updates a resource after rollback using [`MapEntities`].
    fn update_resource_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + MapEntities;

    /// Adds a component type to the checksum generation pipeline.
    fn checksum_component<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Component;

    /// Adds a resource type to the checksum generation pipeline.
    fn checksum_resource<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Resource;

    /// Registers [`Rollback`](`super::Rollback`) as a required component for `Type`.
    /// This is useful for third-party components where you can't add `#[require(Rollback)]`.
    fn require_rollback<Type>(&mut self) -> &mut Self
    where
        Type: Component;

    /// Registers an observer that automatically creates a [`RollbackId`](`super::RollbackId`)
    /// when `Type` is added to an entity. This replaces the need to manually add
    /// [`Rollback`](`super::Rollback`) to spawn bundles.
    fn rollback_entities_with<Type>(&mut self) -> &mut Self
    where
        Type: Component;

    /// Registers rollback for `Type` using the configuration from its
    /// [`RollbackRegistration`] implementation (typically generated by `#[derive(Rollback)]`).
    fn rollback<Type>(&mut self) -> &mut Self
    where
        Type: RollbackRegistration;

    /// Disables auto-registration of rollback types from the type registry.
    /// When called, types with `#[reflect(Rollback)]` will NOT be automatically
    /// registered — you must use [`rollback`](`RollbackApp::rollback`) or the
    /// manual `rollback_component_with_*` methods instead.
    fn skip_auto_registration(&mut self) -> &mut Self;
}

impl RollbackApp for App {
    fn rollback_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Reflect + FromWorld,
    {
        self.add_plugins(ComponentSnapshotPlugin::<ReflectStrategy<Type>>::default())
    }

    fn rollback_immutable_component_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Reflect + FromWorld,
    {
        self.add_plugins(ImmutableComponentSnapshotPlugin::<ReflectStrategy<Type>>::default())
    }

    fn rollback_resource_with_reflect<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Reflect + FromWorld,
    {
        self.add_plugins(ResourceSnapshotPlugin::<ReflectStrategy<Type>>::default())
    }

    fn rollback_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Copy,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CopyStrategy<Type>>::default())
    }

    fn rollback_immutable_component_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Copy,
    {
        self.add_plugins(ImmutableComponentSnapshotPlugin::<CopyStrategy<Type>>::default())
    }

    fn rollback_resource_with_copy<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Copy,
    {
        self.add_plugins(ResourceSnapshotPlugin::<CopyStrategy<Type>>::default())
    }

    fn rollback_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Clone,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CloneStrategy<Type>>::default())
    }

    fn rollback_immutable_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Clone,
    {
        self.add_plugins(ImmutableComponentSnapshotPlugin::<CloneStrategy<Type>>::default())
    }

    fn rollback_component_with_copy_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Copy + RollbackEq,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CopyEqStrategy<Type>>::default())
    }

    fn rollback_component_with_clone_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Clone + RollbackEq,
    {
        self.add_plugins(ComponentSnapshotPlugin::<CloneEqStrategy<Type>>::default())
    }

    fn rollback_immutable_component_with_copy_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Copy + RollbackEq,
    {
        self.add_plugins(ImmutableComponentSnapshotPlugin::<CopyEqStrategy<Type>>::default())
    }

    fn rollback_immutable_component_with_clone_eq<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Clone + RollbackEq,
    {
        self.add_plugins(ImmutableComponentSnapshotPlugin::<CloneEqStrategy<Type>>::default())
    }

    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + Clone,
    {
        self.add_plugins(ResourceSnapshotPlugin::<CloneStrategy<Type>>::default())
    }

    fn checksum_component_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Component + Hash,
    {
        self.add_plugins(ComponentChecksumPlugin::<Type>::default())
    }

    fn update_component_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + MapEntities,
    {
        self.add_plugins(ComponentMapEntitiesPlugin::<Type>::default())
    }

    fn checksum_resource_with_hash<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Hash,
    {
        self.add_plugins(ResourceChecksumPlugin::<Type>::default())
    }

    fn update_resource_with_map_entities<Type>(&mut self) -> &mut Self
    where
        Type: Resource<Mutability = Mutable> + MapEntities,
    {
        self.add_plugins(ResourceMapEntitiesPlugin::<Type>::default())
    }

    fn checksum_component<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Component,
    {
        self.add_plugins(ComponentChecksumPlugin::<Type>(hasher))
    }

    fn checksum_resource<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Resource,
    {
        self.add_plugins(ResourceChecksumPlugin::<Type>(hasher))
    }

    fn require_rollback<Type>(&mut self) -> &mut Self
    where
        Type: Component,
    {
        self.register_required_components::<Type, super::Rollback>();
        self
    }

    fn rollback_entities_with<Type>(&mut self) -> &mut Self
    where
        Type: Component,
    {
        self.add_plugins(RollbackEntitiesPlugin::<Type>::default())
    }

    fn rollback<Type>(&mut self) -> &mut Self
    where
        Type: RollbackRegistration,
    {
        Type::register_rollback(self);
        self
    }

    fn skip_auto_registration(&mut self) -> &mut Self {
        self.insert_resource(SkipAutoRegistration);
        self
    }
}
