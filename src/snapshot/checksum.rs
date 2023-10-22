use std::{
    hash::{Hash, Hasher, BuildHasher},
    marker::PhantomData,
};

use bevy::prelude::*;

use crate::{SaveWorld, SaveWorldSet};

/// Flags an entity as containing a checksum for a type `T`
#[derive(Component)]
pub struct ChecksumFlag<T> {
    _phantom: PhantomData<T>,
}

impl<T> Default for ChecksumFlag<T> {
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

/// Represents a checksum value for a specific type, flagged by [`ChecksumFlag`].
#[derive(Component, Default, Hash)]
pub struct ChecksumPart(pub u128);

/// Represents a total checksum for a given frame.
#[derive(Resource, Default, Clone, Copy)]
pub struct Checksum(pub u128);

/// A [`Plugin`] which creates a [`Checksum`] resource which can be read after the
/// [`SaveWorldSet::Snapshot`] set in the [`SaveWorld`] schedule has been run.
///
/// To add you own data to this [`Checksum`], create an [`Entity`] with a [`ChecksumPart`]
/// [`Component`]. Every [`Entity`] with this [`Component`] will participate in the
/// creation of a [`Checksum`].
pub struct GgrsChecksumPlugin;

impl GgrsChecksumPlugin {
    /// A [`System`] responsible for updating [`Checksum`] based on [`ChecksumParts`](`ChecksumPart`).
    pub fn update(mut checksum: ResMut<Checksum>, parts: Query<&ChecksumPart>) {
        let parts = parts.iter().fold(0, |result: u128, &ChecksumPart(part)| {
            result.wrapping_add(part)
        });

        let mut hasher = bevy::utils::FixedState::default().build_hasher();

        parts.hash(&mut hasher);

        *checksum = Checksum(hasher.finish() as u128);
    }
}

impl Plugin for GgrsChecksumPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Checksum>()
            .add_systems(SaveWorld, Self::update.after(SaveWorldSet::Checksum).before(SaveWorldSet::Snapshot));
    }
}
