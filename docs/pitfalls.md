# Rollback Pitfalls

This guide covers common mistakes when writing systems for a rollback game. If your game desyncs or behaves unexpectedly, check here first.

## Non-deterministic Query Iteration

Bevy does not guarantee a stable iteration order for queries. If two systems process entities in different orders, your game state will diverge across clients.

**Always sort queries that mutate state:**

```rust
// Sort by a stable component value
fn move_players(
    mut query: Query<(&Player, &mut Transform), With<RollbackId>>,
) {
    let mut players: Vec<_> = query.iter_mut().collect();
    players.sort_by_key(|(player, _)| player.handle);
    for (_, mut transform) in players {
        // ...
    }
}
```

If your entities don't have a natural sort key, use `RollbackOrdered`:

```rust
fn move_players(
    mut query: Query<(&RollbackId, &mut Transform)>,
    order: Res<RollbackOrdered>,
) {
    let mut players: Vec<_> = query.iter_mut().collect();
    players.sort_by_key(|(id, _)| order.order(*id));
    for (_, mut transform) in players {
        // ...
    }
}
```

## Events

Bevy's `Events<T>` resource is **not snapshotted**. Events fired during a frame that gets rolled back will not be re-fired during resimulation, and events from the resimulated frames will not be visible to systems outside `GgrsSchedule`.

**Do not use `EventWriter` / `EventReader` inside `GgrsSchedule`.** Instead:

- Use a component or resource to communicate state changes between rollback systems.
- Only fire Bevy events from systems outside `GgrsSchedule` (e.g. in `Update`) based on snapshotted state.

## `Local<T>` in Rollback Systems

`Local<T>` is per-system state that is **not snapshotted**. Using it inside `GgrsSchedule` will cause the local value to drift between the original simulation and resimulation.

Use a `Component` or `Resource` registered for rollback instead.

## Reading Input Directly

Do not read `ButtonInput<KeyCode>` or similar Bevy input resources inside `GgrsSchedule`. Input resources are not snapshotted and will contain the current frame's input during resimulation, not the input from the frame being resimulated.

Always read inputs from `PlayerInputs<T>`, which bevy_ggrs provides correctly for each simulated frame:

```rust
fn move_player(
    inputs: Res<PlayerInputs<GgrsConfig>>,
    mut query: Query<(&Player, &mut Transform)>,
) {
    for (player, mut transform) in &mut query {
        let (input, _status) = inputs[player.handle];
        // use input here
    }
}
```

## Unregistered Components and Resources

Only components and resources explicitly registered via `rollback_component_with_*` or `rollback_resource_with_*` are snapshotted. Any state stored elsewhere will not be restored on rollback, causing silent desyncs.

If an entity is despawned and re-created during rollback, **all** of its components must be registered — otherwise the resimulated entity will be missing components.

Consider using `SyncTestSession` with `checksum_component_with_hash` to catch these issues early.

## Entity ID Instability

Entity IDs can change when entities are despawned and re-created during rollback. Do not store raw `Entity` handles across frames unless you implement `MapEntities` on your component, which remaps stale IDs after rollback:

```rust
impl MapEntities for MyComponent {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.target = entity_mapper.get_mapped(self.target);
    }
}
```

## `GlobalTransform`

`GlobalTransform` is only updated in `PostUpdate`, which runs outside `GgrsSchedule`. Avoid reading `GlobalTransform` in rollback systems — use `Transform` directly instead.

## Change Detection

Every snapshot restore triggers change detection on all restored components. Systems that react to `Changed<T>` will fire after every rollback, which can cause performance issues or unintended behavior. Be aware of this in systems like transform propagation.
