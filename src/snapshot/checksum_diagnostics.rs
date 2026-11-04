use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

use bevy::prelude::*;

use crate::{ClearSnapshots, RollbackFrameCount, SaveWorld, SaveWorldSystems};

/// Per-frame breakdown of checksum contributions.
///
/// Stored by [`ChecksumDiagnostics`] for each frame in the history ring buffer.
/// Contains both per-type aggregate checksums and per-entity per-type individual
/// hashes, allowing users to pinpoint exactly which entity and component type
/// diverged when a desync is detected.
#[derive(Debug, Default)]
pub struct FrameChecksumDetail {
    /// Per-type checksum: `type_name → ChecksumPart` value (u128).
    ///
    /// Includes components, resources, and the entity metadata checksum.
    /// Keys are `std::any::type_name::<T>()` for components/resources,
    /// or `"Entity"` for the entity count checksum.
    pub type_checksums: HashMap<&'static str, u128>,

    /// Per-entity per-type hash: `(rollback_order, type_name) → individual hash`.
    ///
    /// Only populated for component checksums (not resources).
    /// The `rollback_order` is the stable insertion index from [`RollbackOrdered`].
    /// The hash is the individual entity's contribution before XOR aggregation.
    pub entity_checksums: HashMap<(u64, &'static str), u64>,
}

/// Opt-in diagnostic resource that retains per-type and per-entity checksum
/// history in a ring buffer.
///
/// When present, the existing checksum plugins ([`ComponentChecksumPlugin`],
/// [`ResourceChecksumPlugin`], [`EntityChecksumPlugin`]) will record their
/// per-entity and per-type contributions into this resource each frame.
///
/// This resource is **not** rolled back — it is diagnostic state that persists
/// across rollback/resimulation cycles.
///
/// # Usage
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_ggrs::prelude::*;
/// # use bevy_ggrs::ChecksumDiagnosticsPlugin;
/// # let mut app = App::new();
/// // Enable diagnostics with 120-frame history (2 seconds at 60fps)
/// app.add_plugins(ChecksumDiagnosticsPlugin::default());
/// ```
///
/// Then, when a desync is detected, query the resource to inspect per-entity
/// checksums at the relevant frame.
///
/// [`ComponentChecksumPlugin`]: crate::ComponentChecksumPlugin
/// [`ResourceChecksumPlugin`]: crate::ResourceChecksumPlugin
/// [`EntityChecksumPlugin`]: crate::EntityChecksumPlugin
#[derive(Resource)]
pub struct ChecksumDiagnostics {
    history: HashMap<i32, FrameChecksumDetail>,
    history_length: usize,
    frame_order: VecDeque<i32>,
    /// Current frame being built. Protected by a mutex so that multiple
    /// checksum systems (which take `Res<Self>`, i.e. shared access) can
    /// write concurrently.
    pending: Mutex<Option<(i32, FrameChecksumDetail)>>,
}

impl ChecksumDiagnostics {
    /// Create a new diagnostics resource with the given history length.
    pub fn new(history_length: usize) -> Self {
        Self {
            history: HashMap::new(),
            history_length,
            frame_order: VecDeque::new(),
            pending: Mutex::new(None),
        }
    }

    /// Look up the checksum detail for a specific frame.
    pub fn frame(&self, frame: i32) -> Option<&FrameChecksumDetail> {
        self.history.get(&frame)
    }

    /// Iterate over all stored frames and their details, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &FrameChecksumDetail)> {
        self.history.iter().map(|(&f, d)| (f, d))
    }

    /// Number of frames currently in the history.
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    /// Clear all history. Called when snapshots are reset (e.g. session restart).
    pub fn clear(&mut self) {
        self.history.clear();
        self.frame_order.clear();
        *self.pending.get_mut().unwrap() = None;
    }

    /// Prepare a new frame entry. Called by [`begin_diagnostics_frame`] before
    /// the `Checksum` system set runs.
    pub fn begin_frame(&mut self, frame: i32) {
        *self.pending.get_mut().unwrap() = Some((frame, FrameChecksumDetail::default()));
    }

    /// Record a per-type aggregate checksum. Called by checksum plugins via
    /// shared `Res<Self>` access (locks the mutex).
    pub fn record_type_checksum(&self, type_name: &'static str, checksum: u128) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some((_, ref mut detail)) = *pending
        {
            detail.type_checksums.insert(type_name, checksum);
        }
    }

    /// Record a per-entity hash contribution. Called by [`ComponentChecksumPlugin`]
    /// via shared `Res<Self>` access (locks the mutex).
    ///
    /// [`ComponentChecksumPlugin`]: crate::ComponentChecksumPlugin
    pub fn record_entity_hash(&self, type_name: &'static str, rollback_order: u64, hash: u64) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some((_, ref mut detail)) = *pending
        {
            detail
                .entity_checksums
                .insert((rollback_order, type_name), hash);
        }
    }

