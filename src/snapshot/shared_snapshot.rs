//! Snapshots of per-entity data, shared per entity between frames.
//!
//! A type's snapshot ([`GgrsComponentSnapshot`]) is split into chunks by
//! [`RollbackId`]: the ids made from one run of [`CHUNK`] entity indices share
//! a chunk, and each chunk sits behind its own `Arc`. A save copies only the
//! chunks holding a component that changed, came or went since the snapshot
//! the world holds ([`HeldComponentSnapshot`]), and shares every other chunk
//! with it. A load writes only the chunks that differ from the ones the world
//! holds. An id keeps its chunk for life, so a spawn or a despawn moves no
//! other entity's.

use std::{marker::PhantomData, sync::Arc};

use bevy::ecs::change_detection::{CheckChangeTicks, Tick};

use crate::RollbackId;

/// How many entity indices a chunk spans, as a power of two.
const CHUNK_BITS: u32 = 4;

/// How many entity indices a chunk spans.
pub const CHUNK: usize = 1 << CHUNK_BITS;

/// The chunk `id` falls in. Entity indices are handed out densely, so the
/// chunk indices of a world are too.
#[inline(always)]
fn chunk_of(id: &RollbackId) -> usize {
    (id.index() >> CHUNK_BITS) as usize
}

/// The entries of one chunk, sorted by id.
type Chunk<As> = Arc<[(RollbackId, As)]>;

/// Sorts `entries` by id and moves them into a chunk, leaving `entries`
/// empty with its allocation kept.
fn seal<As>(entries: &mut Vec<(RollbackId, As)>) -> Chunk<As> {
    entries.sort_unstable_by_key(|(id, _)| *id);
    entries.drain(..).collect()
}

/// Whether two chunks are the same one, shared: both absent, or one `Arc`.
fn same_chunk<As>(a: Option<&Chunk<As>>, b: Option<&Chunk<As>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

/// A storage type suitable for per-[`Entity`](bevy::prelude::Entity)
/// snapshots, such as [`Component`](bevy::prelude::Component) types.
///
/// The data is shared: cloning a snapshot is an `Arc` clone, and two snapshots
/// of the same type share every chunk nothing changed in between (module
/// docs).
pub struct GgrsComponentSnapshot<For, As = For> {
    snapshot: Arc<Chunks<As>>,
    _phantom: PhantomData<For>,
}

struct Chunks<As> {
    /// By chunk index; `None` where no entity of the chunk is held.
    chunks: Vec<Option<Chunk<As>>>,
    len: usize,
}

impl<As> Default for Chunks<As> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<As> Clone for Chunks<As> {
    fn clone(&self) -> Self {
        Self {
            chunks: self.chunks.clone(),
            len: self.len,
        }
    }
}

impl<As> Chunks<As> {
    fn chunk(&self, index: usize) -> Option<&Chunk<As>> {
        self.chunks.get(index).and_then(Option::as_ref)
    }

    fn chunk_len(&self, index: usize) -> usize {
        self.chunk(index).map_or(0, |chunk| chunk.len())
    }
}

impl<For, As> Default for GgrsComponentSnapshot<For, As> {
    fn default() -> Self {
        Self {
            snapshot: default(),
            _phantom: PhantomData,
        }
    }
}

impl<For, As> Clone for GgrsComponentSnapshot<For, As> {
    fn clone(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            _phantom: PhantomData,
        }
    }
}

fn default<T: Default>() -> T {
    T::default()
}

impl<For, As> GgrsComponentSnapshot<For, As> {
    /// Create a new snapshot from a list of [`RollbackId`]s and stored values.
    /// A later entry for an id replaces an earlier one.
    pub fn new(components: impl IntoIterator<Item = (RollbackId, As)>) -> Self {
        let mut entries: Vec<Vec<(RollbackId, As)>> = Vec::new();
        for (id, stored) in components {
            let index = chunk_of(&id);
            if index >= entries.len() {
                entries.resize_with(index + 1, Vec::new);
            }
            entries[index].push((id, stored));
        }
        let mut len = 0;
        let chunks = entries
            .iter_mut()
            .map(|entries| {
                // The stable sort keeps a later entry after an earlier one for the
                // same id, so reversed first, the dedup keeps the later one.
                entries.reverse();
                entries.sort_by_key(|(id, _)| *id);
                entries.dedup_by_key(|(id, _)| *id);
                len += entries.len();
                (!entries.is_empty()).then(|| seal(entries))
            })
            .collect();
        Self {
            snapshot: Arc::new(Chunks { chunks, len }),
            _phantom: PhantomData,
        }
    }

