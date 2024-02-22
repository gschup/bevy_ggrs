use crate::{ConfirmedFrameCount, Rollback, DEFAULT_FPS};
use bevy::{prelude::*, utils::HashMap};
use seahash::SeaHasher;
use std::{collections::VecDeque, marker::PhantomData};

mod checksum;
mod component_checksum;
mod component_map;
mod component_snapshot;
mod entity;
mod entity_checksum;
mod resource_checksum;
mod resource_map;
mod resource_snapshot;
mod rollback_entity_map;
mod set;
mod strategy;

pub use checksum::*;
pub use component_checksum::*;
pub use component_map::*;
pub use component_snapshot::*;
pub use entity::*;
pub use entity_checksum::*;
pub use resource_checksum::*;
pub use resource_map::*;
pub use resource_snapshot::*;
pub use rollback_entity_map::*;
pub use set::*;
pub use strategy::*;

pub mod prelude {
    pub use super::{Checksum, LoadWorldSet, SaveWorldSet};
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
    /// Separate from `frames`` to avoid padding.
    snapshots: VecDeque<As>,
    /// Queue of frames, newest at the front, oldest at the back.
    /// Separate from `snapshots`` to avoid padding.
    frames: VecDeque<i32>,
    /// Maximum amount of snapshots to store at any one time
    depth: usize,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsSnapshots<For, As> {
    fn default() -> Self {
        Self {
            snapshots: VecDeque::with_capacity(DEFAULT_FPS),
            frames: VecDeque::with_capacity(DEFAULT_FPS),
            depth: DEFAULT_FPS, // TODO: Make sensible choice here
            _phantom: default(),
        }
    }
}

impl<For, As> GgrsSnapshots<For, As> {
    /// Updates the capacity of this storage to the provided depth.
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

    /// Get the current capacity of this snapshot storage.
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Push a new snapshot for the provided frame. If the frame is earlier than any
    /// currently stored snapshots, those snapshots will be discarded.
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

            // Handle the possibility of wrapping i32
            let wrapped = current.abs_diff(frame) > u32::MAX / 2;
            let current_after_frame = current >= frame && !wrapped;
            let current_after_frame_wrapped = frame >= current && wrapped;

            if current_after_frame || current_after_frame_wrapped {
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

    /// Confirms a snapshot as being stable across clients. Snapshots from before this
    /// point are discarded as no longer required.
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

    /// Rolls back to the provided frame, discarding snapshots taken after the rollback point.
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

    /// Get the current snapshot. Use `rollback(frame)` to first select a frame to rollback to.
    pub fn get(&self) -> &As {
        self.snapshots.front().unwrap()
    }

    /// Get a particular snapshot if it exists.
    pub fn peek(&self, frame: i32) -> Option<&As> {
        let (index, _) = self
            .frames
            .iter()
            .enumerate()
            .find(|(_, &saved_frame)| saved_frame == frame)?;
        self.snapshots.get(index)
    }

    /// A system which automatically confirms the [`ConfirmedFrameCount`], discarding older snapshots.
    pub fn discard_old_snapshots(
        mut snapshots: ResMut<Self>,
        confirmed_frame: Option<Res<ConfirmedFrameCount>>,
    ) where
        For: Send + Sync + 'static,
        As: Send + Sync + 'static,
    {
        let Some(confirmed_frame) = confirmed_frame else {
            return;
        };

        snapshots.confirm(confirmed_frame.0);
    }
}

/// A storage type suitable for per-[`Entity`] snapshots, such as [`Component`] types.
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
    /// Create a new snapshot from a list of [`Rollback`] flags and stored [`Component`] types.
    pub fn new(components: impl IntoIterator<Item = (Rollback, As)>) -> Self {
        Self {
            snapshot: components.into_iter().collect(),
            ..default()
        }
    }

    /// Insert a single snapshot for the provided [`Rollback`].
    pub fn insert(&mut self, entity: Rollback, snapshot: As) -> &mut Self {
        self.snapshot.insert(entity, snapshot);
        self
    }

    /// Get a single snapshot for the provided [`Rollback`].
    pub fn get(&self, entity: &Rollback) -> Option<&As> {
        self.snapshot.get(entity)
    }

    /// Iterate over all stored snapshots.
    pub fn iter(&self) -> impl Iterator<Item = (&Rollback, &As)> + '_ {
        self.snapshot.iter()
    }
}

/// Returns a hasher built using the `seahash` library appropriate for creating portable checksums.
pub fn checksum_hasher() -> SeaHasher {
    SeaHasher::new()
}
