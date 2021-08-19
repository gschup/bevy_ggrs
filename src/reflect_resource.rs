use bevy::{ecs::component::Component, prelude::*, reflect::FromType};

/*
 * Special thanks to https://github.com/jamescarterbell for this piece of code
 */

#[derive(Clone)]
pub(crate) struct ReflectResource {
    add_resource: fn(&mut World, &dyn Reflect),
    remove_resource: fn(&mut World),
    apply_resource: fn(&mut World, &dyn Reflect),
    reflect_resource: fn(&World) -> Option<&dyn Reflect>,
    copy_resource: fn(&World, &mut World),
}

impl ReflectResource {
    pub(crate) fn add_resource(&self, world: &mut World, resource: &dyn Reflect) {
        (self.add_resource)(world, resource);
    }

    pub(crate) fn remove_resource(&self, world: &mut World) {
        (self.remove_resource)(world);
    }

    pub(crate) fn apply_resource(&self, world: &mut World, resource: &dyn Reflect) {
        (self.apply_resource)(world, resource);
    }

    pub(crate) fn reflect_resource<'a>(&self, world: &'a World) -> Option<&'a dyn Reflect> {
        (self.reflect_resource)(world)
    }

    #[allow(dead_code)]
    pub(crate) fn copy_resource(&self, source_world: &World, destination_world: &mut World) {
        (self.copy_resource)(source_world, destination_world);
    }
}

impl<C: Component + Reflect + FromWorld> FromType<C> for ReflectResource {
    fn from_type() -> Self {
        ReflectResource {
            add_resource: |world, reflected_resource| {
                let mut resource = C::from_world(world);
                resource.apply(reflected_resource);
                world.insert_resource(resource);
            },
            remove_resource: |world| {
                world.remove_resource::<C>();
            },
            apply_resource: |world, reflected_resource| {
                let mut resource = world.get_resource_mut::<C>().unwrap();
                resource.apply(reflected_resource);
            },
            copy_resource: |source_world, destination_world| {
                let source_resource = source_world.get_resource::<C>().unwrap();
                let mut destination_resource = C::from_world(destination_world);
                destination_resource.apply(source_resource);
                destination_world.insert_resource(destination_resource);
            },
            reflect_resource: |world| world.get_resource::<C>().map(|c| c as &dyn Reflect),
        }
    }
}
