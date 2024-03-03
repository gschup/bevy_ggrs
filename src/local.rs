use crate::{ConfirmedFrameCount, LastRollback, RollbackFrameCount, Rollbacks};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

#[derive(Debug)]
struct LocalState<T> {
    rollback_generation: usize,
    frame: i32,
    data: T,
}

#[derive(Debug)]
struct LocalStates<T>(VecDeque<LocalState<T>>);

impl<T: FromWorld> FromWorld for LocalStates<T> {
    fn from_world(world: &mut World) -> Self {
        Self(
            vec![LocalState {
                rollback_generation: 0,
                // the first frame has frame count 0, so we need to store it prior to that
                frame: -1,
                data: T::from_world(world),
            }]
            .into(),
        )
    }
}

#[derive(SystemParam, Debug)]
pub struct GgrsLocal<'w, 's, T: FromWorld + Clone + Sync + Send + 'static> {
    snapshots: Local<'s, LocalStates<T>>,
    rollbacks: Res<'w, Rollbacks>,
    last_rollback: Res<'w, LastRollback>,
    current_frame: Res<'w, RollbackFrameCount>,
    confirmed_frame: Res<'w, ConfirmedFrameCount>,
}

impl<'w, 's, T: FromWorld + Clone + Sync + Send + 'static> Deref for GgrsLocal<'w, 's, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.snapshots.0[self.get_snapshot_index()].data
    }
}

impl<'w, 's, T: FromWorld + Clone + Sync + Send + 'static> DerefMut for GgrsLocal<'w, 's, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        let confirmed_frames = self
            .snapshots
            .0
            .iter()
            .take_while(|s| s.frame <= self.confirmed_frame.0)
            .count();

        // keep only the last confirmed frame
        self.snapshots
            .0
            .drain(0..confirmed_frames.saturating_sub(1));

        let mut i = self.get_snapshot_index();

        // snapshots after this are mis-predictions, so remove them
        self.snapshots.0.truncate(i + 1);

        if self.snapshots.0[i].frame != self.current_frame.0 {
            let new_snapshot = LocalState {
                rollback_generation: self.rollbacks.0,
                frame: self.current_frame.0,
                data: self.snapshots.0[i].data.clone(),
            };
            self.snapshots.0.push_back(new_snapshot);
            i = self.snapshots.0.len() - 1;
        }

        &mut self.snapshots.0[i].data
    }
}

impl<'w, 's, T: FromWorld + Clone + Sync + Send + 'static> GgrsLocal<'w, 's, T> {
    /// gets the index of the snapshot that's the best fit for the current frame
    fn get_snapshot_index(&self) -> usize {
        let current_frame = self.current_frame.0;
        let last_rollback = self.last_rollback.0;

        let mut best_snapshot_index = None;
        let mut best_gen = 0;

        for (i, snapshot) in self.snapshots.0.iter().enumerate() {
            if snapshot.frame > current_frame {
                break;
            }
            if snapshot.rollback_generation < best_gen {
                break;
            }
            best_gen = snapshot.rollback_generation;

            let valid = if snapshot.rollback_generation == **self.rollbacks {
                true
            } else if snapshot.rollback_generation + 1 == **self.rollbacks {
                // from last generation, valid if we didn't roll back past it
                snapshot.frame < last_rollback
            } else {
                panic!(
                    "Encountered old snapshot. Make sure systems with Ggrs locals run every frame"
                );
            };

            if valid {
                best_snapshot_index = Some(i);
            }
        }

        best_snapshot_index
            .expect("No valid snapshots available. Make sure the system runs every frame")
    }
}

#[cfg(test)]
mod test {
    use super::GgrsLocal;
    use crate::{ConfirmedFrameCount, LastRollback, RollbackFrameCount, Rollbacks};
    use bevy::prelude::*;

    #[test]
    fn constructor() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut system = IntoSystem::into_system(|_: GgrsLocal<bool>| {});
        system.initialize(&mut world);