    /// Move the pending frame into the history ring buffer, evicting the oldest
    /// frame if over capacity. Called by [`commit_diagnostics_frame`] after the
    /// `Checksum` system set completes.
    pub fn commit_frame(&mut self) {
        let pending = self.pending.get_mut().unwrap().take();
        if let Some((frame, detail)) = pending {
            // If this frame was already in history (resimulation), remove the
            // old entry from frame_order to avoid duplicates.
            if self.history.contains_key(&frame) {
                self.frame_order.retain(|&f| f != frame);
            }
            self.history.insert(frame, detail);
            self.frame_order.push_back(frame);

            // Evict oldest frames beyond capacity.
            while self.frame_order.len() > self.history_length {
                if let Some(oldest) = self.frame_order.pop_front() {
                    self.history.remove(&oldest);
                }
            }
        }
    }
}

/// Opt-in plugin that enables checksum diagnostics.
///
/// When added, the existing checksum plugins will record per-type and per-entity
/// hash contributions each frame into a [`ChecksumDiagnostics`] resource.
///
/// Recording runs while the resource exists. It costs a map insert per
/// checksummed entity per frame, so an app that compares checksums only in
/// some sessions can remove the resource outside them and insert
/// [`ChecksumDiagnostics::new`] when one starts.
///
/// # Examples
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_ggrs::prelude::*;
/// # use bevy_ggrs::ChecksumDiagnosticsPlugin;
/// # let mut app = App::new();
/// app.add_plugins(ChecksumDiagnosticsPlugin { history_length: 120 });
/// ```
pub struct ChecksumDiagnosticsPlugin {
    /// Number of frames to retain in the ring buffer. Default: 120.
    pub history_length: usize,
}

impl Default for ChecksumDiagnosticsPlugin {
    fn default() -> Self {
        Self {
            history_length: 120,
        }
    }
}

impl Plugin for ChecksumDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ChecksumDiagnostics::new(self.history_length));

        app.add_systems(
            SaveWorld,
            (
                begin_diagnostics_frame.before(SaveWorldSystems::Checksum),
                commit_diagnostics_frame
                    .after(SaveWorldSystems::Checksum)
                    .before(SaveWorldSystems::Snapshot),
            )
                .run_if(resource_exists::<ChecksumDiagnostics>),
        );
        app.add_observer(
            |_trigger: On<ClearSnapshots>, diagnostics: Option<ResMut<ChecksumDiagnostics>>| {
                if let Some(mut diagnostics) = diagnostics {
                    diagnostics.clear();
                }
            },
        );
    }
}

/// Prepare a fresh [`FrameChecksumDetail`] for the current frame.
fn begin_diagnostics_frame(
    mut diagnostics: ResMut<ChecksumDiagnostics>,
    frame_count: Res<RollbackFrameCount>,
) {
    diagnostics.begin_frame(frame_count.0);
}

/// Move the pending frame detail into the history ring buffer.
fn commit_diagnostics_frame(mut diagnostics: ResMut<ChecksumDiagnostics>) {
    diagnostics.commit_frame();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_evicts_oldest() {
        let mut diag = ChecksumDiagnostics::new(3);

        for frame in 0..5 {
            diag.begin_frame(frame);
            diag.record_type_checksum("TestType", frame as u128 * 100);
            diag.commit_frame();
        }

        // Should only have frames 2, 3, 4
        assert_eq!(diag.len(), 3);
        assert!(diag.frame(0).is_none());
        assert!(diag.frame(1).is_none());
        assert!(diag.frame(2).is_some());
        assert!(diag.frame(3).is_some());
        assert!(diag.frame(4).is_some());
    }

    #[test]
    fn resimulation_overwrites_frame() {
        let mut diag = ChecksumDiagnostics::new(10);

        // Simulate frame 5
        diag.begin_frame(5);
        diag.record_type_checksum("Pos", 0xAAAA);
        diag.commit_frame();

        // Resimulate frame 5 with different data
        diag.begin_frame(5);
        diag.record_type_checksum("Pos", 0xBBBB);
        diag.commit_frame();

        assert_eq!(diag.len(), 1);
        let detail = diag.frame(5).unwrap();
        assert_eq!(detail.type_checksums["Pos"], 0xBBBB);
    }

    #[test]
    fn per_entity_recording() {
        let mut diag = ChecksumDiagnostics::new(10);

        diag.begin_frame(0);
        diag.record_entity_hash("Position", 0, 0x1111);
        diag.record_entity_hash("Position", 1, 0x2222);
        diag.record_entity_hash("Velocity", 0, 0x3333);
        diag.record_type_checksum("Position", 0xAABB);
        diag.record_type_checksum("Velocity", 0xCCDD);
        diag.commit_frame();

        let detail = diag.frame(0).unwrap();
        assert_eq!(detail.entity_checksums[&(0, "Position")], 0x1111);
        assert_eq!(detail.entity_checksums[&(1, "Position")], 0x2222);
        assert_eq!(detail.entity_checksums[&(0, "Velocity")], 0x3333);
        assert_eq!(detail.type_checksums["Position"], 0xAABB);
        assert_eq!(detail.type_checksums["Velocity"], 0xCCDD);
    }

    #[test]
    fn no_pending_is_safe() {
        let diag = ChecksumDiagnostics::new(10);
        // Recording without begin_frame should be a no-op, not panic
        diag.record_type_checksum("Foo", 42);
        diag.record_entity_hash("Foo", 0, 42);
    }
}
