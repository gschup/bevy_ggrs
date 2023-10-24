use std::{hash::Hash, marker::PhantomData};

use bevy::prelude::*;

use crate::{ChecksumFlag, ChecksumPart, Rollback, SaveWorld, SaveWorldSet};

/// Plugin which will track the [`Resource`] `R` and ensure a [`ChecksumPart`] is
/// available and updated. This can be used to generate a [`Checksum`](`crate::Checksum`).
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::prelude::*;
/// #
/// # const FPS: usize = 60;
/// #
/// # type MyInputType = u8;
/// #
/// # fn read_local_inputs() {}
/// #
/// # fn start(session: Session<GgrsConfig<MyInputType>>) {
/// # let mut app = App::new();
/// #[derive(Resource, Clone, Hash)]
/// struct BossHealth(u32);
///
/// // To include something in the checksum, it should also be rolled back
/// app.rollback_resource_with_clone::<BossHealth>();
///
/// // This will update the checksum every frame to include BossHealth
/// app.add_plugins(GgrsResourceChecksumHashPlugin::<BossHealth>::default());
/// # }
/// ```
pub struct GgrsResourceChecksumHashPlugin<R>
where
    R: Resource + Hash,
{
    _phantom: PhantomData<R>,
}

impl<R> Default for GgrsResourceChecksumHashPlugin<R>
where
    R: Resource + Hash,
{
    fn default() -> Self {
        Self {
            _phantom: default(),
        }
    }
}

impl<R> GgrsResourceChecksumHashPlugin<R>
where
    R: Resource + Hash,
{
    /// A [`System`] responsible for managing a [`ChecksumPart`] for the [`Resource`] type `R`.
    pub fn update(
        mut commands: Commands,
        resource: Res<R>,
        mut checksum: Query<&mut ChecksumPart, (Without<Rollback>, With<ChecksumFlag<R>>)>,
    ) {
        let result = ChecksumPart::from_value(resource.as_ref());

        trace!(
            "Resource {} has checksum {:X}",
            bevy::utils::get_short_name(std::any::type_name::<R>()),
            result.0
        );

        if let Ok(mut checksum) = checksum.get_single_mut() {
            *checksum = result;
        } else {
            commands.spawn((result, ChecksumFlag::<R>::default()));
        }
    }
}

impl<R> Plugin for GgrsResourceChecksumHashPlugin<R>
where
    R: Resource + Hash,
{
    fn build(&self, app: &mut App) {
        app.add_systems(SaveWorld, Self::update.in_set(SaveWorldSet::Checksum));
    }
}