        system.run((), &mut world);
    }

    #[test]
    fn constructor_multiple() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut system = IntoSystem::into_system(|_: GgrsLocal<bool>, _: GgrsLocal<usize>| {});
        system.initialize(&mut world);

        system.run((), &mut world);
    }

    #[test]
    fn default_value() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut system = IntoSystem::into_system(|local: GgrsLocal<bool>| -> bool { *local });
        system.initialize(&mut world);

        let value = system.run((), &mut world);
        assert!(!value);
    }

    #[test]
    fn mutate() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut system = IntoSystem::into_system(|mut local: GgrsLocal<bool>| -> bool {
            *local = true;
            *local
        });
        system.initialize(&mut world);

        let value = system.run((), &mut world);
        assert!(value);
    }

    #[test]
    fn simple_rollback() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut increment_system =
            IntoSystem::into_system(|mut counter: GgrsLocal<usize>| -> usize {
                *counter += 1;
                *counter
            });

        increment_system.initialize(&mut world);

        let value = increment_system.run((), &mut world);
        assert_eq!(value, 1);

        world.insert_resource(RollbackFrameCount(1));

        let value = increment_system.run((), &mut world);
        assert_eq!(value, 2);

        // now simulate a rollback
        world.insert_resource(Rollbacks(1));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let value = increment_system.run((), &mut world);
        assert_eq!(value, 1);
    }

    fn advance_frames(world: &mut World, frames: i32) {
        let current_frame = world.get_resource::<RollbackFrameCount>().unwrap().0;
        world.get_resource_mut::<RollbackFrameCount>().unwrap().0 += frames;

        if frames < 0 {
            world.get_resource_mut::<LastRollback>().unwrap().0 = current_frame + frames;
            world.get_resource_mut::<Rollbacks>().unwrap().0 += 1;
        }
    }

    #[test]
    fn discard_old_snapshots() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut toggle_system = IntoSystem::into_system(|mut local: GgrsLocal<usize>| {
            // todo: be more specific about the limit here
            assert!(local.snapshots.0.len() <= 10);
            *local = !*local;
        });

        toggle_system.initialize(&mut world);

        for i in 0..50 {
            toggle_system.run((), &mut world);
            let confirmed = i - 5;
            if confirmed >= 0 {
                world.insert_resource(ConfirmedFrameCount(confirmed));
            }
            advance_frames(&mut world, 1);
        }
    }

    #[test]
    fn does_not_pick_old_frames() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut add_system =
            IntoSystem::into_system(|In(input): In<usize>, mut sum: GgrsLocal<usize>| -> usize {
                *sum += input;
                *sum
            });

        add_system.initialize(&mut world);

        for _ in 0..9 {
            add_system.run(1, &mut world);
            advance_frames(&mut world, 1);
        }
        let sum = add_system.run(1, &mut world);
        assert_eq!(sum, 10);

        // now we roll back 5 frames (from 9 to 4)
        advance_frames(&mut world, -5);
        let sum = add_system.run(100, &mut world);
        assert_eq!(sum, 104);

        advance_frames(&mut world, 1);
        let sum = add_system.run(1, &mut world);
        assert_eq!(sum, 105);

        advance_frames(&mut world, 1);
        let sum = add_system.run(1, &mut world);
        assert_eq!(sum, 106);
    }

    // #[test]
    // fn handles_skipped_frames() {
    //     let mut world = World::new();

    //     world.insert_resource(Rollbacks(0));
    //     world.insert_resource(LastRollback(0));
    //     world.insert_resource(RollbackFrameCount(0));
    //     world.insert_resource(ConfirmedFrameCount(-1));

    //     let mut add_system =
    //         IntoSystem::into_system(|In(input): In<usize>, mut sum: GgrsLocal<usize>| -> usize {
    //             *sum += input;
    //             *sum
    //         });

    //     add_system.initialize(&mut world);

    //     for _ in 0..9 {
    //         add_system.run(1, &mut world);
    //         advance_frames(&mut world, 1);
    //     }
    //     let sum = add_system.run(1, &mut world);
    //     assert_eq!(sum, 10);

    //     // now we roll back 5 frames (from 9 to 4)
    //     advance_frames(&mut world, -5);
    //     let sum = add_system.run(100, &mut world);
    //     assert_eq!(sum, 104);

    //     // skip a frame (might happen due to run conditions)
    //     advance_frames(&mut world, 2);
    //     let sum = add_system.run(1, &mut world);
    //     assert_eq!(sum, 105);

    //     advance_frames(&mut world, 1);
    //     let sum = add_system.run(1, &mut world);
    //     assert_eq!(sum, 106);
    // }

    // todo: it would be nice to actually handle this, but for now, it's good enough that we panic
    #[test]
    #[should_panic]
    fn multiple_rollbacks_behind_system_param_panics() {
        let mut world = World::new();

        world.insert_resource(Rollbacks(0));
        world.insert_resource(LastRollback(0));
        world.insert_resource(RollbackFrameCount(0));
        world.insert_resource(ConfirmedFrameCount(-1));

        let mut add_system =
            IntoSystem::into_system(|In(input): In<usize>, mut sum: GgrsLocal<usize>| -> usize {
                *sum += input;
                *sum
            });

        add_system.initialize(&mut world);

        for _ in 0..9 {
            add_system.run(1, &mut world);
            advance_frames(&mut world, 1);
        }
        let _sum = add_system.run(1, &mut world);
        // assert_eq!(sum, 10);

        world.insert_resource(Rollbacks(1));
        world.insert_resource(LastRollback(4));
        world.insert_resource(RollbackFrameCount(4));

        // assume the system doesn't run because of run conditions

        world.insert_resource(Rollbacks(2));
        world.insert_resource(LastRollback(8));
        world.insert_resource(RollbackFrameCount(8));
        let _sum = add_system.run(1, &mut world);
        // frame 4 was the last time the system ran prior to any rollbacks
        // so it should use the snapshot from frame 4 (4) and add 1 to get 5
        // assert_eq!(sum, 5);
    }
}