    /// Get a single snapshot for the provided [`RollbackId`].
    pub fn get(&self, id: &RollbackId) -> Option<&As> {
        find(self.snapshot.chunk(chunk_of(id))?, id)
    }

    /// Iterate over all stored snapshots, in chunk order, by id within a chunk.
    pub fn iter(&self) -> impl Iterator<Item = (&RollbackId, &As)> + '_ {
        self.snapshot
            .chunks
            .iter()
            .flatten()
            .flat_map(|chunk| chunk.iter().map(|(id, stored)| (id, stored)))
    }

    /// How many entities the snapshot holds.
    pub fn len(&self) -> usize {
        self.snapshot.len
    }

    /// Whether the snapshot holds no entity.
    pub fn is_empty(&self) -> bool {
        self.snapshot.len == 0
    }

    /// Whether `self` and `other` are the same snapshot, shared, not two
    /// that happen to hold equal data.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.snapshot, &other.snapshot)
    }

    /// How many of `self`'s chunks are shared with `other`, and how many
    /// chunks `self` holds.
    pub fn chunks_shared_with(&self, other: &Self) -> (usize, usize) {
        let ours = self.snapshot.chunks.iter().flatten().count();
        let shared = self
            .snapshot
            .chunks
            .iter()
            .enumerate()
            .filter(|(index, chunk)| {
                chunk.is_some() && same_chunk(chunk.as_ref(), other.snapshot.chunk(*index))
            })
            .count();
        (shared, ours)
    }
}

/// The entry for `id` in `chunk`.
#[inline]
fn find<'a, As>(chunk: &'a Chunk<As>, id: &RollbackId) -> Option<&'a As> {
    chunk
        .binary_search_by_key(id, |(id, _)| *id)
        .ok()
        .map(|at| &chunk[at].1)
}

impl<For, As: Clone> GgrsComponentSnapshot<For, As> {
    /// Insert a single snapshot for the provided [`RollbackId`], replacing any
    /// it held. Copies the chunk first, and the chunk list if another snapshot
    /// shares it.
    pub fn insert(&mut self, id: RollbackId, stored: As) -> &mut Self {
        let chunks = Arc::make_mut(&mut self.snapshot);
        let index = chunk_of(&id);
        if index >= chunks.chunks.len() {
            chunks.chunks.resize(index + 1, None);
        }
        let mut entries: Vec<(RollbackId, As)> = chunks.chunks[index]
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|(held, _)| *held != id)
            .cloned()
            .collect();
        let replaced = chunks.chunk_len(index) != entries.len();
        entries.push((id, stored));
        chunks.chunks[index] = Some(seal(&mut entries));
        if !replaced {
            chunks.len += 1;
        }
        self
    }
}

/// What a scan of the world found in one chunk.
#[derive(Clone, Copy, Default)]
struct ChunkScan {
    /// How many of the world's components fall in the chunk.
    count: u32,
    /// Whether one of them is newer than the held tick.
    changed: bool,
    /// Whether the chunk is shared: kept from the held snapshot by a save, left
    /// alone by a load.
    shared: bool,
}

/// The snapshot of `For` the world's components equal, and the tick from
/// which that holds: the last save, or the last load.
///
/// A save or load checks it chunk by chunk. A chunk whose components are all
/// no newer than the tick, and as many as the held chunk holds (so none was
/// removed, none added), still equals the held chunk: a save shares it
/// instead of copying, and a load of a frame that shares it writes nothing
/// there. A load that wrote nothing leaves no change behind, so the ticks it
/// resimulates share too.
#[derive(bevy::prelude::Resource)]
pub struct HeldComponentSnapshot<For, As = For> {
    held: Option<(GgrsComponentSnapshot<For, As>, Tick)>,
    /// Per chunk, what the last scan found. Kept between runs for its
    /// allocation, as `copies` is.
    scan: Vec<ChunkScan>,
    /// Per chunk, the values a save copies.
    copies: Vec<Vec<(RollbackId, As)>>,
}

impl<For, As> Default for HeldComponentSnapshot<For, As> {
    fn default() -> Self {
        Self {
            held: None,
            scan: Vec::new(),
            copies: Vec::new(),
        }
    }
}

