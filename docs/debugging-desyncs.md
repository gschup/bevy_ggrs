# Debugging Desyncs

A desync means two clients have diverged — the same inputs have produced different game states. This guide covers how to detect and diagnose desyncs.

## SyncTest Session

The easiest way to catch desyncs during development is to use `SyncTestSession` instead of `P2PSession`. It simulates rollback locally by re-running the last `check_distance` frames every update and comparing checksums.

```rust
let session = SessionBuilder::<GgrsConfig>::new()
    .with_num_players(1)
    .unwrap()
    // re-simulate 7 frames back every update and compare checksums
    .with_check_distance(7)
    .add_player(PlayerType::Local, 0)
    .unwrap()
    .start_synctest_session()
    .unwrap();
```

A `SyncTestMismatch` trigger fires if checksums diverge. You can observe it:

```rust
app.observe(|trigger: Trigger<SyncTestMismatch>| {
    error!(
        "Desync detected! Frame: {}, mismatched frames: {:?}",
        trigger.event().current_frame,
        trigger.event().mismatched_frames
    );
});
```

## Adding Checksums

bevy_ggrs only detects desyncs for state that is checksummed. Register checksums alongside your rollback state:

```rust
app.rollback_component_with_copy::<Health>()
   .checksum_component_with_hash::<Health>();

app.rollback_resource_with_clone::<Score>()
   .add_plugins(ResourceChecksumPlugin::<Score>::default());
```

The more state you checksum, the more precisely SyncTest can locate a desync.

## Common Causes

**Non-deterministic query order** — See [pitfalls.md](./pitfalls.md). The most common cause of desyncs.

**Unregistered components** — An entity is resimulated without all its components, changing behavior. Register everything that affects gameplay.

**`Local<T>` in rollback systems** — Diverges between original and resimulation. Use a snapshotted resource instead.

**Floating-point non-determinism** — Rust's `f32`/`f64` arithmetic is deterministic on the same platform, but not across different CPUs or operating systems. If you need cross-platform play, consider fixed-point math.

**Randomness** — If your game uses `rand`, seed the RNG from confirmed game state and register it for rollback. Never use OS randomness inside `GgrsSchedule`.

**Time-dependent logic** — Do not use `Res<Time>` inside `GgrsSchedule`. Use `RollbackFrameCount` to track logical game time instead.

## P2P Desync Detection

For P2P sessions, enable desync detection via GGRS:

```rust
SessionBuilder::<GgrsConfig>::new()
    .with_desync_detection_mode(ggrs::DesyncDetection::On { interval: 10 })
    // ...
```

When a desync is detected, GGRS fires a `GgrsEvent::DesyncDetected` event containing the local and remote checksums and the frame number. You can observe this via the `Session` resource.

## Known Limitations

- **Snapshot access during desync**: Snapshots are pruned as frames are confirmed, which happens before desync detection is reported. It is not currently possible to inspect the snapshot of the diverging frame directly.
- **SyncTest does not emit `GgrsEvent::DesyncDetected`**: It fires `SyncTestMismatch` instead (see above).
