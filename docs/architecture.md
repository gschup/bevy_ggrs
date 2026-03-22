# Architecture

This document explains how bevy_ggrs works internally. It is aimed at contributors and advanced users who want to build custom snapshot plugins or understand the rollback loop.

## The Rollback Loop

Each Bevy frame, `GgrsPlugin` runs a single system — `run_ggrs_schedules` — in `PreUpdate` (or whichever schedule was configured). That system:

1. Accumulates real-world delta time into a fixed-timestep accumulator.
2. Polls the active `Session` for remote input.
3. While enough time has accumulated, calls `session.advance_frame()` and processes the returned `GgrsRequest` list.

GGRS produces three kinds of requests, each mapped to a Bevy schedule:

| GGRS Request      | bevy_ggrs Schedule | What it does                                  |
|-------------------|--------------------|-----------------------------------------------|
| `SaveGameState`   | `SaveWorld`        | Snapshot the current world state              |
| `LoadGameState`   | `LoadWorld`        | Restore the world to a previously saved frame |
| `AdvanceFrame`    | `AdvanceWorld`     | Simulate one frame of game logic              |

A normal frame (no rollback needed) produces exactly one `SaveGameState` followed by one `AdvanceFrame`. A rollback produces one `LoadGameState` to rewind, then a sequence of `AdvanceFrame` + `SaveGameState` pairs to re-simulate up to the current frame.

### Schedule Order Within a Frame

```
PreUpdate
└─ run_ggrs_schedules
   ├─ SaveWorld          (snapshot current state)
   │  ├─ SaveWorldSystems::Checksum   (compute per-type ChecksumParts)
   │  │  └─ [ChecksumPlugin aggregates parts into Checksum]
   │  └─ SaveWorldSystems::Snapshot   (write snapshot storage)
   │
   ├─ LoadWorld          (restore to rollback frame, if needed)
   │  ├─ LoadWorldSystems::Entity     (reconcile entity set, build RollbackEntityMap)
   │  ├─ LoadWorldSystems::EntityFlush
   │  ├─ LoadWorldSystems::Data       (restore component/resource values)
   │  ├─ LoadWorldSystems::DataFlush
   │  └─ LoadWorldSystems::Mapping    (remap stale Entity references via MapEntities)
   │
   └─ AdvanceWorld       (run one frame of game logic)
      ├─ AdvanceWorldSystems::First   (pre-frame setup, e.g. GgrsTime update)
      ├─ [ApplyDeferred]
      ├─ AdvanceWorldSystems::Main    (runs GgrsSchedule — your game logic)
      ├─ [ApplyDeferred]
      └─ AdvanceWorldSystems::Last    (post-frame cleanup, e.g. restore Time<()>)
```

## Snapshot Storage

Each snapshotted type gets its own `GgrsSnapshots<For, As>` resource — a double-ended queue of `(frame, snapshot)` pairs stored newest-first.

- **Depth** is synced to `MaxPredictionWindow` before every save. This ensures the queue is always deep enough to roll back to any frame GGRS might request.
- **Confirmation** — when GGRS confirms a frame, `ConfirmedFrameCount` is updated and old snapshots are pruned.
- **Rollback** — `GgrsSnapshots::rollback(frame)` advances the front of the queue to the target frame, discarding newer snapshots.

For components, `GgrsComponentSnapshots<C, As>` wraps `GgrsSnapshots` and stores a `GgrsComponentSnapshot<C, As>` per frame — a `HashMap<RollbackId, As>` mapping each rollback entity to its stored value at that frame.

For resources, `GgrsResourceSnapshots<R, As>` stores `Option<As>` per frame, where `None` means the resource was absent.

## Snapshot Strategies

A `Strategy` defines the serialise/deserialise contract for a type:

| Strategy          | Requirement          | Notes                                    |
|-------------------|----------------------|------------------------------------------|
| `CopyStrategy`    | `Copy`               | Cheapest — bitwise copy                  |
| `CloneStrategy`   | `Clone`              | Heap-allocating clone each frame         |
| `ReflectStrategy` | `Reflect + FromWorld`| Uses dynamic reflection; slowest         |

Strategies are passed as type parameters to `ComponentSnapshotPlugin<S>` and `ResourceSnapshotPlugin<S>`. The `RollbackApp` convenience methods (`rollback_component_with_copy`, etc.) select the right strategy for you.

