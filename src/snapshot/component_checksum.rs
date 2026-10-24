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

use bevy::{
    ecs::{change_detection::Tick, system::SystemChangeTick},
    platform::collections::HashMap,
    prelude::*,
};

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

/// A type's checksum as it was last taken: each entity's order and hash, by
/// [`RollbackId`], their XOR, and the tick it was taken at.
#[derive(Default)]
struct HeldChecksum {
    tick: Tick,
    hashes: HashMap<RollbackId, (u64, u64)>,
    xor: u64,
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
                           mut held: Local<HeldChecksum>,
                           ticks: SystemChangeTick| {
            let type_name = std::any::type_name::<C>();
            let this_run = ticks.this_run();

            // Rehash what changed since the last checksum, XORing its old hash
            // out and its new one in. An entry for an entity that no longer has
            // the component cannot be found that way: then start over.
            for pass in 0..2 {
                if pass == 1 {
                    held.hashes.clear();
                    held.xor = 0;
                }
                let mut count = 0;
                for (&rollback, component) in components.iter() {
                    count += 1;
                    let known = held.hashes.get(&rollback).map(|&(_, hash)| hash);
                    if known.is_some()
                        && !component.last_changed().is_newer_than(held.tick, this_run)
                    {
                        continue;
                    }
                    let mut hasher = checksum_hasher();
                    // Hashing the rollback index ensures this hash is unique and stable
                    let order = rollback_ordered.order(rollback);
                    order.hash(&mut hasher);
                    custom_hasher(&component).hash(&mut hasher);
                    let entity_hash = hasher.finish();
                    // XOR chosen over addition or multiplication as it is closed on u64 and
                    // commutative, and is its own inverse, so a changed entity's old hash
                    // comes back out
                    if let Some(old) = known {
                        held.xor ^= old;
                    }
                    held.xor ^= entity_hash;
                    held.hashes.insert(rollback, (order, entity_hash));
                }
                if held.hashes.len() == count {
                    break;
                }
            }
            held.tick = this_run;

            if let Some(ref diag) = diagnostics {
                for &(order, entity_hash) in held.hashes.values() {
                    diag.record_entity_hash(type_name, order, entity_hash);
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
