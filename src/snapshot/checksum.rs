use std::{
    hash::{BuildHasher, Hash, Hasher},
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

impl ChecksumPart {
    /// Converts a provided value `T` into a [Hash] using Bevy's [`FixedState`](`bevy::utils::FixedState`) hasher.
    pub fn from_value<T: Hash>(value: &T) -> Self {
        let mut hasher = bevy::utils::FixedState.build_hasher();

        value.hash(&mut hasher);

        Self(hasher.finish() as u128)
    }
}

/// Represents a total checksum for a given frame.
#[derive(Resource, Default, Clone, Copy)]
pub struct Checksum(pub u128);

/// A [`Plugin`] which creates a [`Checksum`] resource which can be read after or during the
/// [`SaveWorldSet::Snapshot`] set in the [`SaveWorld`] schedule has been run.
///
/// To add you own data to this [`Checksum`], create an [`Entity`] with a [`ChecksumPart`]
/// [`Component`]. Every [`Entity`] with this [`Component`] will participate in the
/// creation of a [`Checksum`].
pub struct GgrsChecksumPlugin;

impl GgrsChecksumPlugin {
    /// A [`System`] responsible for updating [`Checksum`] based on [`ChecksumParts`](`ChecksumPart`).
    pub fn update(mut checksum: ResMut<Checksum>, parts: Query<&ChecksumPart>) {
        // TODO: Add explicit ordering to `ChecksumPart`'s to make checksum more robust to transposition
        // XOR is commutative, ensuring order does not matter.
        // Chosen over addition and multiplication as XOR is closed on u128
        let parts = parts.iter().fold(0, |a: u128, &ChecksumPart(b)| a ^ b);

        trace!("Frame has checksum {:X}", parts);

        *checksum = Checksum(parts);
    }
}

impl Plugin for GgrsChecksumPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Checksum>().add_systems(
            SaveWorld,
            Self::update
                .after(SaveWorldSet::PreSnapshotFlush)
                .before(SaveWorldSet::Snapshot),
        );
    }
}