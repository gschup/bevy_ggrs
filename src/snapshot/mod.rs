use crate::{ConfirmedFrameCount, MaxPredictionWindow, Rollback};
use bevy::{prelude::*, utils::HashMap};
use std::{collections::VecDeque, marker::PhantomData};

mod checksum;
mod component_checksum_hash;
mod component_clone;
mod component_copy;
mod component_map;
mod component_reflect;
mod entity;
mod resource_checksum_hash;
mod resource_clone;
mod resource_copy;
mod resource_map;
mod resource_reflect;
mod rollback_entity_map;
mod set;

pub use checksum::*;
pub use component_checksum_hash::*;
pub use component_clone::*;
pub use component_copy::*;
pub use component_map::*;
pub use component_reflect::*;
pub use entity::*;
pub use resource_checksum_hash::*;
pub use resource_clone::*;
pub use resource_copy::*;
pub use resource_map::*;
pub use resource_reflect::*;
pub use rollback_entity_map::*;
pub use set::*;

pub mod prelude {
    pub use super::{
        Checksum, GgrsChecksumPlugin, GgrsComponentChecksumHashPlugin,
        GgrsComponentMapEntitiesPlugin, GgrsComponentSnapshotClonePlugin,
        GgrsComponentSnapshotCopyPlugin, GgrsComponentSnapshotReflectPlugin,
        GgrsEntitySnapshotPlugin, GgrsResourceChecksumHashPlugin, GgrsResourceMapEntitiesPlugin,
        GgrsResourceSnapshotClonePlugin, GgrsResourceSnapshotCopyPlugin,
        GgrsResourceSnapshotReflectPlugin, GgrsSnapshotSetPlugin, LoadWorldSet, SaveWorldSet,
    };
}

/// Typical [`Resource`] used to store snapshots for a [`Resource`] `R` as the type `As`.
/// For most types, the default `As = R` will suffice.
pub type GgrsResourceSnapshots<R, As = R> = GgrsSnapshots<R, Option<As>>;

/// Typical [`Resource`] used to store snapshots for a [`Component`] `C` as the type `As`.
/// For most types, the default `As = C` will suffice.
pub type GgrsComponentSnapshots<C, As = C> = GgrsSnapshots<C, GgrsComponentSnapshot<C, As>>;

/// Collection of snapshots for a type `For`, stored as `As`
#[derive(Resource)]
pub struct GgrsSnapshots<For, As = For> {
    /// Queue of snapshots, newest at the front, oldest at the back.
    snapshots: VecDeque<As>,
    /// Queue of snapshots, newest at the front, oldest at the back.
    /// Separate from snapshots to avoid padding.
    frames: VecDeque<i32>,
    /// Maximum amount of snapshots to store at any one time
    depth: usize,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsSnapshots<For, As> {
    /// Create a default [`GgrsSnapshots`] resource. This will only track a maximum of 8 snapshots.
    /// If you require a longer rollback window, use [`set_depth`](`GgrsSnapshots::set_depth`)
    fn default() -> Self {
        // 8 is the current default `max_prediction_window`
        let depth = 8;

        Self {
            snapshots: VecDeque::with_capacity(depth),
            frames: VecDeque::with_capacity(depth),
            depth,
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsSnapshots<For, As> {
    pub fn set_depth(&mut self, depth: usize) -> &mut Self {
        self.depth = depth;

        // Greedy allocation to avoid allocating at a more sensitive time.
        if self.snapshots.capacity() < self.depth {
            let additional = self.depth - self.snapshots.capacity();
            self.snapshots.reserve(additional);
        }

        if self.frames.capacity() < self.depth {
            let additional = self.depth - self.frames.capacity();
            self.frames.reserve(additional);
        }

        self
    }

    pub const fn depth(&self) -> usize {
        self.depth
    }

    pub fn push(&mut self, frame: i32, snapshot: As) -> &mut Self {
        debug_assert_eq!(
            self.snapshots.len(),
            self.frames.len(),
            "Snapshot and Frame queues must always be in sync"
        );

        loop {
            let Some(&current) = self.frames.front() else {
                break;
            };

            // TODO: Handle wrapping behavior (wrapping_sub, etc.)

            if current >= frame {
                self.snapshots.pop_front().unwrap();
                self.frames.pop_front().unwrap();
            } else {
                break;
            }
        }

        self.snapshots.push_front(snapshot);
        self.frames.push_front(frame);

        while self.snapshots.len() > self.depth {
            self.snapshots.pop_back().unwrap();
            self.frames.pop_back().unwrap();
        }

        self
    }

    pub fn confirm(&mut self, confirmed_frame: i32) -> &mut Self {
        debug_assert_eq!(
            self.snapshots.len(),
            self.frames.len(),
            "Snapshot and Frame queues must always be in sync"
        );

        while let Some(&frame) = self.frames.back() {
            if frame < confirmed_frame {
                self.snapshots.pop_back().unwrap();
                self.frames.pop_back().unwrap();
            } else {
                break;
            }
        }

        self
    }

    pub fn rollback(&mut self, frame: i32) -> &mut Self {
        loop {
            let Some(&current) = self.frames.front() else {
                // TODO: A panic may not be appropriate here, but suitable for now.
                panic!("Could not rollback to {frame}: no snapshot at that moment could be found.");
            };

            if current != frame {
                self.snapshots.pop_front().unwrap();
                self.frames.pop_front().unwrap();
            } else {
                break;
            }
        }

        self
    }

    pub fn get(&self) -> &As {
        self.snapshots.front().unwrap()
    }

    pub fn peek(&self, frame: i32) -> Option<&As> {
        let (index, _) = self
            .frames
            .iter()
            .enumerate()
            .find(|(_, &saved_frame)| saved_frame == frame)?;
        self.snapshots.get(index)
    }

    pub fn discard_old_snapshots(
        mut snapshots: ResMut<Self>,
        confirmed_frame: Option<Res<ConfirmedFrameCount>>,
        max_prediction_window: Option<Res<MaxPredictionWindow>>,
    ) where
        For: Send + Sync + 'static,
        As: Send + Sync + 'static,
    {
        if let Some(max_prediction_window) = max_prediction_window {
            snapshots.set_depth(max_prediction_window.0);
        }

        let Some(confirmed_frame) = confirmed_frame else {
            return;
        };

        snapshots.confirm(confirmed_frame.0);
    }
}

pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: HashMap<Rollback, As>,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsComponentSnapshot<For, As> {
    fn default() -> Self {
        Self {
            snapshot: default(),
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsComponentSnapshot<For, As> {
    pub fn new(components: impl IntoIterator<Item = (Rollback, As)>) -> Self {
        Self {
            snapshot: components.into_iter().collect(),
            ..default()
        }
    }

    pub fn insert(&mut self, entity: Rollback, snapshot: As) -> &mut Self {
        self.snapshot.insert(entity, snapshot);
        self
    }

    pub fn get(&self, entity: &Rollback) -> Option<&As> {
        self.snapshot.get(entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Rollback, &As)> + '_ {
        self.snapshot.iter()
    }
}
