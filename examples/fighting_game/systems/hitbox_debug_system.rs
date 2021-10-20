use crate::systems::*;
use crate::*;

#[derive(Default, Component)]
pub struct ShouldRenderHitBoxes {
    should_render: bool,
}

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

pub fn hitbox_debug_system(
    mut commands: Commands,
    should_render_hit_box: ResMut<ShouldRenderHitBoxes>,
    collider_set_component: Res<ColliderSetComponent>,
    texture_handles: ResMut<TextureAtlasDictionary>,
    mut debug_query: Query<(&mut Transform, &DebugBox, Entity), Without<PlayerState>>,
    player_query: Query<(&PlayerState, &Transform, &ScreenSideEnum), Without<DebugBox>>,
) {

    /*
    if should_render_hit_box.should_render {
        // move all of the current hit boxes away from the middle of the screen, not great but EH
        for (mut t, _, _) in debug_query.iter_mut() {
            t.translation = Vec3::new(1000.0, 1000.0, 1000.0);
        }

        let mut debug_iter = debug_query.iter_mut();

        for (&player_state, &player_transform, &screen_side) in player_query.iter() {
            let frame_colliders = &collider_set_component.colliders[&player_state.player_state.to_string()][player_state.current_sprite_index];
            for collider in frame_colliders {
                let (mut transform, &_debug_box, entity) = debug_iter.next().unwrap();
                let texture_handle;
                match collider.collider_type {
                    ColliderType::HitBox => {
                        texture_handle = texture_handles.debug_hit_box_texture.clone();
                    },
                    ColliderType::HurtBox => {
                        texture_handle = texture_handles.debug_hurt_box_texture.clone();
                    }
                }
                let mut right_side_inverse = 1.0f32;
                match screen_side {
                    ScreenSideEnum::Right => {
                        right_side_inverse = -1.0f32;
                    },
                    _ => {}
                }

                let mut collider_offset = collider.offset.clone();
                collider_offset.x = collider_offset.x * right_side_inverse;
                transform.translation = collider_offset + player_transform.translation;
                transform.scale.x = collider.dimension.x;
                transform.scale.y = collider.dimension.y;
                commands.entity(entity).insert(texture_handle);
            }
        }
    }
    */
}
