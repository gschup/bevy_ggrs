use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy::prelude::*;

use crate::{ChecksumFlag, ChecksumPart, Rollback, SaveWorld, SaveWorldSet};

/// A [`Plugin`] which will track the [`Component`] `C` on [`Rollback Entities`](`Rollback`) and ensure a
/// [`ChecksumPart`] is available and updated. This can be used to generate a [`Checksum`](`crate::Checksum`).
pub struct GgrsComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for GgrsComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<C> GgrsComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    /// A [`System`] responsible for managing a [`ChecksumPart`] for the [`Component`] type `C`.
    pub fn update(
        mut commands: Commands,
        components: Query<(&Rollback, &C), (With<Rollback>, Without<ChecksumFlag<C>>)>,
        mut checksum: Query<&mut ChecksumPart, (Without<Rollback>, With<ChecksumFlag<C>>)>,
    ) {
        let mut hasher = DefaultHasher::new();

        let mut components = components.iter().collect::<Vec<_>>();

        components.sort_by_key(|(&rollback, _)| rollback);

        for (_, component) in components {
            component.hash(&mut hasher);
        }

        let result = ChecksumPart(hasher.finish());

        if let Ok(mut checksum) = checksum.get_single_mut() {
            *checksum = result;
        } else {
            commands.spawn((result, ChecksumFlag::<C>::default()));
        }
    }
}

impl<C> Plugin for GgrsComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update.in_set(SaveWorldSet::Snapshot));
    }
}
