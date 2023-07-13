# [![GGRS LOGO](./ggrs_logo.png)](https://gschup.github.io/ggrs/)

[![crates.io](https://img.shields.io/crates/v/bevy_ggrs?style=for-the-badge)](https://crates.io/crates/bevy_ggrs)
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/gschup/bevy_ggrs/rust.yml?branch=main&style=for-the-badge)

## Bevy GGRS

[Bevy](https://github.com/bevyengine/bevy) plugin for the [GGRS](https://github.com/gschup/ggrs) P2P rollback networking library.
The plugin creates a custom stage with a separate schedule, which handles correctly advancing the gamestate, including rollbacks.
It efficiently handles saving and loading of the gamestate by only snapshotting relevant parts of the world, as defined by the user. It is supposed to work with the latest released version of bevy.

For explanation on how to use it, check the 👉[examples](./examples/)!

## Live Demonstration

bevy_GGRS has a demo app you can try in the browser! It uses [matchbox](https://github.com/johanhelsing/matchbox) to facilitate communication between browsers. Try it out with a friend! Just click the link and match with another player! (You can also open the link in two separate windows to play against yourself)

- [Demo](https://gschup.github.io/bevy_ggrs_demo/)
- [Demo Repository](https://github.com/gschup/bevy_ggrs_demo)

## Compatible Versions

|bevy|bevy_ggrs|ggrs|
|---|---|---|
|0.11|main|main|
|0.10|0.12|0.9.4|
|0.9|0.11|0.9.3|
|0.8|0.10|0.9|
|0.6|0.9|0.9|

## Thanks

to [bevy_backroll](https://github.com/HouraiTeahouse/backroll-rs/tree/main/bevy_backroll) and [bevy_rollback](https://github.com/jamescarterbell/bevy_rollback) for figuring out pieces of the puzzle that made bevy_GGRS possible. Special thanks to the helpful folks in the bevy discord, providing useful help and pointers all over the place.

## Licensing

Bevy_GGRS is dual-licensed under either

- [MIT License](./LICENSE-MIT): Also available [online](http://opensource.org/licenses/MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE): Also available [online](http://www.apache.org/licenses/LICENSE-2.0)

at your option.
