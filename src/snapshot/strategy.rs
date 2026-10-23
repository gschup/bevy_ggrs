//! Snapshot storage strategies: [`CopyStrategy`], [`CloneStrategy`], and [`ReflectStrategy`].
//!
//! A [`Strategy`] defines how a value is serialized into a stored snapshot form and
//! deserialized back. The three built-in strategies cover the common cases:
//! - [`CopyStrategy`] — bitwise copy for [`Copy`] types (cheapest)
//! - [`CloneStrategy`] — `.clone()` for [`Clone`] types
//! - [`ReflectStrategy`] — dynamic reflection for [`Reflect`] + [`FromWorld`] types
//!
//! [`CopyEqStrategy`] and [`CloneEqStrategy`] are the first two for a type that is
//! also [`RollbackEq`]: a load leaves a component that already holds the
//! snapshot's value alone, so it is not marked changed.
//!
//! Pass a strategy as a type parameter to [`ComponentSnapshotPlugin`](`super::ComponentSnapshotPlugin`)
//! or [`ResourceSnapshotPlugin`](`super::ResourceSnapshotPlugin`) to control how data is stored.

use std::marker::PhantomData;

use bevy::{
    prelude::{FromWorld, World},
    reflect::{PartialReflect, Reflect},
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

    /// Whether `target` already holds exactly `stored`, so a load may leave it
    /// alone and not mark it changed. `false` unless the strategy can tell:
    /// the load then always writes.
    #[inline(always)]
    fn equals(_target: &Self::Target, _stored: &Self::Stored) -> bool {
        false
    }
}

/// Equality as a rollback needs it: the same bits, not the same value. A
/// load that skipped writing `-0.0` over `0.0`, which [`PartialEq`] calls
/// equal, would leave the world holding other bits than the snapshot, and a
/// later computation that depends on the sign of a zero would part from a
/// run that never rolled back. Compare floats with `to_bits`.
pub trait RollbackEq {
    /// Whether `self` and `other` have the same bits.
    fn rollback_eq(&self, other: &Self) -> bool;
}

impl RollbackEq for f32 {
    #[inline(always)]
    fn rollback_eq(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl RollbackEq for bevy::math::Vec2 {
    #[inline(always)]
    fn rollback_eq(&self, other: &Self) -> bool {
        self.x.rollback_eq(&other.x) && self.y.rollback_eq(&other.y)
    }
}

impl RollbackEq for bevy::math::Vec3 {
    #[inline(always)]
    fn rollback_eq(&self, other: &Self) -> bool {
        self.x.rollback_eq(&other.x) && self.y.rollback_eq(&other.y) && self.z.rollback_eq(&other.z)
    }
}

/// [`CopyStrategy`] for a [`RollbackEq`] type: a load skips a component that
/// already holds the snapshot's bits.
pub struct CopyEqStrategy<T: Copy + RollbackEq>(PhantomData<T>);

impl<T: Copy + RollbackEq> Strategy for CopyEqStrategy<T> {
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

    #[inline(always)]
    fn equals(target: &Self::Target, stored: &Self::Stored) -> bool {
        target.rollback_eq(stored)
    }
}

/// [`CloneStrategy`] for a [`RollbackEq`] type: a load skips a component that
/// already holds the snapshot's bits.
pub struct CloneEqStrategy<T: Clone + RollbackEq>(PhantomData<T>);

impl<T: Clone + RollbackEq> Strategy for CloneEqStrategy<T> {
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

    #[inline(always)]
    fn equals(target: &Self::Target, stored: &Self::Stored) -> bool {
        target.rollback_eq(stored)
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
    }

    #[inline(always)]
    fn load(stored: &Self::Stored) -> Self::Target {
        let mut world: World = Default::default();
        let mut target = Self::Target::from_world(&mut world);
        Self::update(&mut target, stored);
        target
    }
}

#[cfg(test)]
mod tests {
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
}
