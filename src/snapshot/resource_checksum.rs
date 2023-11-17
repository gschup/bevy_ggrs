use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::{checksum_hasher, ChecksumFlag, ChecksumPart, Rollback, SaveWorld, SaveWorldSet};

/// Plugin which will track the [`Resource`] `R` and ensure a [`ChecksumPart`] is
/// available and updated. This can be used to generate a [`Checksum`](`crate::Checksum`).
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ResourceChecksumPlugin};
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
/// app.add_plugins(ResourceChecksumPlugin::<BossHealth>::default());
/// # }
/// ```
pub struct ResourceChecksumPlugin<R: Resource>(pub for<'a> fn(&'a R) -> u64);

fn default_hasher<R: Resource + Hash>(resource: &R) -> u64 {
    let mut hasher = checksum_hasher();
    resource.hash(&mut hasher);
    hasher.finish()
}

impl<R> Default for ResourceChecksumPlugin<R>
where
    R: Resource + Hash,
{
    fn default() -> Self {
        Self(default_hasher::<R>)
    }
}

impl<R> Plugin for ResourceChecksumPlugin<R>
where
    R: Resource,
{
    fn build(&self, app: &mut App) {
        let custom_hasher = self.0;

        let update = move |mut commands: Commands,
                           resource: Res<R>,
                           mut checksum: Query<
            &mut ChecksumPart,
            (Without<Rollback>, With<ChecksumFlag<R>>),
        >| {
            let result = ChecksumPart(custom_hasher(resource.as_ref()) as u128);

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
        };
        app.add_systems(SaveWorld, update.in_set(SaveWorldSet::Checksum));
    }
}
