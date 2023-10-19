use bevy::{
    prelude::*,
    utils::HashMap,
};
use crate::Rollback;
use std::{collections::VecDeque, marker::PhantomData};

mod component_clone;
mod resource_clone;

pub use component_clone::*;
pub use resource_clone::*;

/// Collection of snapshots for a type `For`, stored as `As`
#[derive(Resource)]
pub struct GgrsSnapshots<For, As = For> {
    /// Queue of snapshots, newest at the front, oldest at the back.
    snapshots: VecDeque<As>,
    /// Queue of snapshots, newest at the front, oldest at the back.\
    /// Separate from snapshots to avoid padding.
    frames: VecDeque<i32>,
    /// Maximum amount of snapshots to store at any one time
    depth: usize,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsSnapshots<For, As> {
    fn default() -> Self {
        Self {
            snapshots: Default::default(),
            frames: Default::default(),
            depth: 60, // TODO: Make sensible choice here
            _phantom: Default::default(),
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
}

pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: HashMap<Rollback, As>,
    _phantom: PhantomData<For>,
}

impl<For, As> Default for GgrsComponentSnapshot<For, As> {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            _phantom: Default::default(),
        }
    }
}

impl<For, As> GgrsComponentSnapshot<For, As> {
    pub fn new(components: impl IntoIterator<Item = (Rollback, As)>) -> Self {
        Self {
            snapshot: components.into_iter().collect(),
            ..Default::default()
        }
    }

    pub fn insert(&mut self, entity: Rollback, snapshot: As) -> &mut Self {
        self.snapshot.insert(entity, snapshot);
        self
    }

    pub fn get(&self, entity: &Rollback) -> Option<&As> {
        self.snapshot.get(entity)
    }
}
