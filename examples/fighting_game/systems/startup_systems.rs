
use bevy::prelude::*;
use crate::systems::*;
use crate::*;

fn load_sprite_atlas_into_texture_dictionary(
    animation_name: String, 
    asset_server: &Res<AssetServer>, 
    texture_atlases: &mut ResMut<Assets<TextureAtlas>>,
    texture_atlas_handles: &mut ResMut<TextureAtlasDictionary>,
    width: f32,
    height: f32,
    number_of_images: usize
) {
    let texture_handle = asset_server.load(animation_name.as_str());
    let texture_atlas = TextureAtlas::from_grid(texture_handle, Vec2::new(width, height), number_of_images, 1);
    let texture_atlas_handle = texture_atlases.add(texture_atlas);
    texture_atlas_handles.animation_handles.insert(animation_name, texture_atlas_handle);
}

pub fn match_setup(
    mut state: ResMut<State<GameState>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut rip: ResMut<RollbackIdProvider>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut texture_atlas_handles: ResMut<TextureAtlasDictionary>,
    p2p_session: Option<Res<P2PSession>>,
) {
    commands.spawn_bundle(UiCameraBundle::default());

    let cloud_image = asset_server.load("sprites/Cloud.png");
    texture_atlas_handles.cloud_image = materials.add(cloud_image.clone().into());

    //Load each of our textures
    // TODO: have this handle different characters, for now it is just the single samurai
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Idle.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 8);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Run.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 8);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Jump.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 2);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Attack1.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 6);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Fall.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 2);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/TakeHit.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 4);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Death.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 6);
    load_sprite_atlas_into_texture_dictionary(String::from("sprites/Dash.png"), &asset_server, &mut texture_atlases, &mut texture_atlas_handles, 200.0, 200.0, 4);

    let num_players = p2p_session
        .map(|s| s.num_players()).expect("No GGRS session found");


    //Spawn the sprites that are used by the reset system to hide the screen for the reset
    //maybe we can have 
    for i in 0..2 {
        let black_texture = asset_server.load("sprites/black.png");
        let mut blind_transform = Transform::from_translation(Vec3::new(0.0, 760.0 - (760.0 * 2.0 * i as f32), 9.0));
        blind_transform.scale.x = 100000.0;
        blind_transform.scale.y = 800.02;

        if i == 0 {
            commands.spawn_bundle(SpriteBundle {
                material: materials.add(black_texture.into()),
                transform: blind_transform,
                ..Default::default()
            }).insert(UpperBlind::default());
        }
        else {
            commands.spawn_bundle(SpriteBundle {
                material: materials.add(black_texture.into()),
                transform: blind_transform,
                ..Default::default()
            }).insert(LowerBlind::default());
        }
    }
    
    //Spawn the camera, and move it back so we can build in layers(UI, actions, background)
    let camera_transform = Transform::from_translation(Vec3::new(0.0, 0.0, 10.0));
    let mut camera = OrthographicCameraBundle::new_2d();
    camera.transform = camera_transform;
    commands.spawn_bundle(camera);

    //Spawn each player
    for i in 0..num_players {
        
        let mut p1_transform = Transform::from_translation(Vec3::new(-120.0 + (240.0 * i as f32), FLOOR_HEIGHT, 0.0));
        p1_transform.scale.x = 2.0;
        p1_transform.scale.y = 2.0;

        if i == 0 {
            let entity_id = commands
                .spawn_bundle(SpriteSheetBundle {
                    texture_atlas: texture_atlas_handles.animation_handles["sprites/Idle.png"].clone(),
                    transform:p1_transform,
                    ..Default::default()
                })
                .insert(Timer::from_seconds(0.05, true))
                .insert(PlayerState::new(i as usize, PlayerStateEnum::Idle)) 
                .insert(Rollback::new(rip.next_id()))
                .insert(Player1::default())
                .insert(ScreenSideEnum::Left)
                .insert(PlayerHealth::new()).id().clone();
            
            let hitbox_texture_handle = asset_server.load("sprites/health_bar.png");
            let mut health_transform = Transform::from_translation(Vec3::new(-400.0, HEALTH_UI_HEIGHT, 1.0));
            health_transform.scale = Vec3::new(400.0, 50.0, 1.0);
            commands.spawn_bundle(SpriteBundle {
                material: materials.add(hitbox_texture_handle.into()),
                transform: health_transform,
                ..Default::default()
            }).insert(PlayerHealthUI::new(entity_id));
        }
        else {
            let entity_id = commands
                .spawn_bundle(SpriteSheetBundle {
                    texture_atlas: texture_atlas_handles.animation_handles["sprites/Idle.png"].clone(),
                    transform:p1_transform,
                    ..Default::default()
                })
                .insert(Timer::from_seconds(0.05, true))
                .insert(PlayerState::new(i as usize, PlayerStateEnum::Idle))
                .insert(Rollback::new(rip.next_id()))
                .insert(Player2::default())
                .insert(PlayerHealth::new())
                .insert(ScreenSideEnum::Right).id().clone();

            let hitbox_texture_handle = asset_server.load("sprites/health_bar.png");
            let mut health_transform = Transform::from_translation(Vec3::new(400.0, HEALTH_UI_HEIGHT, 1.0));
            health_transform.scale = Vec3::new(400.0, 50.0, 1.0);
            commands.spawn_bundle(SpriteBundle {
                material: materials.add(hitbox_texture_handle.into()),
                transform: health_transform,
                ..Default::default()
            }).insert(PlayerHealthUI::new(entity_id));
        }
    }
    state.set(GameState::Fighting).unwrap();
}