/// What a load does with one entity's component.
pub enum Restore<'a, As> {
    /// Its chunk is the one the world holds: nothing to write.
    Shared,
    /// The snapshot holds none for it: remove it.
    Absent,
    /// The snapshot holds this for it.
    Stored(&'a As),
}

/// A load of one snapshot, after [`HeldComponentSnapshot::load`] scanned the
/// world: which chunks it writes, and what it writes there.
pub struct LoadPlan<'a, For, As> {
    snapshot: &'a GgrsComponentSnapshot<For, As>,
    scan: &'a [ChunkScan],
    /// How many entries of the snapshot fall in chunks it writes.
    to_write: usize,
}

impl<'a, For, As> LoadPlan<'a, For, As> {
    /// What the load does with the component of the entity `id`.
    #[inline]
    pub fn restore(&self, id: &RollbackId) -> Restore<'a, As> {
        let index = chunk_of(id);
        if self.scan.get(index).is_some_and(|scan| scan.shared) {
            return Restore::Shared;
        }
        match self.snapshot.get(id) {
            Some(stored) => Restore::Stored(stored),
            None => Restore::Absent,
        }
    }

    /// The snapshot's value for an entity the world holds none for, unless
    /// its chunk is shared.
    #[inline]
    pub fn missing(&self, id: &RollbackId) -> Option<&'a As> {
        match self.restore(id) {
            Restore::Stored(stored) => Some(stored),
            Restore::Shared | Restore::Absent => None,
        }
    }

    /// Whether, with `matched` of the world's components found in the
    /// snapshot, it holds some for entities that have none: they then need
    /// [`missing`](Self::missing) for each entity without one.
    pub fn holds_more(&self, matched: usize) -> bool {
        matched < self.to_write
    }

    /// How many entries of the snapshot the load writes or checks.
    pub fn to_write(&self) -> usize {
        self.to_write
    }
}

impl<For, As> HeldComponentSnapshot<For, As> {
    /// Whether the world still equals the held snapshot as a whole: no
    /// component newer than its tick, and as many as it holds. The cheap check
    /// first, since most types change in no tick: it reads no id.
    fn unchanged_whole(&self, ticks: impl Iterator<Item = Tick>, this_run: Tick) -> bool {
        let Some((held, tick)) = &self.held else {
            return false;
        };
        // A fold, not a loop with an early exit: a query's fold walks its
        // tables directly, several times faster than `next`.
        let (count, newer) = ticks.fold((0, false), |(count, newer), changed| {
            (count + 1, newer | changed.is_newer_than(*tick, this_run))
        });
        !newer && count == held.len()
    }

    /// Fill `scan`, chunk by chunk, from every component of the type the world
    /// holds: `ticks` is each one's id and change tick.
    fn scan(&mut self, ticks: impl Iterator<Item = (RollbackId, Tick)>, this_run: Tick) {
        self.scan.clear();
        let held_tick = self.held.as_ref().map(|(_, tick)| *tick);
        let chunks = &mut self.scan;
        ticks.for_each(|(id, changed)| {
            let index = chunk_of(&id);
            if index >= chunks.len() {
                chunks.resize(index + 1, ChunkScan::default());
            }
            let scan = &mut chunks[index];
            scan.count += 1;
            scan.changed |= held_tick.is_none_or(|tick| changed.is_newer_than(tick, this_run));
        });
        let held_chunks = self
            .held
            .as_ref()
            .map_or(0, |(held, _)| held.snapshot.chunks.len());
        if self.scan.len() < held_chunks {
            self.scan.resize(held_chunks, ChunkScan::default());
        }
    }

    /// Whether chunk `index` of the world still equals the held one, after a
    /// [`scan`](Self::scan).
    fn unchanged(&self, index: usize) -> bool {
        let Some((held, _)) = &self.held else {
            return false;
        };
        let scan = self.scan[index];
        !scan.changed && scan.count as usize == held.snapshot.chunk_len(index)
    }

