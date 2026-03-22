# bevy_ggrs Open Items

## Blocked on PR #127 (Despawn via disabling component)

- **#127** — Awaiting co-maintainer review, then merge via web UI (purple merged icon).
  Rebased onto main, doc fixed, sealed trait and todo!() stubs removed. Branch pushed to fork.
  Has known open issue with Avian physics entity ID reallocation (out of scope for this PR).

- **#108 – SyncTest misses desyncs from unregistered components** — Close once #127 merges.
  PR #127 addresses this: entities are never truly removed, so all components survive rollback.

- **#72 – Sprite not respawned after rollback** — Coordinate with PR #127 before fixing.
  Entity ID mismatch during rollback breaks `Parent`/`Children` references, causing recursive despawn to lose components.

## Backlog (evaluate after above is done)

- **#123** – `app.rollback_required_components_with_clone<T>()`
- **#93** – Convenient way to end a session
- **#55** – More idiomatic checksums for float types (addition/hash instead of XOR)
- **#39** – Avoid triggering `Added`/`Changed` queries on all rollbacks (optimization)
- **#110** – Partial world state sync (large feature, consider post-stable)
- **GgrsLocal** (from closed PR #96) — rollback-aware `Local` system param; needs full implementation
