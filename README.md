# Bevy_GGRS

Bevy plugin for the [GGRS](https://github.com/gschup/ggrs) P2P rollback networking library.
The plugin creates a custom stage with a separate schedule, which handles correctly advancing the gamestate, including rollbacks.
It efficiently handles saving and loading of the gamestate by only snapshotting relevant parts of the world, as defined by the user.

For advise on how to use it, check the [examples](./examples/)!

## Development Status

bevy_ggrs is in a very early stage:

- no checksums are generated, so `SyncTestSession` doesn't do all that much.
- GGRS dependency is directly linked to the repository (will change once necessary changes are published)
- only components of entities can be rolled back, no resources yet.
- since bevy_ggrs operates with a separate schedule, compatibility with other plugins might be complicated to achieve.
- currently, it is not possible to create stages inside the GGRS schedule or define system orderings.

## Licensing

Bevy_GGRS is dual-licensed under either

- [MIT License](./LICENSE-MIT): Also available [online](http://opensource.org/licenses/MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE): Also available [online](http://www.apache.org/licenses/LICENSE-2.0)

at your option.
