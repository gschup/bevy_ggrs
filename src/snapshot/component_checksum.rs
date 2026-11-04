//! Per-frame checksum tracking for [`Component`] types on rollback entities.
//!
//! [`ComponentChecksumPlugin`] hashes each rollback entity's component value (combined with
//! its stable [`RollbackId`] order) and XORs the results into a [`ChecksumPart`] so that
//! component desyncs are detected by GGRS's checksum comparison.
//!
//! Each type keeps its entities' hashes from the last checksum and rehashes
//! only the components changed since, so a tick costs what changed in it, not
//! what exists: the world holds the same values elsewhere, so their hashes are
//! the same. It is the rule the held snapshots go by
//! ([`HeldComponentSnapshot`](crate::HeldComponentSnapshot)), and it leans on
//! change detection the same way: a write that does not mark the component
//! changed is missed by both.

use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::{
    ChecksumDiagnostics, ChecksumFlag, ChecksumPart, RollbackId, RollbackOrdered, SaveWorld,
    SaveWorldSystems, checksum_hasher,
};

/// A [`Plugin`] which will track the [`Component`] `C` on [`Rollback`](`crate::Rollback`) entities and ensure a
/// [`ChecksumPart`] is available and updated. This can be used to generate a [`Checksum`](`crate::Checksum`).
///
/// # Examples
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_ggrs::{prelude::*, ComponentChecksumPlugin};
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
/// app.add_plugins(ComponentChecksumPlugin::<Health>::default());
/// # }
/// ```
pub struct ComponentChecksumPlugin<C: Component>(pub for<'a> fn(&'a C) -> u64);

/// A type's checksum as it was last taken: each entity's hash, their XOR,
/// and a count of the passes taken.
///
/// The hashes are a dense list of the type's entities, found through a table
/// by entity index, not a map: every pass visits every entity of the type, so
/// the lookup is paid per entity per tick, and two indexed reads are several
/// times cheaper than a hash lookup. The table holds a `u32` per entity index
/// up to the highest the type has seen; Bevy reuses a freed index, so that
/// follows the world's peak entity count. A pass that finds an entity gone
/// walks the list, the type's own count, never the table.
#[derive(Default)]
struct HeldChecksum {
    hashes: Vec<EntityHash>,
    /// By entity index, the entity's position in `hashes` plus one; 0 for none.
    slots: Vec<u32>,
    xor: u64,
    pass: u32,
}

/// One entity's part of a [`HeldChecksum`], and the pass that last found it.
/// The id tells it from an entity that held the index before.
struct EntityHash {
    id: RollbackId,
    order: u64,
    hash: u64,
    seen: u32,
}

impl HeldChecksum {
    /// Start a pass over the world's components of the type.
    fn begin(&mut self) {
        self.pass = self.pass.wrapping_add(1);
    }

    /// Find the entity `id` in this pass: rehash it with `hash` (its order and
    /// its hash) when it `changed` or is not held, else keep its hash.
    #[inline]
    fn visit(&mut self, id: RollbackId, changed: bool, hash: impl FnOnce() -> (u64, u64)) {
        let index = id.index() as usize;
        if index >= self.slots.len() {
            self.slots.resize(index + 1, 0);
        }
        let held = self.slots[index].checked_sub(1).map(|at| at as usize);
        if let Some(at) = held {
            let entry = &mut self.hashes[at];
            if entry.id == id && !changed {
                entry.seen = self.pass;
                return;
            }
        }
        let (order, hash) = hash();
        // XOR chosen over addition or multiplication as it is closed on u64 and
        // commutative, and is its own inverse, so a changed entity's old hash
        // comes back out. An entry under another id is a despawned entity's
        // whose index was reused: its hash comes out too.
        let entry = EntityHash {
            id,
            order,
            hash,
            seen: self.pass,
        };
        match held {
            Some(at) => {
                self.xor ^= self.hashes[at].hash;
                self.hashes[at] = entry;
            }
            None => {
                self.hashes.push(entry);
                self.slots[index] = self.hashes.len() as u32;
            }
        }
        self.xor ^= hash;
    }

    /// End a pass that found `count` entities: one held that the pass did not
    /// find no longer has the component, and its hash comes back out.
    fn end(&mut self, count: usize) {
        if self.hashes.len() == count {
            return;
        }
        let mut at = 0;
        while at < self.hashes.len() {
            if self.hashes[at].seen == self.pass {
                at += 1;
                continue;
            }
            let gone = self.hashes.swap_remove(at);
            self.xor ^= gone.hash;
            self.slots[gone.id.index() as usize] = 0;
            if let Some(moved) = self.hashes.get(at) {
                self.slots[moved.id.index() as usize] = at as u32 + 1;
            }
        }
    }
}

fn default_hasher<C: Component + Hash>(component: &C) -> u64 {
    let mut hasher = checksum_hasher();
    component.hash(&mut hasher);
    hasher.finish()
}

impl<C> Default for ComponentChecksumPlugin<C>
where
    C: Component + Hash,
{
    fn default() -> Self {
        Self(default_hasher::<C>)
    }
}

