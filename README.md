# [![GGRS LOGO](./ggrs_logo.png)](https://gschup.github.io/ggrs/)

[![crates.io](https://img.shields.io/crates/v/bevy_ggrs?style=for-the-badge)](https://crates.io/crates/bevy_ggrs)
![GitHub Workflow Status](https://img.shields.io/github/workflow/status/gschup/bevy_ggrs/Rust?style=for-the-badge)

## Bevy GGRS

[Bevy](https://github.com/bevyengine/bevy) plugin for the [GGRS](https://github.com/gschup/ggrs) P2P rollback networking library.
The plugin creates a custom stage with a separate schedule, which handles correctly advancing the gamestate, including rollbacks.
It efficiently handles saving and loading of the gamestate by only snapshotting relevant parts of the world, as defined by the user. It is supposed to work with the latest released version of bevy.

For explanation on how to use it, check the 👉[examples](./examples/)!

## ⚠️ REFACTOR WARNING ⚠️

Due to exciting upcoming [stageless scheduling](https://github.com/bevyengine/rfcs/pull/45) This whole plugin will have to be reworked from the ground up (Once it is there). Since I am not happy with using bevy-reflect to save and load snapshots of the world, I am looking forward to this refactoring!

## Live Demonstration

bevy_GGRS has a demo app you can try in the browser! It uses [matchbox](https://github.com/johanhelsing/matchbox) to facilitate communication between browsers. Try it out with a friend! Just click the link and match with another player! (You can also open the link in two separate windows to play against yourself)

- [Demo](https://gschup.github.io/bevy_ggrs_demo/)
- [Demo Repository](https://github.com/gschup/bevy_ggrs_demo)

## How it works

The GGRS plugin creates a custom `GGRSStage` which owns a separate schedule. Inside this schedule, the user can add stages and systems as they wish.
When the default schedule runs the `GGRSStage`, it polls the session and executes resulting `GGRSRequests`, such as loading, saving and advancing the gamestate.

- advancing the gamestate is done by running the internal schedule once.
- saving the gamestate is done by creating a snapshot of entities tagged with a `bevy_ggrs::Rollback` component and saving only the components that were registered through `register_rollback_type::<YourCoolComponent>()`. The plugin internally keeps track of the snapshots together with the GGRS session.
- loading the gamestate applies the snapshot by overwriting, creating and deleting entities tagged with a `bevy_ggrs::Rollback` component and updating the registered components values.

Since bevy_ggrs operates with a separate schedule, compatibility with other plugins might be complicated to achieve out of the box, as all gamestate-relevant systems needs to somehow end up inside the internal GGRS schedule to be updated together the rest of the game systems.

## Compatible Versions

|bevy|bevy_ggrs|ggrs|
|---|---|---|
|0.8|main|main|
|0.8|0.10|0.9|
|0.6|0.9|0.9|

## Thanks

to [bevy_backroll](https://github.com/HouraiTeahouse/backroll-rs/tree/main/bevy_backroll) and [bevy_rollback](https://github.com/jamescarterbell/bevy_rollback) for figuring out pieces of the puzzle that made bevy_GGRS possible. Special thanks to the helpful folks in the bevy discord, providing useful help and pointers all over the place.

## Licensing

Bevy_GGRS is dual-licensed under either

- [MIT License](./LICENSE-MIT): Also available [online](http://opensource.org/licenses/MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE): Also available [online](http://www.apache.org/licenses/LICENSE-2.0)

at your option.
