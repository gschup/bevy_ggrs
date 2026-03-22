# Example Instructions

Gathered here are some additional instructions on how to build and run the examples.

## BoxGame

BoxGame is a very basic 2-4 player game example with each player controlling a coloured box.
There is no real game, just movement with ice physics. Optionally,
you can specify spectators.

- W, A, S, D to move around

### Important Disclaimer - Determinism

BoxGame uses `f32` arithmetic (velocity integration, `clamp_length_max`) which is deterministic
on the same platform but may produce different results across different CPU architectures or
operating systems. I fully expect this example to desync when compiled on two different platforms.
This is intentional. If you plan to implement your own deterministic game, consider fixed-point
math or limit cross-platform play to the same architecture.

### Launching BoxGame P2P and Spectator

The P2P example is launched by command-line arguments:

- `--local-port / -l`: local port the client is listening to
- `--players / -p`: a list of player addresses, with the local player being identified by `localhost`
- `--spectators / -s`: a list of spectator addresses. This client will act as a host for these spectators

For the spectator, the following command-line arguments exist:

- `--local-port / -l`: local port the client is listening to
- `--num-players / -n`: number of players that will participate in the game
- `--host / -h`: address of the host

For example, to run a two-player game, run these commands in separate terminals:

```shell
cargo run --example box_game_p2p -- --local-port 7000 --players localhost 127.0.0.1:7001
cargo run --example box_game_p2p -- --local-port 7001 --players 127.0.0.1:7000 localhost
```

In order to run a two-player game with a spectator,
run these commands in separate terminals:

```shell
cargo run --example box_game_p2p -- --local-port 7000 --players localhost 127.0.0.1:7001 --spectators 127.0.0.1:7002
cargo run --example box_game_p2p -- --local-port 7001 --players 127.0.0.1:7000 localhost
cargo run --example box_game_spectator -- --local-port 7002 --num-players 2 --host 127.0.0.1:7000 
```

## BoxGame SyncTest

The same game, but without network functionality.
Instead, the SyncTestSession focuses on simulating rollbacks and comparing checksums.

### Launching BoxGame SyncTest

BoxGame SyncTest is launched by a single command-line argument:

- `--num-players / -n`: number of players that will participate in the game
- `--check-distance / -c`: number of frames that will be rolled back and resimulated each frame

```shell
cargo run --example box_game_synctest -- --num-players 2 --check-distance 7
```

## Particles (Stress Test)

A P2P stress test that spawns large numbers of particles to measure rollback performance.
Supports switching between clone/copy-based and reflect-based rollback via `--reflect` to
compare their overhead.

### Launching Particles

```
--local-port / -l      local UDP port to bind
--players / -p         list of player addresses; use "localhost" for yourself
--spectators / -s      (optional) list of spectator addresses
--input-delay          input delay in frames (default: 2)
--rate / -n            particles spawned per frame when Space is held (default: 100)
--fps                  simulation frame rate (default: 60)
--max-prediction       max prediction window in frames (default: 8)
--reflect              use reflect-based rollback instead of clone/copy
--desync-detection-interval  how often to exchange checksums (default: 10; 0 = off)
--continue-after-desync      log desyncs instead of panicking
```

Press **Space** to spawn particles. Press **N** to trigger a no-op rollback.

For example, to run a two-player stress test:

```shell
cargo run --release --example particles -- --local-port 7000 --players localhost 127.0.0.1:7001
cargo run --release --example particles -- --local-port 7001 --players 127.0.0.1:7000 localhost
```
