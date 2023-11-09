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

#[cfg(feature = "ron")]
mod ron_strategy {
    use std::marker::PhantomData;

    use serde::{de::DeserializeOwned, Serialize};

    use crate::Strategy;

    /// A [`Strategy`] based on [`serde`] and [`ron`]
    pub struct RonStrategy<T: Serialize + DeserializeOwned>(PhantomData<T>);

    impl<T: Serialize + DeserializeOwned> Strategy for RonStrategy<T> {
        type Target = T;

        type Stored = String;

        #[inline(always)]
        fn store(target: &Self::Target) -> Self::Stored {
            ron::to_string(target).unwrap()
        }

        #[inline(always)]
        fn load(stored: &Self::Stored) -> Self::Target {
            ron::from_str(stored).unwrap()
        }
    }
}

#[cfg(feature = "ron")]
pub use ron_strategy::*;

#[cfg(feature = "bincode")]
mod bincode_strategy {
    use std::marker::PhantomData;

    use serde::{de::DeserializeOwned, Serialize};

    use crate::Strategy;

    /// A [`Strategy`] based on [`serde`] and [`bincode`]
    pub struct BincodeStrategy<T: Serialize + DeserializeOwned>(PhantomData<T>);

    impl<T: Serialize + DeserializeOwned> Strategy for BincodeStrategy<T> {
        type Target = T;

        type Stored = Vec<u8>;

        #[inline(always)]
        fn store(target: &Self::Target) -> Self::Stored {
            bincode::serialize(target).unwrap()
        }

        #[inline(always)]
        fn load(stored: &Self::Stored) -> Self::Target {
            bincode::deserialize(stored).unwrap()
        }
    }
}

#[cfg(feature = "bincode")]
pub use bincode_strategy::*;
