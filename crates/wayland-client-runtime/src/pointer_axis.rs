use smithay_client_toolkit::reexports::client::protocol::wl_pointer;
use smithay_client_toolkit::seat::pointer::AxisScroll;

/// Physical source of a pointer axis frame.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerAxisSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}

/// Relationship between physical motion and the compositor's axis direction.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerAxisDirection {
    Identical,
    Inverted,
}

/// All values accumulated for one axis in a `wl_pointer.frame`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerAxisValue {
    /// Continuous compositor-coordinate delta using Wayland's sign convention.
    pub continuous: f64,
    /// Deprecated whole-step delta supplied by older compositors.
    pub discrete: i32,
    /// High-resolution wheel delta; 120 units represent one logical step.
    pub value120: i32,
    /// Physical direction relative to the compositor axis direction.
    pub relative_direction: Option<PointerAxisDirection>,
    /// The device reported that continuous scrolling stopped on this axis.
    pub stopped: bool,
}

impl PointerAxisValue {
    /// Preferred logical step delta for wheel-style consumers.
    ///
    /// `axis_value120` takes precedence over deprecated discrete values. A
    /// continuous-only frame returns `None` instead of inventing a conversion.
    pub fn logical_steps(self) -> Option<f64> {
        if self.value120 != 0 {
            Some(f64::from(self.value120) / 120.0)
        } else if self.discrete != 0 {
            Some(f64::from(self.discrete))
        } else {
            None
        }
    }

    pub fn has_motion(self) -> bool {
        self.continuous != 0.0 || self.discrete != 0 || self.value120 != 0
    }
}

pub(crate) fn map_axis_value(value: AxisScroll) -> PointerAxisValue {
    PointerAxisValue {
        continuous: value.absolute,
        discrete: value.discrete,
        value120: value.value120,
        relative_direction: value.relative_direction.and_then(map_axis_direction),
        stopped: value.stop,
    }
}

pub(crate) fn map_axis_source(source: Option<wl_pointer::AxisSource>) -> Option<PointerAxisSource> {
    match source? {
        wl_pointer::AxisSource::Wheel => Some(PointerAxisSource::Wheel),
        wl_pointer::AxisSource::Finger => Some(PointerAxisSource::Finger),
        wl_pointer::AxisSource::Continuous => Some(PointerAxisSource::Continuous),
        wl_pointer::AxisSource::WheelTilt => Some(PointerAxisSource::WheelTilt),
        _ => None,
    }
}

fn map_axis_direction(
    direction: wl_pointer::AxisRelativeDirection,
) -> Option<PointerAxisDirection> {
    match direction {
        wl_pointer::AxisRelativeDirection::Identical => Some(PointerAxisDirection::Identical),
        wl_pointer::AxisRelativeDirection::Inverted => Some(PointerAxisDirection::Inverted),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value120_preserves_partial_high_resolution_steps() {
        let value = PointerAxisValue {
            discrete: 2,
            value120: 30,
            ..Default::default()
        };

        assert_eq!(value.logical_steps(), Some(0.25));
        assert!(value.has_motion());
    }

    #[test]
    fn deprecated_discrete_steps_remain_a_fallback() {
        let value = PointerAxisValue {
            discrete: -2,
            ..Default::default()
        };

        assert_eq!(value.logical_steps(), Some(-2.0));
    }

    #[test]
    fn continuous_and_stop_only_frames_are_not_fabricated_as_steps() {
        let continuous = PointerAxisValue {
            continuous: 3.5,
            ..Default::default()
        };
        let stopped = PointerAxisValue {
            stopped: true,
            ..Default::default()
        };

        assert_eq!(continuous.logical_steps(), None);
        assert!(continuous.has_motion());
        assert_eq!(stopped.logical_steps(), None);
        assert!(!stopped.has_motion());
    }

    #[test]
    fn sctk_axis_fields_and_direction_are_preserved() {
        let value = AxisScroll {
            absolute: 4.0,
            discrete: 1,
            value120: 90,
            relative_direction: Some(wl_pointer::AxisRelativeDirection::Inverted),
            stop: true,
        };

        assert_eq!(
            map_axis_value(value),
            PointerAxisValue {
                continuous: 4.0,
                discrete: 1,
                value120: 90,
                relative_direction: Some(PointerAxisDirection::Inverted),
                stopped: true,
            }
        );
    }
}
