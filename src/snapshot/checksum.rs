use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy::prelude::*;

use crate::SaveWorld;

/// Flags an entity as containing a checksum for a type `T`
#[derive(Component)]
pub struct ChecksumFlag<T> {
    _phantom: PhantomData<T>,
}

impl<T> Default for ChecksumFlag<T> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

/// Represents a checksum value for a specific type, flagged by [`ChecksumFlag`].
#[derive(Component, Default, Hash)]
pub struct ChecksumPart(pub u64);

/// Represents a total checksum for a given frame.
#[derive(Resource, Default, Clone)]
pub struct Checksum(pub u64);

pub struct GgrsChecksumPlugin;

impl GgrsChecksumPlugin {
    pub fn update(mut checksum: ResMut<Checksum>, parts: Query<&ChecksumPart>) {
        let mut hasher = DefaultHasher::new();

        for part in parts.iter() {
            part.hash(&mut hasher);
        }

        *checksum = Checksum(hasher.finish());
    }
}

impl Plugin for GgrsChecksumPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Checksum>()
            .add_systems(SaveWorld, Self::update);
    }
}
