//! One insert per entity for the components a rollback adds.
//!
//! A `load` system updates a component the entity already has in place and removes one the
//! snapshot lacks. A component the entity lacks, which is every one of them on a respawned
//! entity, it does not insert itself: it stages the value with [`stage_insert`], and
//! [`insert_staged_components`] inserts all of an entity's staged values as one dynamic bundle
//! at the end of [`LoadWorldSystems::ComponentDataFlush`](crate::LoadWorldSystems).
//!
//! One insert means `#[require]` fills nothing a snapshot provides, and every hook runs once,
//! after all the snapshot values are written. Inserted one type at a time, a component whose
//! required component's snapshot had not landed yet got a placeholder default first, and that
//! placeholder's hook ran: a physics body index taking a number from a counter, for one.

use std::{
    alloc::{Layout, dealloc},
    any::Any,
    ptr::NonNull,
};

use bevy::{
    ecs::component::ComponentId, platform::collections::HashMap, prelude::*, ptr::OwningPtr,
};

use crate::RollbackId;

/// The component values a [`LoadWorld`](crate::LoadWorld) has staged for insertion, per entity.
///
/// Filled by [`stage_insert`] commands during
/// [`LoadWorldSystems::ComponentData`](crate::LoadWorldSystems), drained by
/// [`insert_staged_components`].
#[derive(Resource, Default)]
pub struct StagedInserts {
    entities: HashMap<Entity, StagedEntity>,
}

/// The values staged for one entity.
struct StagedEntity {
    rollback: RollbackId,
    /// Each box holds a value of the type its [`ComponentId`] was registered for.
    components: Vec<(ComponentId, Box<dyn Any + Send + Sync>)>,
}

/// A [`Command`] that stages `value` for insertion on `entity` by the next
/// [`insert_staged_components`].
pub fn stage_insert<T: Component>(
    entity: Entity,
    rollback: RollbackId,
    value: T,
) -> impl Command<Out = ()> {
    move |world: &mut World| {
        let id = world.register_component::<T>();
        world
            .resource_mut::<StagedInserts>()
            .entities
            .entry(entity)
            .or_insert_with(|| StagedEntity {
                rollback,
                components: Vec::new(),
            })
            .components
            .push((id, Box::new(value)));
    }
}

/// Inserts every entity's staged components, one insert per entity.
///
/// Runs after the [`ApplyDeferred`] of
/// [`LoadWorldSystems::ComponentDataFlush`](crate::LoadWorldSystems), so the removals and
/// the re-inserts the load systems queued have landed, and a hook here sees the entity as the
/// snapshot had it. Entities go in [`RollbackId`] order: a hook may touch shared state, and the
/// map's order is not simulated.
pub fn insert_staged_components(world: &mut World) {
    let staged = std::mem::take(&mut world.resource_mut::<StagedInserts>().entities);
    let mut staged: Vec<(Entity, StagedEntity)> = staged.into_iter().collect();
    staged.sort_by_key(|(_, staged)| staged.rollback);

    for (entity, staged) in staged {
        match world.get_entity_mut(entity) {
            Ok(mut entity) => staged.insert_into(&mut entity),
            // Dropping the boxes drops the values.
            Err(_) => warn!(
                "{:?} ({entity:?}) went away before its rolled-back components were inserted",
                staged.rollback
            ),
        }
    }
}

impl StagedEntity {
    /// Inserts the staged values as one dynamic bundle.
    fn insert_into(self, entity: &mut EntityWorldMut) {
        let mut components = self.components;
        // A sorted id list is what the bundle cache keys on. A duplicate id would panic in the
        // bundle, so of two values for one type the later one is dropped here instead, with a
        // warning: a type two snapshot plugins load is a registration mistake.
        components.sort_by_key(|(id, _)| *id);
        components.dedup_by(|later, earlier| {
            let duplicate = later.0 == earlier.0;
            if duplicate {
                warn!(
                    "{:?} staged twice for {:?}: two snapshot plugins load it; keeping the first",
                    entity.world().components().get_name(earlier.0),
                    self.rollback
                );
            }
            duplicate
        });
        let ids: Vec<ComponentId> = components.iter().map(|(id, _)| *id).collect();
        let values: Vec<(NonNull<u8>, Layout)> = components
            .into_iter()
            .map(|(_, value)| into_raw(value))
            .collect();

        // SAFETY: `stage_insert` registered each id in this world for the type of the value it
        // staged with it, and `values[i]` owns an initialized, aligned value of the type `ids[i]`
        // names. The insert moves every value out exactly once (one pointer per id).
        unsafe {
            entity.insert_by_ids(&ids, values.iter().map(|&(ptr, _)| OwningPtr::new(ptr)));
        }

        for (ptr, layout) in values {
            // A zero-sized value had no allocation.
            if layout.size() != 0 {
                // SAFETY: `ptr` came out of a `Box` of this layout in `into_raw`, and the value
                // in it was moved out by the insert, so only the allocation is left to free.
                unsafe { dealloc(ptr.as_ptr(), layout) };
            }
        }
    }
}

/// Takes the allocation out of a box, with the value still in it, so the value can be moved
/// out through an [`OwningPtr`] and the allocation freed afterwards.
fn into_raw(value: Box<dyn Any + Send + Sync>) -> (NonNull<u8>, Layout) {
    let layout = Layout::for_value::<dyn Any + Send + Sync>(&*value);
    let ptr = Box::into_raw(value).cast::<u8>();
    (NonNull::new(ptr).expect("a box is never null"), layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A staged value whose entity is gone by the insert is dropped, once.
    #[test]
    fn a_staged_value_without_its_entity_is_dropped() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static DROPS: AtomicU32 = AtomicU32::new(0);

        #[derive(Component)]
        struct Counted(#[allow(dead_code)] Box<u32>);

        impl Drop for Counted {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut world = World::new();
        world.init_resource::<StagedInserts>();
        let entity = world.spawn_empty().id();
        let rollback = RollbackId::new(entity);
        world.despawn(entity);

        stage_insert(entity, rollback, Counted(Box::new(1))).apply(&mut world);
        insert_staged_components(&mut world);

        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        assert!(world.resource::<StagedInserts>().entities.is_empty());
    }

    /// Two values staged for one type on one entity do not panic the bundle: the first lands,
    /// and a warning names the type.
    #[test]
    fn a_type_staged_twice_lands_once() {
        #[derive(Component, Debug, PartialEq)]
        struct Value(u32);

        let mut world = World::new();
        world.init_resource::<StagedInserts>();
        let entity = world.spawn_empty().id();
        let rollback = RollbackId::new(entity);

        stage_insert(entity, rollback, Value(1)).apply(&mut world);
        stage_insert(entity, rollback, Value(2)).apply(&mut world);
        insert_staged_components(&mut world);

        assert_eq!(world.get::<Value>(entity), Some(&Value(1)));
    }
}
