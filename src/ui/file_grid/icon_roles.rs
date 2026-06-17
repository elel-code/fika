use std::collections::HashMap;
use std::time::Duration;

use fika_core::PaneId;

pub(crate) const DOLPHIN_ICON_SIZE_UPDATE_DELAY: Duration = Duration::from_millis(300);

#[derive(Debug, Default)]
pub(crate) struct PaneIconRoleSizeState {
    role_sizes: HashMap<PaneId, f32>,
    update_versions: HashMap<PaneId, u64>,
}

impl PaneIconRoleSizeState {
    pub(crate) fn role_size_or(&self, pane_id: PaneId, fallback_icon_size: f32) -> f32 {
        self.role_sizes
            .get(&pane_id)
            .copied()
            .unwrap_or(fallback_icon_size)
    }

    pub(crate) fn begin_deferred_update(
        &mut self,
        pane_id: PaneId,
        previous_icon_size: f32,
    ) -> u64 {
        self.role_sizes.entry(pane_id).or_insert(previous_icon_size);

        let version = self
            .update_versions
            .entry(pane_id)
            .and_modify(|version| *version = version.wrapping_add(1).max(1))
            .or_insert(1);
        *version
    }

    pub(crate) fn is_current_deferred_update(&self, pane_id: PaneId, version: u64) -> bool {
        self.update_versions.get(&pane_id) == Some(&version)
    }

    pub(crate) fn commit(&mut self, pane_id: PaneId, icon_size: f32) -> bool {
        let changed = self
            .role_sizes
            .get(&pane_id)
            .is_none_or(|current| (current - icon_size).abs() > f32::EPSILON);
        if !changed {
            return false;
        }

        self.role_sizes.insert(pane_id, icon_size);
        true
    }

    pub(crate) fn remove_pane(&mut self, pane_id: PaneId) {
        self.role_sizes.remove(&pane_id);
        self.update_versions.remove(&pane_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u64) -> PaneId {
        PaneId(id)
    }

    #[test]
    fn role_size_falls_back_until_deferred_or_committed() {
        let state = PaneIconRoleSizeState::default();

        assert_eq!(state.role_size_or(pane(1), 48.0), 48.0);
    }

    #[test]
    fn deferred_updates_keep_original_role_size_and_increment_version() {
        let mut state = PaneIconRoleSizeState::default();

        let first_version = state.begin_deferred_update(pane(1), 48.0);
        let second_version = state.begin_deferred_update(pane(1), 64.0);

        assert_eq!(first_version, 1);
        assert_eq!(second_version, 2);
        assert_eq!(state.role_size_or(pane(1), 96.0), 48.0);
        assert!(!state.is_current_deferred_update(pane(1), first_version));
        assert!(state.is_current_deferred_update(pane(1), second_version));
    }

    #[test]
    fn commit_reports_only_real_size_changes() {
        let mut state = PaneIconRoleSizeState::default();

        assert!(state.commit(pane(1), 48.0));
        assert!(!state.commit(pane(1), 48.0));
        assert!(state.commit(pane(1), 64.0));
        assert_eq!(state.role_size_or(pane(1), 48.0), 64.0);
    }

    #[test]
    fn remove_pane_clears_deferred_state() {
        let mut state = PaneIconRoleSizeState::default();

        let version = state.begin_deferred_update(pane(1), 48.0);
        state.remove_pane(pane(1));

        assert_eq!(state.role_size_or(pane(1), 64.0), 64.0);
        assert!(!state.is_current_deferred_update(pane(1), version));
    }
}
