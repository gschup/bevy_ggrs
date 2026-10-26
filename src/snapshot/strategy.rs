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
/// # Panics
///
/// [`store`](`Strategy::store`) panics if the value can't be converted to its dynamic
/// representation with [`PartialReflect::to_dynamic`]. Types that derive [`Reflect`] always
/// convert; this only happens when `T` contains an opaque type whose hand-written
/// [`PartialReflect`] implementation overrides neither `to_dynamic` nor `reflect_clone`.
/// The panic message names `T` and the type to blame.
pub struct ReflectStrategy<T: Reflect + FromWorld>(PhantomData<T>);

impl<T: Reflect + FromWorld> Strategy for ReflectStrategy<T> {
    type Target = T;

    type Stored = Box<dyn PartialReflect>;

    #[inline(always)]
    fn store(target: &Self::Target) -> Self::Stored {
        target.as_reflect().to_dynamic().unwrap_or_else(|error| {
            panic!(
                "failed to snapshot `{}` for rollback: {error}",
                std::any::type_name::<T>()
            )
        })
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
    use std::any::Any;

    use bevy::{
        prelude::*,
        reflect::{
            ApplyError, OpaqueInfo, ReflectMut, ReflectOwned, ReflectRef, TypeInfo, Typed,
            utility::NonGenericTypeInfoCell,
        },
    };

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

    /// An opaque type with a hand-written [`PartialReflect`] that overrides neither
    /// `to_dynamic` nor `reflect_clone`, so it can't be made dynamic.
    #[derive(TypePath, Default)]
    struct Uncloneable;

    impl Typed for Uncloneable {
        fn type_info() -> &'static TypeInfo {
            static CELL: NonGenericTypeInfoCell = NonGenericTypeInfoCell::new();
            CELL.get_or_set(|| TypeInfo::Opaque(OpaqueInfo::new::<Self>()))
        }
    }

    impl PartialReflect for Uncloneable {
        fn get_represented_type_info(&self) -> Option<&'static TypeInfo> {
            Some(Self::type_info())
        }
        fn into_partial_reflect(self: Box<Self>) -> Box<dyn PartialReflect> {
            self
        }
        fn as_partial_reflect(&self) -> &dyn PartialReflect {
            self
        }
        fn as_partial_reflect_mut(&mut self) -> &mut dyn PartialReflect {
            self
        }
        fn try_into_reflect(self: Box<Self>) -> Result<Box<dyn Reflect>, Box<dyn PartialReflect>> {
            Ok(self)
        }
        fn try_as_reflect(&self) -> Option<&dyn Reflect> {
            Some(self)
        }
        fn try_as_reflect_mut(&mut self) -> Option<&mut dyn Reflect> {
            Some(self)
        }
        fn try_apply(&mut self, _value: &dyn PartialReflect) -> Result<(), ApplyError> {
            Ok(())
        }
        fn reflect_ref(&self) -> ReflectRef<'_> {
            ReflectRef::Opaque(self)
        }
        fn reflect_mut(&mut self) -> ReflectMut<'_> {
            ReflectMut::Opaque(self)
        }
        fn reflect_owned(self: Box<Self>) -> ReflectOwned {
            ReflectOwned::Opaque(self)
        }
    }

    impl Reflect for Uncloneable {
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn into_reflect(self: Box<Self>) -> Box<dyn Reflect> {
            self
        }
        fn as_reflect(&self) -> &dyn Reflect {
            self
        }
        fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
            self
        }
        fn set(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
            *self = *value.downcast()?;
            Ok(())
        }
    }

    /// A value that can't be made dynamic panics with the name of the snapshotted type.
    #[test]
    #[should_panic(
        expected = "failed to snapshot `bevy_ggrs::snapshot::strategy::tests::Uncloneable`"
    )]
    fn reflect_strategy_store_panic_names_type() {
        ReflectStrategy::<Uncloneable>::store(&Uncloneable);
    }
}