## Entity Identity

Bevy `Entity` IDs are not stable across despawn/respawn cycles. bevy_ggrs solves this with two components:

- **`Rollback`** — a marker component. Add it to any entity that should be saved and rolled back. An `on_add` hook automatically assigns a `RollbackId` and registers the entity in `RollbackOrdered`.
- **`RollbackId`** — an immutable component whose value is the `Entity` ID at the time `Rollback` was first added. Used as the key in `GgrsComponentSnapshot`. Stays constant for the logical lifetime of the entity even if it is despawned and re-created.
- **`RollbackOrdered`** — a resource that maintains a stable, insertion-ordered list of all `RollbackId`s ever seen (including despawned entities). Used by checksum plugins to produce deterministic per-entity hashes.

### Entity Reconciliation During LoadWorld

`EntitySnapshotPlugin` compares the live entity set against the snapshot and:

- **Entity exists in both** — records the ID mapping `current → snapshot` (IDs may differ after a respawn).
- **Entity in snapshot only** — spawns a fresh entity with the same `Rollback` + `RollbackId`, records `new_id → old_id`.
- **Entity in world only** — despawns it (it didn't exist at the rollback target frame).

All mappings are stored in `RollbackEntityMap` and used during `LoadWorldSystems::Mapping` to fix up any component or resource that holds stale `Entity` references.

## Checksum Pipeline

Every registered type contributes a `ChecksumPart` (a `u128` stored as a component flagged with `ChecksumFlag<T>`) to the running frame checksum:

1. During `SaveWorldSystems::Checksum`, each type's plugin computes its hash and upserts a `ChecksumPart` entity.
2. After that set, `ChecksumPlugin::update` XORs all parts together into the `Checksum` resource.
3. `run_ggrs_schedules` reads `Checksum` after `SaveWorld` and forwards it to GGRS via `cell.save(frame, None, checksum)`.

GGRS compares checksums from all peers and fires `GgrsEvent::DesyncDetected` (P2P) or `SyncTestMismatch` (SyncTest) if they diverge.

## Time

`GgrsTimePlugin` provides `Time<GgrsTime>`, a deterministic clock that advances by exactly `1 / RollbackFrameRate` seconds per rollback frame. Inside `GgrsSchedule`, the default `Time<()>` is replaced with `Time<GgrsTime>` so that systems using `Res<Time>` automatically get the rolled-back time. At the end of `AdvanceWorld`, `Time<()>` is restored to `Time<Virtual>`.

## Plugin Composition

```
GgrsPlugin
├─ SnapshotPlugin
│  ├─ SnapshotSetPlugin          (configures system sets and ApplyDeferred barriers)
│  ├─ EntitySnapshotPlugin       (entity reconciliation + RollbackEntityMap)
│  ├─ ResourceSnapshotPlugin     (RollbackOrdered snapshot — required for entity checksums)
│  └─ ChildOfSnapshotPlugin      (hierarchy snapshot with inline entity remapping)
├─ ChecksumPlugin                (aggregates ChecksumParts into Checksum)
├─ EntityChecksumPlugin          (contributes entity-count checksum)
└─ GgrsTimePlugin                (deterministic Time<GgrsTime>)
```

User code adds further plugins via `RollbackApp`:

```rust
app.rollback_component_with_clone::<Transform>();    // → ComponentSnapshotPlugin<CloneStrategy<Transform>>
app.checksum_component_with_hash::<Health>();        // → ComponentChecksumPlugin<Health>
app.update_component_with_map_entities::<Target>();  // → ComponentMapEntitiesPlugin<Target>
```

## Adding a Custom Snapshot Plugin

To snapshot a type that doesn't fit the built-in strategies, implement `Strategy` and wrap it in `ComponentSnapshotPlugin` or `ResourceSnapshotPlugin`:

```rust
struct MyStrategy;

impl Strategy for MyStrategy {
    type Target = MyComponent;
    type Stored = MyStoredForm;

    fn store(target: &MyComponent) -> MyStoredForm { /* ... */ }
    fn load(stored: &MyStoredForm) -> MyComponent  { /* ... */ }
}

app.add_plugins(ComponentSnapshotPlugin::<MyStrategy>::default());
```
