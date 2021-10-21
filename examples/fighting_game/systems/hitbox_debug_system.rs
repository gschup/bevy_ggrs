use crate::*;

#[derive(Default, Copy, Clone, Component)]
pub struct DebugBox {}

pub fn hit_box_setup_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_handles: ResMut<TextureAtlasDictionary>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let hitbox_texture_handle = asset_server.load("sprites/hitbox.png");
    let hurtbox_texture_handle = asset_server.load("sprites/hurtbox.png");
    texture_handles.debug_hit_box_texture = materials.add(hitbox_texture_handle.clone().into());
    texture_handles.debug_hurt_box_texture = materials.add(hurtbox_texture_handle.clone().into());

    for _ in 0..4 {
        let sprite_transform = Transform::from_translation(Vec3::new(0.0, 0.0, 0.0));

        commands
            .spawn_bundle(SpriteBundle {
                material: texture_handles.debug_hit_box_texture.clone(),
                transform: sprite_transform,
                ..Default::default()
            })
            .insert(DebugBox::default());
    }
}
