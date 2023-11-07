use std::time::Duration;

use bevy::prelude::*;

use crate::{
    AdvanceWorld, AdvanceWorldSet, CloneStrategy, ResourceSnapshotPlugin, RollbackFrameCount,
    DEFAULT_FPS,
};

/// [`Resource`] describing the rate at which the [`AdvanceWorld`] will run.
#[derive(Resource, Clone, Copy, Debug, Hash, Deref)]
pub struct RollbackFrameRate(pub(crate) usize);

impl Default for RollbackFrameRate {
    fn default() -> Self {
        Self(DEFAULT_FPS)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct GgrsTime;

/// This plugins provides [`Time<GgrsTime>`], which is rolled-back automatically, and will also
/// automatically replace [`Time<()>`] when accessed inside [`GgrsSchedule`](`crate::GgrsSchedule`).
pub struct GgrsTimePlugin;

impl GgrsTimePlugin {
    pub fn update(
        mut time: ResMut<Time<GgrsTime>>,
        framerate: Res<RollbackFrameRate>,
        frame: Res<RollbackFrameCount>,
    ) {
        let this_frame = frame.0 as u64;
        let framerate = framerate.0 as u64;

        // 1_000_000_000 fits within a u32, and so does frame, making their product at most u64 in size
        // By scaling to nanoseconds, rounding error should be minimised.
        let runtime = Duration::from_nanos(this_frame * 1_000_000_000 / framerate);

        time.advance_to(runtime);
    }

    pub fn replace_default_with_ggrs(
        mut default_time: ResMut<Time<()>>,
        ggrs_time: Res<Time<GgrsTime>>,
    ) {
        *default_time = ggrs_time.as_generic();
    }

    pub fn replace_default_with_virtual(
        mut default_time: ResMut<Time<()>>,
        virtual_time: Res<Time<Virtual>>,
    ) {
        *default_time = virtual_time.as_generic();
    }
}

impl Plugin for GgrsTimePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::new_with(GgrsTime::default()))
            .add_plugins(ResourceSnapshotPlugin::<CloneStrategy<Time<GgrsTime>>>::default())
            .add_systems(
                AdvanceWorld,
                (Self::update, Self::replace_default_with_ggrs)
                    .chain()
                    .in_set(AdvanceWorldSet::First),
            )
            .add_systems(
                AdvanceWorld,
                Self::replace_default_with_virtual.in_set(AdvanceWorldSet::Last),
            );
    }
}