    /// The snapshot of the world's components of the type, which the world
    /// equals from `this_run` on.
    ///
    /// Three walks over every component of the type, each only when the one
    /// before found a change: `changed` is each one's change tick, `ticks`
    /// its id and change tick, `values` its id and what `store` makes its
    /// stored value of. Only the changed chunks' values are stored. Nothing
    /// changed, the held snapshot is returned again, shared.
    ///
    /// `changed` reads no id, so it is the cheap walk: a query of the change
    /// ticks alone, not one that fetches each [`RollbackId`] and drops it.
    pub fn save<R>(
        &mut self,
        changed: impl Iterator<Item = Tick>,
        ticks: impl Iterator<Item = (RollbackId, Tick)>,
        values: impl Iterator<Item = (RollbackId, R)>,
        store: impl Fn(R) -> As,
        this_run: Tick,
    ) -> GgrsComponentSnapshot<For, As> {
        if self.unchanged_whole(changed, this_run)
            && let Some((held, _)) = &self.held
        {
            return held.clone();
        }
        self.scan(ticks, this_run);
        let mut all_shared = true;
        for index in 0..self.scan.len() {
            let shared = self.unchanged(index);
            self.scan[index].shared = shared;
            all_shared &= shared;
        }
        if all_shared && let Some((held, _)) = &self.held {
            return held.clone();
        }

        if self.copies.len() < self.scan.len() {
            self.copies.resize_with(self.scan.len(), Vec::new);
        }
        let (scan, copies) = (&self.scan, &mut self.copies);
        values.for_each(|(id, value)| {
            let index = chunk_of(&id);
            if !scan[index].shared {
                copies[index].push((id, store(value)));
            }
        });
        let held = self.held.as_ref().map(|(held, _)| &held.snapshot);
        let last = self.scan.iter().rposition(|scan| scan.count > 0);
        let chunks = (0..last.map_or(0, |last| last + 1))
            .map(|index| {
                if self.scan[index].shared {
                    held.and_then(|held| held.chunk(index)).cloned()
                } else if self.copies[index].is_empty() {
                    None
                } else {
                    Some(seal(&mut self.copies[index]))
                }
            })
            .collect();
        let len = self.scan.iter().map(|scan| scan.count as usize).sum();
        let snapshot = GgrsComponentSnapshot {
            snapshot: Arc::new(Chunks { chunks, len }),
            _phantom: PhantomData,
        };
        self.held = Some((snapshot.clone(), this_run));
        snapshot
    }

