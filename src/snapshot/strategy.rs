//! Snapshot storage strategies: [`CopyStrategy`], [`CloneStrategy`], and [`ReflectStrategy`].
//!
//! A [`Strategy`] defines how a value is serialized into a stored snapshot form and
//! deserialized back. The three built-in strategies cover the common cases:
//! - [`CopyStrategy`] — bitwise copy for [`Copy`] types (cheapest)
//! - [`CloneStrategy`] — `.clone()` for [`Clone`] types
//! - [`ReflectStrategy`] — dynamic reflection for [`Reflect`] + [`FromWorld`] types
//!
//! Pass a strategy as a type parameter to [`ComponentSnapshotPlugin`](`super::ComponentSnapshotPlugin`)
//! or [`ResourceSnapshotPlugin`](`super::ResourceSnapshotPlugin`) to control how data is stored.

use std::marker::PhantomData;

use bevy::{
    prelude::{FromWorld, World},
    reflect::{PartialReflect, Reflect, ReflectMut, ReflectRef},
};

/// Describes how to efficiently transform a [`Target`](`Strategy::Target`) into a
/// [`Stored`](`Strategy::Stored`) version, and vice versa.
/// Any implementation for a [`Strategy`] should form a bijection between [`Target`](`Strategy::Target`) and [`Stored`](`Strategy::Stored`)
pub trait Strategy {
    /// The original version of the data to be stored.
    type Target;

    /// A stored version of the data which can be transformed back into a [`Target`](`Strategy::Target`).
    type Stored;

    /// Create a [`Stored`](`Strategy::Stored`) version of the provided [`Target`](`Strategy::Target`) reference.
    fn store(target: &Self::Target) -> Self::Stored;

    /// Create a [`Target`](`Strategy::Target`) version of the provided [`Stored`](`Strategy::Stored`) reference.
    fn load(stored: &Self::Stored) -> Self::Target;

    /// Directly update a mutable reference to an existing [`Target`](`Strategy::Target`)
    /// with the data from a provided [`Stored`](`Strategy::Stored`).
    fn update(target: &mut Self::Target, stored: &Self::Stored) {
        *target = Self::load(stored);
    }
}

/// A [`Strategy`] based on [`Copy`]
pub struct CopyStrategy<T: Copy>(PhantomData<T>);

impl<T: Copy> Strategy for CopyStrategy<T> {
    type Target = T;

    type Stored = T;

    #[inline(always)]
    fn store(target: &Self::Target) -> Self::Stored {
        *target
    }

    #[inline(always)]
    fn load(stored: &Self::Stored) -> Self::Target {
        *stored
    }
}

/// A [`Strategy`] based on [`Clone`]
pub struct CloneStrategy<T: Clone>(PhantomData<T>);

impl<T: Clone> Strategy for CloneStrategy<T> {
    type Target = T;

    type Stored = T;

    #[inline(always)]
    fn store(target: &Self::Target) -> Self::Stored {
        target.clone()
    }

    #[inline(always)]
    fn load(stored: &Self::Stored) -> Self::Target {
        stored.clone()
    }

    #[inline(always)]
    fn update(target: &mut Self::Target, stored: &Self::Stored) {
        target.clone_from(stored);
    }
}

/// A [`Strategy`] based on [`Reflect`] and [`FromWorld`]
///
/// Restoring a snapshot into an existing value updates it in place via [`PartialReflect::apply`],
/// then truncates any lists that were longer than in the snapshot. Fields marked
/// `#[reflect(ignore)]` are not part of the snapshot, so they are left untouched by an in-place
/// restore and take their [`FromWorld`] value when the value has to be re-created.
pub struct ReflectStrategy<T: Reflect + FromWorld>(PhantomData<T>);

impl<T: Reflect + FromWorld> Strategy for ReflectStrategy<T> {
    type Target = T;

    type Stored = Box<dyn PartialReflect>;

    #[inline(always)]
    fn store(target: &Self::Target) -> Self::Stored {
        target.as_reflect().to_dynamic()
    }

    #[inline(always)]
    fn update(target: &mut Self::Target, stored: &Self::Stored) {
        target.apply(stored.as_ref());
        truncate_lists(target, stored.as_ref());
    }

    #[inline(always)]
    fn load(stored: &Self::Stored) -> Self::Target {
        let mut world: World = Default::default();
        let mut target = Self::Target::from_world(&mut world);
        Self::update(&mut target, stored);
        target
    }
}

