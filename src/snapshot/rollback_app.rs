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
    CopyStrategy, ImmutableComponentSnapshotPlugin, ReflectStrategy, ResourceMapEntitiesPlugin,
};

/// Extension trait to ergonimically add rollback plugins to Bevy Apps
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
        Type: Resource + Copy;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Mutable> + Clone;

    /// Registers a component type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_immutable_component_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Component<Mutability = Immutable> + Clone;

    /// Registers a resource type for saving and loading from the world. This
    /// uses [`Clone`] based snapshots for rollback.
    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Clone;

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
        Type: Resource + Reflect + FromWorld;

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
        Type: Resource + MapEntities;

    /// Adds a component type to the checksum generation pipeline.
    fn checksum_component<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Component;

    /// Adds a resource type to the checksum generation pipeline.
    fn checksum_resource<Type>(&mut self, hasher: for<'a> fn(&'a Type) -> u64) -> &mut Self
    where
        Type: Resource;
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
        Type: Resource + Reflect + FromWorld,
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
        Type: Resource + Copy,
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

    fn rollback_resource_with_clone<Type>(&mut self) -> &mut Self
    where
        Type: Resource + Clone,
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
        Type: Resource + MapEntities,
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
}
