# [![GGRS LOGO](./ggrs_logo.png)](https://gschup.github.io/ggrs/)

[![crates.io](https://img.shields.io/crates/v/bevy_ggrs?style=for-the-badge)](https://crates.io/crates/bevy_ggrs)
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/gschup/bevy_ggrs/rust.yml?branch=main&style=for-the-badge)

## Bevy GGRS

[Bevy](https://github.com/bevyengine/bevy) plugin for the [GGRS](https://github.com/gschup/ggrs) P2P rollback networking library. Handles advancing game state and rollbacks via a dedicated schedule, snapshotting only the components and resources you register.

## Quickstart

```toml
# Cargo.toml
[dependencies]
bevy_ggrs = { git = "https://github.com/gschup/bevy_ggrs" }
```

```rust
use bevy::prelude::*;
use bevy_ggrs::prelude::*;

type GgrsConfig = bevy_ggrs::GgrsConfig<u8>; // replace u8 with your input type

App::new()
    .add_plugins(GgrsPlugin::<GgrsConfig>::default())
    .insert_resource(RollbackFrameRate(60))
    // register components/resources for snapshotting
    .rollback_component_with_copy::<Transform>()
    // provide inputs each frame
    .add_systems(ReadInputs, read_local_inputs)
    // your game logic — must be deterministic!
    .add_systems(GgrsSchedule, move_players)
    .insert_resource(Session::SyncTest(session))
    .run();

// tag entities for rollback at spawn time
fn spawn_player(mut commands: Commands) {
    commands.spawn((Transform::default(), Rollback));
}
```

For full P2P and spectator session examples, see [examples/](./examples/).

## Live Demonstration (unmaintained)

bevy_ggrs has a demo app using [matchbox](https://github.com/johanhelsing/matchbox) for browser-based P2P. It is currently unmaintained and may not work with the latest version.

- [Demo Repository](https://github.com/gschup/bevy_ggrs_demo)

## Compatible Versions

| bevy | bevy_ggrs | ggrs   |
| ---- | --------- | ------ |
| 0.18 | main      | main   |
| 0.18 | 0.20      | 0.11.1 |
| 0.17 | 0.19      | 0.11.1 |
| 0.16 | 0.18      | 0.11.1 |
| 0.15 | 0.17      | 0.11.0 |
| 0.14 | 0.16      | 0.10.2 |
| 0.13 | 0.15      | 0.10.1 |
| 0.12 | 0.14      | 0.10   |
| 0.11 | 0.13      | 0.9.4  |
| 0.10 | 0.12      | 0.9.4  |
| 0.9  | 0.11      | 0.9.3  |
| 0.8  | 0.10      | 0.9    |
| 0.6  | 0.9       | 0.9    |

## Community

- [matchbox](https://github.com/johanhelsing/matchbox) — WebRTC socket layer, pairs well with bevy_ggrs for browser/WASM P2P
- [extreme_bevy](https://github.com/johanhelsing/extreme_bevy) — tutorial and example project: how to build a low-latency P2P web game with bevy_ggrs and matchbox

## Thanks

to [bevy_backroll](https://github.com/HouraiTeahouse/backroll-rs/tree/main/bevy_backroll) and [bevy_rollback](https://github.com/jamescarterbell/bevy_rollback) for figuring out pieces of the puzzle that made bevy_ggrs possible. Special thanks to the helpful folks in the Bevy Discord for their support along the way.

## Licensing

Bevy_GGRS is dual-licensed under either

- [MIT License](./LICENSE-MIT): Also available [online](http://opensource.org/licenses/MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE): Also available [online](http://www.apache.org/licenses/LICENSE-2.0)

at your option.