    /// Plan a load of `snapshot`: `changed` is the change tick of every
    /// component of the type the world holds, `ticks` each one's id and change
    /// tick, walked only when the snapshot is not the held one or the world
    /// changed since ([`save`](Self::save)). `None` when every chunk is the
    /// one the world holds and nothing is to be written.
    ///
    /// The world equals `snapshot` from `this_run` on, once the load has
    /// written what the plan says: the held snapshot is set to it here.
    pub fn load<'a>(
        &'a mut self,
        snapshot: &'a GgrsComponentSnapshot<For, As>,
        changed: impl Iterator<Item = Tick>,
        ticks: impl Iterator<Item = (RollbackId, Tick)>,
        this_run: Tick,
    ) -> Option<LoadPlan<'a, For, As>> {
        if self
            .held
            .as_ref()
            .is_some_and(|(held, _)| held.same(snapshot))
            && self.unchanged_whole(changed, this_run)
        {
            return None;
        }
        self.scan(ticks, this_run);
        if self.scan.len() < snapshot.snapshot.chunks.len() {
            self.scan
                .resize(snapshot.snapshot.chunks.len(), ChunkScan::default());
        }
        let mut to_write = 0;
        for index in 0..self.scan.len() {
            let shared = self.unchanged(index)
                && self.held.as_ref().is_some_and(|(held, _)| {
                    same_chunk(held.snapshot.chunk(index), snapshot.snapshot.chunk(index))
                });
            self.scan[index].shared = shared;
            if !shared {
                to_write += snapshot.snapshot.chunk_len(index);
            }
        }
        let anything = self.scan.iter().any(|scan| !scan.shared);
        self.held = Some((snapshot.clone(), this_run));
        anything.then_some(LoadPlan {
            snapshot,
            scan: &self.scan,
            to_write,
        })
    }

    /// The world equals `snapshot` from `tick` on.
    pub fn set(&mut self, snapshot: GgrsComponentSnapshot<For, As>, tick: Tick) {
        self.held = Some((snapshot, tick));
    }

    /// Forget the held snapshot: the next save copies.
    pub fn clear(&mut self) {
        self.held = None;
    }

    /// Keep the held tick comparable as the world's change tick wraps: a
    /// `Tick` stored outside a component is not kept by Bevy, and after
    /// about 2^32 ticks a change would read as older than it. Bevy triggers
    /// [`CheckChangeTicks`] for this; the snapshot plugins observe it.
    pub fn check_change_ticks(&mut self, check: CheckChangeTicks) {
        if let Some((_, tick)) = &mut self.held {
            tick.check_tick(check);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;

    use super::*;

    fn id(index: u32) -> RollbackId {
        RollbackId::new(Entity::from_raw_u32(index).unwrap())
    }

    fn tick(n: u32) -> Tick {
        Tick::new(n)
    }

    type Snapshot = GgrsComponentSnapshot<u32, u32>;
    type Held = HeldComponentSnapshot<u32, u32>;

    /// A save of `world` (id index, value, change tick) at `now`.
    fn save(held: &mut Held, world: &[(u32, u32, u32)], now: u32) -> Snapshot {
        held.save(
            world.iter().map(|&(_, _, t)| tick(t)),
            world.iter().map(|&(i, _, t)| (id(i), tick(t))),
            world.iter().map(|&(i, v, _)| (id(i), v)),
            |v| v,
            tick(now),
        )
    }

    /// By index: the order of `iter` within a chunk is `RollbackId`'s, which
    /// is not the index's.
    fn values(snapshot: &Snapshot) -> Vec<(u32, u32)> {
        let mut values: Vec<_> = snapshot.iter().map(|(id, v)| (id.index(), *v)).collect();
        values.sort();
        values
    }

    #[test]
    fn new_gets_iterates_and_replaces() {
        let snapshot = Snapshot::new([(id(40), 1), (id(3), 2), (id(40), 3), (id(17), 4)]);
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot.get(&id(40)), Some(&3));
        assert_eq!(snapshot.get(&id(5)), None);
        assert_eq!(values(&snapshot), vec![(3, 2), (17, 4), (40, 3)]);
    }

    #[test]
    fn insert_adds_and_replaces_without_touching_a_sharer() {
        let mut snapshot = Snapshot::new([(id(1), 1)]);
        let before = snapshot.clone();
        snapshot
            .insert(id(2), 2)
            .insert(id(1), 10)
            .insert(id(100), 100);
        assert_eq!(snapshot.len(), 3);
        assert_eq!(values(&snapshot), vec![(1, 10), (2, 2), (100, 100)]);
        assert_eq!(values(&before), vec![(1, 1)]);
    }

    /// One chunk changed: it is copied, every other is the previous save's.
    #[test]
    fn a_save_copies_only_the_chunk_that_changed() {
        let mut held = Held::default();
        let mut world: Vec<(u32, u32, u32)> = (0..64).map(|i| (i, i, 1)).collect();
        let first = save(&mut held, &world, 2);
        assert_eq!(first.chunks_shared_with(&Snapshot::default()), (0, 4));

        let again = save(&mut held, &world, 3);
        assert!(
            again.same(&first),
            "nothing changed: the whole snapshot is shared"
        );

        world[20] = (20, 99, 4);
        let changed = save(&mut held, &world, 5);
        assert_eq!(changed.chunks_shared_with(&first), (3, 4));
        assert_eq!(changed.get(&id(20)), Some(&99));
        assert_eq!(first.get(&id(20)), Some(&20));
    }

    /// A removal changes no tick: the chunk's count catches it.
    #[test]
    fn a_save_copies_a_chunk_that_lost_a_component() {
        let mut held = Held::default();
        let mut world: Vec<(u32, u32, u32)> = (0..32).map(|i| (i, i, 1)).collect();
        let first = save(&mut held, &world, 2);
        world.retain(|&(i, _, _)| i != 3);
        let after = save(&mut held, &world, 3);
        assert_eq!(after.len(), 31);
        assert_eq!(after.get(&id(3)), None);
        assert_eq!(after.chunks_shared_with(&first), (1, 2));
    }

    /// A chunk the world no longer holds anything in is dropped.
    #[test]
    fn a_save_drops_an_emptied_chunk() {
        let mut held = Held::default();
        let world = [(1, 1, 1), (40, 40, 1)];
        save(&mut held, &world, 2);
        let after = save(&mut held, &world[..1], 3);
        assert_eq!(values(&after), vec![(1, 1)]);
        assert_eq!(after.snapshot.chunks.len(), 1);
    }

    /// A load of the held snapshot writes nothing; a load of an older one
    /// writes the chunks that differ and nothing else.
    #[test]
    fn a_load_writes_only_the_chunks_that_differ() {
        let mut held = Held::default();
        let mut world: Vec<(u32, u32, u32)> = (0..64).map(|i| (i, i, 1)).collect();
        let old = save(&mut held, &world, 2);
        world[20] = (20, 99, 3);
        let new = save(&mut held, &world, 4);
        let ticks = |world: &[(u32, u32, u32)]| {
            world
                .iter()
                .map(|&(i, _, t)| (id(i), tick(t)))
                .collect::<Vec<_>>()
        };

        let changed = |world: &[(u32, u32, u32)]| {
            world
                .iter()
                .map(|&(_, _, t)| tick(t))
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert!(
            held.load(&new, changed(&world), ticks(&world).into_iter(), tick(5))
                .is_none()
        );

        let plan = held
            .load(&old, changed(&world), ticks(&world).into_iter(), tick(6))
            .unwrap();
        assert_eq!(plan.to_write(), 16);
        assert!(matches!(plan.restore(&id(20)), Restore::Stored(&20)));
        assert!(matches!(plan.restore(&id(40)), Restore::Shared));
        assert!(!plan.holds_more(16));
    }

    /// A component changed since the held tick makes its chunk load, even
    /// onto the held snapshot.
    #[test]
    fn a_load_writes_a_chunk_changed_since_the_save() {
        let mut held = Held::default();
        let world: Vec<(u32, u32, u32)> = (0..32).map(|i| (i, i, 1)).collect();
        let saved = save(&mut held, &world, 2);
        let mut ticks: Vec<_> = world.iter().map(|&(i, _, t)| (id(i), tick(t))).collect();
        ticks[18].1 = tick(3);
        let changed: Vec<_> = ticks.iter().map(|(_, t)| *t).collect();
        let plan = held
            .load(&saved, changed.into_iter(), ticks.into_iter(), tick(4))
            .unwrap();
        assert!(matches!(plan.restore(&id(18)), Restore::Stored(&18)));
        assert!(matches!(plan.restore(&id(2)), Restore::Shared));
    }

    /// An entity of a written chunk that the world lacks the component on is
    /// found by `missing`, and counted in `holds_more`.
    #[test]
    fn a_load_finds_what_the_world_lacks() {
        let mut held = Held::default();
        let world: Vec<(u32, u32, u32)> = (0..32).map(|i| (i, i, 1)).collect();
        let saved = save(&mut held, &world, 2);
        let ticks = world
            .iter()
            .filter(|&&(i, _, _)| i != 5)
            .map(|&(i, _, t)| (id(i), tick(t)));
        let plan = held
            .load(&saved, ticks.clone().map(|(_, t)| t), ticks, tick(3))
            .unwrap();
        assert!(plan.holds_more(15));
        assert_eq!(plan.missing(&id(5)), Some(&5));
        assert_eq!(plan.missing(&id(20)), None, "its chunk is shared");
    }

    /// The runner's order: a save, a tick that removes a component and is not
    /// saved, then a load that lifts a deleted entity in the same chunk. The
    /// chunk holds as many components as the held one, none newer, yet it is
    /// not the frame's: the load writes it.
    #[test]
    fn a_load_that_lifts_a_deleted_entity_restores_a_removal_beside_it() {
        use crate::{
            Rollback, RollbackApp, SnapshotPlugin, despawn_rollback,
            snapshot::tests::{advance_frame, load_world, save_world},
        };
        use bevy::prelude::*;

        #[derive(Component, Clone, Copy, PartialEq, Debug)]
        struct Value(u32);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(SnapshotPlugin)
            .rollback_component_with_copy::<Value>();
        app.finish();
        app.cleanup();
        app.update();
        let world = app.world_mut();
        let [a, b, c] = [1, 2, 3].map(|v| world.spawn((Rollback, Value(v))).id());
        world.flush();
        let chunks: Vec<usize> = [a, b, c]
            .map(|e| chunk_of(world.get::<RollbackId>(e).unwrap()))
            .into();
        assert!(
            chunks.iter().all(|&k| k == chunks[0]),
            "one chunk: {chunks:?}"
        );

        save_world(world); // frame 0
        advance_frame(world);
        despawn_rollback(world.entity_mut(a)); // frame 1 lacks it
        save_world(world); // frame 1
        advance_frame(world);
        world.entity_mut(b).remove::<Value>(); // frame 2, not saved
        load_world(world, 0);

        let mut values: Vec<(Entity, u32)> = world
            .query::<(Entity, &Value)>()
            .iter(world)
            .map(|(e, v)| (e, v.0))
            .collect();
        values.sort_by_key(|(_, v)| *v);
        assert_eq!(values, vec![(a, 1), (b, 2), (c, 3)]);
    }
}
