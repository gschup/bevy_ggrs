use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy::prelude::*;

use crate::{ChecksumFlag, ChecksumPart, Rollback, SaveWorld};

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
    pub fn update(
        mut commands: Commands,
        components: Query<&C, (With<Rollback>, Without<ChecksumFlag<C>>)>,
        mut checksum: Query<&mut ChecksumPart, (Without<Rollback>, With<ChecksumFlag<C>>)>,
    ) {
        let mut hasher = DefaultHasher::new();

        let Ok(mut checksum) = checksum.get_single_mut() else {
            commands.spawn((ChecksumPart::default(), ChecksumFlag::<C>::default()));

            return;
        };

        for component in components.iter() {
            component.hash(&mut hasher);
        }

        *checksum = ChecksumPart(hasher.finish());
    }
}

impl<C> Plugin for GgrsComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update);
    }
}