/// Removes trailing list elements from `target` that `source` doesn't have, at any depth.
///
/// [`PartialReflect::apply`] patches lists rather than replacing them: it overwrites the
/// common prefix and appends extra elements from the source, but never removes elements
/// beyond the source's length (see <https://github.com/bevyengine/bevy/issues/7129>).
/// Restoring a snapshot in which a list was shorter would otherwise keep the stale tail.
///
/// Must be called after `target.apply(source)`, so both sides have the same shape apart
/// from list lengths. Maps and sets need no handling here, as their `apply` already
/// removes entries missing from the source.
fn truncate_lists(target: &mut dyn PartialReflect, source: &dyn PartialReflect) {
    match (target.reflect_mut(), source.reflect_ref()) {
        (ReflectMut::List(target), ReflectRef::List(source)) => {
            while target.len() > source.len() {
                target.pop();
            }
            for (i, source) in source.iter().enumerate() {
                if let Some(target) = target.get_mut(i) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::Array(target), ReflectRef::Array(source)) => {
            for (i, source) in source.iter().enumerate() {
                if let Some(target) = target.get_mut(i) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::Struct(target), ReflectRef::Struct(source)) => {
            for (name, source) in source.iter_fields() {
                if let Some(target) = target.field_mut(name) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::TupleStruct(target), ReflectRef::TupleStruct(source)) => {
            for (i, source) in source.iter_fields().enumerate() {
                if let Some(target) = target.field_mut(i) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::Tuple(target), ReflectRef::Tuple(source)) => {
            for (i, source) in source.iter_fields().enumerate() {
                if let Some(target) = target.field_mut(i) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::Map(target), ReflectRef::Map(source)) => {
            for (key, source) in source.iter() {
                if let Some(target) = target.get_mut(key) {
                    truncate_lists(target, source);
                }
            }
        }
        (ReflectMut::Enum(target), ReflectRef::Enum(source))
            if target.variant_name() == source.variant_name() =>
        {
            for i in 0..source.field_len() {
                if let (Some(target), Some(source)) = (target.field_at_mut(i), source.field_at(i)) {
                    truncate_lists(target, source);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use bevy::prelude::*;

    use super::{ReflectStrategy, Strategy};

    // --- ReflectStrategy ---

    #[derive(Reflect, Default, PartialEq, Debug)]
    struct Foo {
        x: f32,
        y: u32,
    }

    /// store→load round-trip preserves reflected fields.
    #[test]
    fn reflect_strategy_round_trip() {
        let value = Foo { x: 1.5, y: 7 };
        let stored = ReflectStrategy::<Foo>::store(&value);
        let loaded = ReflectStrategy::<Foo>::load(&stored);
        assert_eq!(loaded, value);
    }

    #[derive(Reflect, Default, PartialEq, Debug)]
    struct Trail(Vec<u32>);

    /// Restoring a snapshot in which a list was shorter removes the extra elements.
    #[test]
    fn reflect_strategy_update_shrinks_list() {
        let stored = ReflectStrategy::<Trail>::store(&Trail(vec![1, 2]));
        let mut value = Trail(vec![1, 2, 3, 4]);
        ReflectStrategy::<Trail>::update(&mut value, &stored);
        assert_eq!(value, Trail(vec![1, 2]));
    }

    /// Restoring a snapshot in which a list was longer appends the missing elements.
    #[test]
    fn reflect_strategy_update_grows_list() {
        let stored = ReflectStrategy::<Trail>::store(&Trail(vec![1, 2, 3]));
        let mut value = Trail(vec![9]);
        ReflectStrategy::<Trail>::update(&mut value, &stored);
        assert_eq!(value, Trail(vec![1, 2, 3]));
    }

    #[derive(Reflect, Default, PartialEq, Debug)]
    enum Shape {
        #[default]
        Point,
        Path(Vec<u32>),
        Polygon {
            vertices: Vec<u32>,
        },
    }

    #[derive(Reflect, Default, PartialEq, Debug)]
    struct Nested {
        deque: VecDeque<u32>,
        nested: Vec<Vec<u32>>,
        option: Option<Vec<u32>>,
        tuple_variant: Shape,
        struct_variant: Shape,
        tuple: (Vec<u32>, u32),
        array: [Vec<u32>; 2],
        map: HashMap<u32, Vec<u32>>,
    }

    /// Lists are truncated wherever they appear: in other lists, options, enum variants,
    /// tuples, arrays and map values.
    #[test]
    fn reflect_strategy_update_shrinks_nested_lists() {
        let snapshot = Nested {
            deque: [1].into(),
            nested: vec![vec![1], vec![]],
            option: Some(vec![1]),
            tuple_variant: Shape::Path(vec![1]),
            struct_variant: Shape::Polygon { vertices: vec![1] },
            tuple: (vec![1], 1),
            array: [vec![1], vec![]],
            map: [(1, vec![1])].into(),
        };
        let stored = ReflectStrategy::<Nested>::store(&snapshot);

        let mut value = Nested {
            deque: [1, 2, 3].into(),
            nested: vec![vec![1, 2], vec![3], vec![4]],
            option: Some(vec![1, 2]),
            tuple_variant: Shape::Path(vec![1, 2]),
            struct_variant: Shape::Polygon {
                vertices: vec![1, 2],
            },
            tuple: (vec![1, 2], 1),
            array: [vec![1, 2], vec![3]],
            map: [(1, vec![1, 2]), (2, vec![3])].into(),
        };
        ReflectStrategy::<Nested>::update(&mut value, &stored);

        assert_eq!(value, snapshot);
    }

    #[derive(Reflect, PartialEq, Debug)]
    struct Slots(Vec<u32>);

    impl Default for Slots {
        fn default() -> Self {
            Self(vec![0; 4])
        }
    }

    /// Loading into a freshly created value whose default contains a longer list still
    /// produces exactly the snapshot.
    #[test]
    fn reflect_strategy_load_shrinks_non_empty_default() {
        let stored = ReflectStrategy::<Slots>::store(&Slots(vec![7]));
        assert_eq!(ReflectStrategy::<Slots>::load(&stored), Slots(vec![7]));
    }

    #[derive(Reflect, Default, PartialEq, Debug)]
    struct WithIgnored {
        values: Vec<u32>,
        #[reflect(ignore)]
        cache: u32,
    }

    /// Fields excluded from reflection are not part of the snapshot and are left untouched
    /// by an in-place restore.
    #[test]
    fn reflect_strategy_update_leaves_ignored_fields() {
        let stored = ReflectStrategy::<WithIgnored>::store(&WithIgnored {
            values: vec![1],
            cache: 0,
        });
        let mut value = WithIgnored {
            values: vec![1, 2],
            cache: 7,
        };
        ReflectStrategy::<WithIgnored>::update(&mut value, &stored);
        assert_eq!(
            value,
            WithIgnored {
                values: vec![1],
                cache: 7
            }
        );
    }
}
