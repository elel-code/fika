use std::collections::HashSet;

use crate::SurfaceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GestureSubscriptionChange {
    Unchanged,
    KeepSeats,
    AttachSeats,
    DetachSeats,
}

/// Surfaces that currently request semantic pointer gestures.
///
/// The manager is global and gesture objects are pointer-scoped, so only the
/// empty/non-empty transition requires protocol work. Individual membership
/// is also used to reject gestures focused on an unsubscribed surface.
#[derive(Debug, Default)]
pub(crate) struct PointerGestureSubscriptions(HashSet<SurfaceId>);

impl PointerGestureSubscriptions {
    pub(crate) fn set(&mut self, surface: SurfaceId, enabled: bool) -> GestureSubscriptionChange {
        let was_active = self.is_active();
        let changed = if enabled {
            self.0.insert(surface)
        } else {
            self.0.remove(&surface)
        };
        if !changed {
            return GestureSubscriptionChange::Unchanged;
        }
        match (was_active, self.is_active()) {
            (false, true) => GestureSubscriptionChange::AttachSeats,
            (true, false) => GestureSubscriptionChange::DetachSeats,
            _ => GestureSubscriptionChange::KeepSeats,
        }
    }

    pub(crate) fn remove_surface(&mut self, surface: SurfaceId) -> GestureSubscriptionChange {
        self.set(surface, false)
    }

    pub(crate) fn contains(&self, surface: SurfaceId) -> bool {
        self.0.contains(&surface)
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_empty_transitions_change_seat_objects() {
        let first = SurfaceId(2);
        let second = SurfaceId(5);
        let mut subscriptions = PointerGestureSubscriptions::default();

        assert_eq!(
            subscriptions.set(first, true),
            GestureSubscriptionChange::AttachSeats
        );
        assert_eq!(
            subscriptions.set(second, true),
            GestureSubscriptionChange::KeepSeats
        );
        assert_eq!(
            subscriptions.set(first, false),
            GestureSubscriptionChange::KeepSeats
        );
        assert!(subscriptions.contains(second));
        assert_eq!(
            subscriptions.remove_surface(second),
            GestureSubscriptionChange::DetachSeats
        );
        assert!(!subscriptions.is_active());
    }

    #[test]
    fn duplicate_updates_are_noops() {
        let surface = SurfaceId(4);
        let mut subscriptions = PointerGestureSubscriptions::default();

        assert_eq!(
            subscriptions.set(surface, false),
            GestureSubscriptionChange::Unchanged
        );
        assert_eq!(
            subscriptions.set(surface, true),
            GestureSubscriptionChange::AttachSeats
        );
        assert_eq!(
            subscriptions.set(surface, true),
            GestureSubscriptionChange::Unchanged
        );
    }
}