impl<C> Plugin for ComponentChecksumPlugin<C>
where
    C: Component,
{
    /// Registers the checksum update system for this component type in [`SaveWorldSystems::Checksum`].
    fn build(&self, app: &mut App) {
        let custom_hasher = self.0;

        let update = move |mut commands: Commands,
                           rollback_ordered: Res<RollbackOrdered>,
                           components: Query<
            (&RollbackId, Ref<C>),
            (With<RollbackId>, Without<ChecksumFlag<C>>),
        >,
                           mut checksum: Query<
            &mut ChecksumPart,
            (Without<RollbackId>, With<ChecksumFlag<C>>),
        >,
                           diagnostics: Option<Res<ChecksumDiagnostics>>,
                           mut held: Local<HeldChecksum>| {
            let type_name = std::any::type_name::<C>();
            let held = &mut *held;
            held.begin();

            // Rehash what changed since the last checksum (this system runs on
            // every save, so its last run is that checksum). A fold, not a
            // loop: a query's fold walks its tables directly.
            let count = components.iter().fold(0, |count, (&rollback, component)| {
                held.visit(rollback, component.is_changed(), || {
                    let mut hasher = checksum_hasher();
                    // Hashing the rollback index ensures this hash is unique and stable
                    let order = rollback_ordered.order(rollback);
                    order.hash(&mut hasher);
                    custom_hasher(&component).hash(&mut hasher);
                    (order, hasher.finish())
                });
                count + 1
            });
            held.end(count);

            if let Some(ref diag) = diagnostics {
                for entry in &held.hashes {
                    diag.record_entity_hash(type_name, entry.order, entry.hash);
                }
            }

            // Hash the XOR'ed result to break commutativity with other types
            let mut hasher = checksum_hasher();
            held.xor.hash(&mut hasher);

            let result = ChecksumPart(hasher.finish() as u128);

            trace!(
                "Component {} has checksum {:X}",
                disqualified::ShortName::of::<C>(),
                result.0
            );

            if let Some(ref diag) = diagnostics {
                diag.record_type_checksum(type_name, result.0);
            }

            if let Ok(mut checksum) = checksum.single_mut() {
                *checksum = result;
            } else {
                commands.spawn((result, ChecksumFlag::<C>::default()));
            }
        };

        app.add_systems(SaveWorld, update.in_set(SaveWorldSystems::Checksum));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(index: u32, generation: u32) -> RollbackId {
        let entity = Entity::from_raw_u32(index).unwrap();
        let entity = Entity::from_index_and_generation(
            entity.index(),
            bevy::ecs::entity::EntityGeneration::FIRST.after_versions(generation),
        );
        RollbackId::new(entity)
    }

    /// The XOR of `entries`' hashes, as a checksum taken fresh holds it.
    fn fresh(entries: &[(RollbackId, u64)]) -> u64 {
        let mut held = HeldChecksum::default();
        held.begin();
        for &(id, hash) in entries {
            held.visit(id, true, || (0, hash));
        }
        held.end(entries.len());
        held.xor
    }

    /// An entity despawned and its index handed to a new one between two
    /// passes: the old hash comes out, the new one goes in.
    #[test]
    fn a_reused_index_replaces_the_old_hash() {
        let mut held = HeldChecksum::default();
        held.begin();
        held.visit(id(3, 0), true, || (0, 0xa));
        held.visit(id(4, 0), true, || (1, 0xb));
        held.end(2);

        held.begin();
        held.visit(id(3, 1), false, || (2, 0xc));
        held.visit(id(4, 0), false, || unreachable!("unchanged and held"));
        held.end(2);

        assert_eq!(held.hashes.len(), 2);
        assert_eq!(held.xor, fresh(&[(id(3, 1), 0xc), (id(4, 0), 0xb)]));
    }

    /// One entity lost the component and another came back in the same pass:
    /// the count is the same, the departed hash still comes out.
    #[test]
    fn a_departure_beside_an_arrival_comes_out() {
        let mut held = HeldChecksum::default();
        held.begin();
        held.visit(id(1, 0), true, || (0, 0x1));
        held.visit(id(2, 0), true, || (1, 0x2));
        held.end(2);

        held.begin();
        held.visit(id(1, 0), false, || unreachable!());
        held.visit(id(9, 0), false, || (2, 0x9));
        held.end(2);

        assert_eq!(held.hashes.len(), 2);
        assert_eq!(held.xor, fresh(&[(id(1, 0), 0x1), (id(9, 0), 0x9)]));
    }

    /// Two entities gone from the middle of the list: the ones moved into
    /// their places are still found where they are, not rehashed.
    #[test]
    fn a_departure_keeps_the_rest_found() {
        let mut held = HeldChecksum::default();
        held.begin();
        for i in 0..6 {
            held.visit(id(i * 7, 0), true, || (i as u64, 1 << i));
        }
        held.end(6);

        held.begin();
        for i in [0, 2, 3, 5] {
            held.visit(id(i * 7, 0), false, || unreachable!("unchanged and held"));
        }
        held.end(4);

        held.begin();
        for i in [0, 2, 3, 5] {
            held.visit(id(i * 7, 0), false, || unreachable!("found after the swap"));
        }
        held.end(4);

        assert_eq!(held.hashes.len(), 4);
        assert_eq!(
            held.xor,
            fresh(&[
                (id(0, 0), 1),
                (id(14, 0), 4),
                (id(21, 0), 8),
                (id(35, 0), 32)
            ])
        );
    }
}
