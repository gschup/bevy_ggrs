use std::marker::PhantomData;

use bevy::{
    prelude::{FromWorld, World},
    reflect::Reflect,
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
pub struct ReflectStrategy<T: Reflect + FromWorld>(PhantomData<T>);

impl<T: Reflect + FromWorld> Strategy for ReflectStrategy<T> {
    type Target = T;

    type Stored = Box<dyn Reflect>;

    #[inline(always)]
    fn store(target: &Self::Target) -> Self::Stored {
        target.as_reflect().clone_value()
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
