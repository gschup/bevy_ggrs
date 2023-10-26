use std::{
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
};

use bevy::prelude::*;

use crate::{ChecksumFlag, ChecksumPart, Rollback, RollbackOrdered, SaveWorld, SaveWorldSet};

/// A [`Plugin`] which will track the [`Component`] `C` on [`Rollback Entities`](`Rollback`) and ensure a
/// [`ChecksumPart`] is available and updated. This can be used to generate a [`Checksum`](`crate::Checksum`).
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ComponentChecksumHashPlugin};
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// #[derive(Component, Clone, Copy, Hash)]
/// struct Health(u32);
///
/// // To include something in the checksum, it should also be rolled back
/// app.rollback_component_with_clone::<Health>();
///
/// // This will update the checksum every frame to include Health on rollback entities
/// app.add_plugins(ComponentChecksumHashPlugin::<Health>::default());
/// # }
/// ```
pub struct ComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    _phantom: PhantomData<C>,
}

impl<C> Default for ComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<C> ComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    /// A [`System`] responsible for managing a [`ChecksumPart`] for the [`Component`] type `C`.
    #[allow(clippy::type_complexity)]
    pub fn update(
        mut commands: Commands,
        rollback_ordered: Res<RollbackOrdered>,
        components: Query<(&Rollback, &C), (With<Rollback>, Without<ChecksumFlag<C>>)>,
        mut checksum: Query<&mut ChecksumPart, (Without<Rollback>, With<ChecksumFlag<C>>)>,
    ) {
        let mut hasher = bevy::utils::FixedState.build_hasher();

        let mut result = 0;

        for (&rollback, component) in components.iter() {
            let mut hasher = hasher.clone();

            // Hashing the rollback index ensures this hash is unique and stable
            rollback_ordered.order(rollback).hash(&mut hasher);
            component.hash(&mut hasher);

            // XOR chosen over addition or multiplication as it is closed on u64 and commutative
            result ^= hasher.finish();
        }

        // Hash the XOR'ed result to break commutativity with other types
        result.hash(&mut hasher);

        let result = ChecksumPart(hasher.finish() as u128);

        trace!(
            "Component {} has checksum {:X}",
            bevy::utils::get_short_name(std::any::type_name::<C>()),
            result.0
        );

        if let Ok(mut checksum) = checksum.get_single_mut() {
            *checksum = result;
        } else {
            commands.spawn((result, ChecksumFlag::<C>::default()));
        }
    }
}

impl<C> Plugin for ComponentChecksumHashPlugin<C>
where
    C: Component + Hash,
{
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update.in_set(SaveWorldSet::Checksum));
    }
}
