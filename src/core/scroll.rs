use super::view::ViewPoint;
use std::time::{Duration, Instant};

pub const SMOOTH_SCROLL_DURATION: Duration = Duration::from_millis(180);
pub const SMOOTH_SCROLL_FRAME: Duration = Duration::from_millis(16);

const KINETIC_MIN_VELOCITY: f32 = 120.0;
const KINETIC_STOP_VELOCITY: f32 = 18.0;
const KINETIC_MAX_VELOCITY: f32 = 4800.0;
const KINETIC_FRICTION_PER_FRAME: f32 = 0.86;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollBounds {
    pub max_x: f32,
    pub max_y: f32,
}

impl ScrollBounds {
    pub fn new(max_x: f32, max_y: f32) -> Self {
        Self {
            max_x: max_x.max(0.0),
            max_y: max_y.max(0.0),
        }
    }

    pub fn clamp(self, offset: ViewPoint) -> ViewPoint {
        ViewPoint {
            x: offset.x.clamp(0.0, self.max_x),
            y: offset.y.clamp(0.0, self.max_y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollAdvance {
    pub offset: ViewPoint,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothScroll {
    bounds: ScrollBounds,
    motion: ScrollMotion,
}

impl SmoothScroll {
    pub fn from_scroll_contents_by(
        current: ViewPoint,
        distance: ViewPoint,
        bounds: ScrollBounds,
        now: Instant,
    ) -> Self {
        Self {
            bounds,
            motion: ScrollMotion::Ease(EaseScroll {
                start: bounds.clamp(current),
                target: dolphin_scroll_end_offset(current, distance, bounds),
                started_at: now,
                duration: SMOOTH_SCROLL_DURATION,
                easing: ScrollEasing::InOutQuad,
            }),
        }
    }

    pub fn to_target(
        current: ViewPoint,
        target: ViewPoint,
        bounds: ScrollBounds,
        now: Instant,
    ) -> Self {
        Self {
            bounds,
            motion: ScrollMotion::Ease(EaseScroll {
                start: bounds.clamp(current),
                target: bounds.clamp(target),
                started_at: now,
                duration: SMOOTH_SCROLL_DURATION,
                easing: ScrollEasing::InOutQuad,
            }),
        }
    }

    pub fn scroll_contents_by(
        self,
        current: ViewPoint,
        distance: ViewPoint,
        bounds: ScrollBounds,
        now: Instant,
    ) -> Self {
        let (start, target, easing) = match self.motion {
            ScrollMotion::Ease(ease) => {
                let current_offset = self.offset_at(now).offset;
                let old_end_offset = ease.target;
                let (start_offset, end_offset) = dolphin_interrupted_offsets(
                    current_offset,
                    old_end_offset,
                    distance,
                    bounds,
                    SMOOTH_SCROLL_DURATION,
                );
                (start_offset, end_offset, ScrollEasing::OutQuad)
            }
            ScrollMotion::Kinetic(_) => {
                let current = bounds.clamp(current);
                let target = dolphin_scroll_end_offset(current, distance, bounds);
                (current, target, ScrollEasing::InOutQuad)
            }
        };
        Self {
            bounds,
            motion: ScrollMotion::Ease(EaseScroll {
                start,
                target,
                started_at: now,
                duration: SMOOTH_SCROLL_DURATION,
                easing,
            }),
        }
    }

    pub fn kinetic(velocity: ViewPoint, bounds: ScrollBounds, now: Instant) -> Option<Self> {
        let velocity = clamp_velocity(velocity);
        if velocity_magnitude(velocity) < KINETIC_MIN_VELOCITY {
            return None;
        }
        Some(Self {
            bounds,
            motion: ScrollMotion::Kinetic(KineticScroll {
                velocity,
                last_at: now,
            }),
        })
    }

    pub fn bounds(self) -> ScrollBounds {
        self.bounds
    }

    pub fn maximum_matches(self, bounds: ScrollBounds) -> bool {
        scroll_bound_eq(self.bounds.max_x, bounds.max_x)
            && scroll_bound_eq(self.bounds.max_y, bounds.max_y)
    }

    pub fn target_offset(self) -> Option<ViewPoint> {
        match self.motion {
            ScrollMotion::Ease(ease) => Some(ease.target),
            ScrollMotion::Kinetic(_) => None,
        }
    }

    pub fn offset_at(self, now: Instant) -> ScrollAdvance {
        match self.motion {
            ScrollMotion::Ease(ease) => ease.offset_at(now, self.bounds),
            ScrollMotion::Kinetic(_) => ScrollAdvance {
                offset: ViewPoint::default(),
                active: true,
            },
        }
    }

    pub fn advance(&mut self, current: ViewPoint, now: Instant) -> ScrollAdvance {
        match &mut self.motion {
            ScrollMotion::Ease(ease) => ease.offset_at(now, self.bounds),
            ScrollMotion::Kinetic(kinetic) => kinetic.advance(current, self.bounds, now),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollMotion {
    Ease(EaseScroll),
    Kinetic(KineticScroll),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EaseScroll {
    start: ViewPoint,
    target: ViewPoint,
    started_at: Instant,
    duration: Duration,
    easing: ScrollEasing,
}

impl EaseScroll {
    fn offset_at(self, now: Instant, bounds: ScrollBounds) -> ScrollAdvance {
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed >= self.duration {
            return ScrollAdvance {
                offset: bounds.clamp(self.target),
                active: false,
            };
        }

        let ratio = duration_ratio(elapsed, self.duration);
        ScrollAdvance {
            offset: bounds.clamp(lerp_point(
                self.start,
                self.target,
                self.easing.apply(ratio),
            )),
            active: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct KineticScroll {
    velocity: ViewPoint,
    last_at: Instant,
}

impl KineticScroll {
    fn advance(&mut self, current: ViewPoint, bounds: ScrollBounds, now: Instant) -> ScrollAdvance {
        let elapsed = now.saturating_duration_since(self.last_at);
        self.last_at = now;
        let seconds = elapsed.as_secs_f32().min(0.05);
        if seconds <= f32::EPSILON {
            return ScrollAdvance {
                offset: bounds.clamp(current),
                active: true,
            };
        }

        let mut velocity = self.velocity;
        let mut offset = bounds.clamp(ViewPoint {
            x: current.x + velocity.x * seconds,
            y: current.y + velocity.y * seconds,
        });
        if offset.x <= 0.0 || offset.x >= bounds.max_x {
            velocity.x = 0.0;
            offset.x = offset.x.clamp(0.0, bounds.max_x);
        }
        if offset.y <= 0.0 || offset.y >= bounds.max_y {
            velocity.y = 0.0;
            offset.y = offset.y.clamp(0.0, bounds.max_y);
        }

        let decay = KINETIC_FRICTION_PER_FRAME.powf(seconds / SMOOTH_SCROLL_FRAME.as_secs_f32());
        velocity.x *= decay;
        velocity.y *= decay;
        self.velocity = velocity;

        ScrollAdvance {
            offset,
            active: velocity_magnitude(velocity) >= KINETIC_STOP_VELOCITY,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollEasing {
    InOutQuad,
    OutQuad,
}

impl ScrollEasing {
    fn apply(self, ratio: f32) -> f32 {
        let ratio = ratio.clamp(0.0, 1.0);
        match self {
            Self::InOutQuad if ratio < 0.5 => 2.0 * ratio * ratio,
            Self::InOutQuad => 1.0 - (-2.0 * ratio + 2.0).powi(2) / 2.0,
            Self::OutQuad => 1.0 - (1.0 - ratio).powi(2),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollDragTracker {
    previous: Option<ScrollDragSample>,
    last: Option<ScrollDragSample>,
    velocity: ViewPoint,
}

impl ScrollDragTracker {
    pub fn sample(&mut self, offset: ViewPoint, at: Instant) {
        if let Some(last) = self.last {
            let elapsed = at.saturating_duration_since(last.at).as_secs_f32();
            if elapsed > f32::EPSILON {
                self.previous = Some(last);
                self.velocity = clamp_velocity(ViewPoint {
                    x: (offset.x - last.offset.x) / elapsed,
                    y: (offset.y - last.offset.y) / elapsed,
                });
            }
        }
        self.last = Some(ScrollDragSample { offset, at });
    }

    pub fn velocity(self) -> ViewPoint {
        if self.previous.is_some() {
            self.velocity
        } else {
            ViewPoint::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollDragSample {
    offset: ViewPoint,
    at: Instant,
}

fn dolphin_interrupted_offsets(
    current: ViewPoint,
    old_end: ViewPoint,
    distance: ViewPoint,
    bounds: ScrollBounds,
    duration: Duration,
) -> (ViewPoint, ViewPoint) {
    let distance = add_point(distance, sub_point(current, old_end));
    let end = dolphin_scroll_end_offset(current, distance, bounds);
    let start = dolphin_interrupted_start(current, end, duration);
    (start, end)
}

fn dolphin_scroll_end_offset(
    current: ViewPoint,
    distance: ViewPoint,
    bounds: ScrollBounds,
) -> ViewPoint {
    bounds.clamp(sub_point(current, distance))
}

fn dolphin_interrupted_start(
    current: ViewPoint,
    target: ViewPoint,
    duration: Duration,
) -> ViewPoint {
    let duration_ms = duration.as_secs_f32() * 1000.0;
    let ratio = if duration_ms <= f32::EPSILON {
        1.0
    } else {
        (1000.0 / (duration_ms * 60.0)).clamp(0.0, 1.0)
    };
    clamp_step_toward_target(lerp_point(current, target, ratio), current, target)
}

fn clamp_step_toward_target(step: ViewPoint, current: ViewPoint, target: ViewPoint) -> ViewPoint {
    ViewPoint {
        x: clamp_axis_step(step.x, current.x, target.x),
        y: clamp_axis_step(step.y, current.y, target.y),
    }
}

fn clamp_axis_step(step: f32, current: f32, target: f32) -> f32 {
    if current < target {
        step.min(target)
    } else {
        step.max(target)
    }
}

fn duration_ratio(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
}

fn add_point(left: ViewPoint, right: ViewPoint) -> ViewPoint {
    ViewPoint {
        x: left.x + right.x,
        y: left.y + right.y,
    }
}

fn sub_point(left: ViewPoint, right: ViewPoint) -> ViewPoint {
    ViewPoint {
        x: left.x - right.x,
        y: left.y - right.y,
    }
}

fn lerp_point(start: ViewPoint, target: ViewPoint, ratio: f32) -> ViewPoint {
    ViewPoint {
        x: start.x + (target.x - start.x) * ratio,
        y: start.y + (target.y - start.y) * ratio,
    }
}

fn clamp_velocity(velocity: ViewPoint) -> ViewPoint {
    ViewPoint {
        x: velocity
            .x
            .clamp(-KINETIC_MAX_VELOCITY, KINETIC_MAX_VELOCITY),
        y: velocity
            .y
            .clamp(-KINETIC_MAX_VELOCITY, KINETIC_MAX_VELOCITY),
    }
}

fn velocity_magnitude(velocity: ViewPoint) -> f32 {
    velocity.x.abs().max(velocity.y.abs())
}

fn scroll_bound_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_scroll_eases_to_target_and_finishes() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(500.0, 0.0);
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 200.0, y: 0.0 },
            bounds,
            now,
        );

        let quarter = scroll.offset_at(now + SMOOTH_SCROLL_DURATION / 4);
        assert!(quarter.active);
        assert!(quarter.offset.x > 0.0);
        assert!(quarter.offset.x < 100.0);

        let done = scroll.offset_at(now + SMOOTH_SCROLL_DURATION);
        assert!(!done.active);
        assert_eq!(done.offset, ViewPoint { x: 200.0, y: 0.0 });
    }

    #[test]
    fn smooth_scroll_contents_by_accumulates_from_existing_target() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(500.0, 0.0);
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 100.0, y: 0.0 },
            bounds,
            now,
        );
        let current = scroll.offset_at(now + SMOOTH_SCROLL_DURATION / 4).offset;
        let retargeted = scroll.scroll_contents_by(
            current,
            ViewPoint { x: -50.0, y: 0.0 },
            bounds,
            now + SMOOTH_SCROLL_DURATION / 4,
        );

        assert_eq!(
            retargeted.target_offset(),
            Some(ViewPoint { x: 150.0, y: 0.0 })
        );
        assert!(
            retargeted
                .offset_at(now + SMOOTH_SCROLL_DURATION / 4)
                .offset
                .x
                > current.x
        );
    }

    #[test]
    fn smooth_scroll_contents_by_uses_dolphin_interrupted_start_step() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(500.0, 0.0);
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 100.0, y: 0.0 },
            bounds,
            now,
        );
        let current = scroll.offset_at(now + SMOOTH_SCROLL_DURATION / 4).offset;
        let retargeted = scroll.scroll_contents_by(
            current,
            ViewPoint { x: -50.0, y: 0.0 },
            bounds,
            now + SMOOTH_SCROLL_DURATION / 4,
        );
        let start = retargeted
            .offset_at(now + SMOOTH_SCROLL_DURATION / 4)
            .offset;
        let expected = current.x + (150.0 - current.x) * 1000.0 / (180.0 * 60.0);

        assert!((start.x - expected).abs() < 0.01);
    }

    #[test]
    fn smooth_scroll_contents_by_matches_dolphin_reverse_scroll_formula() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(500.0, 0.0);
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 200.0, y: 0.0 },
            bounds,
            now,
        );
        let retarget_at = now + SMOOTH_SCROLL_DURATION / 3;
        let current = scroll.offset_at(retarget_at).offset;
        let retargeted =
            scroll.scroll_contents_by(current, ViewPoint { x: 190.0, y: 0.0 }, bounds, retarget_at);
        let start = retargeted.offset_at(retarget_at).offset;

        let old_end = 200.0;
        let qt_scroll_contents_by_distance = 190.0 + current.x - old_end;
        let dolphin_end = current.x - qt_scroll_contents_by_distance;
        let expected_start = current.x + (dolphin_end - current.x) * 1000.0 / (180.0 * 60.0);

        let target = retargeted.target_offset().unwrap();
        assert!((target.x - dolphin_end).abs() < 0.01);
        assert_eq!(target.y, 0.0);
        assert!((start.x - expected_start).abs() < 0.01);
        assert!(start.x < current.x);
    }

    #[test]
    fn smooth_scroll_contents_by_clamps_interrupted_start_toward_bounds() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(210.0, 0.0);
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 200.0, y: 0.0 },
            bounds,
            now,
        );
        let retarget_at = now + SMOOTH_SCROLL_DURATION / 2;
        let current = scroll.offset_at(retarget_at).offset;
        let retargeted =
            scroll.scroll_contents_by(current, ViewPoint { x: -80.0, y: 0.0 }, bounds, retarget_at);
        let start = retargeted.offset_at(retarget_at).offset;

        assert_eq!(
            retargeted.target_offset(),
            Some(ViewPoint { x: 210.0, y: 0.0 })
        );
        assert!(start.x >= current.x);
        assert!(start.x <= 210.0);
    }

    #[test]
    fn smooth_scroll_contents_by_from_kinetic_starts_at_current_offset() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(500.0, 0.0);
        let scroll = SmoothScroll::kinetic(ViewPoint { x: 2000.0, y: 0.0 }, bounds, now).unwrap();
        let current = ViewPoint { x: 40.0, y: 0.0 };
        let retargeted = scroll.scroll_contents_by(
            current,
            ViewPoint { x: -80.0, y: 0.0 },
            bounds,
            now + SMOOTH_SCROLL_FRAME,
        );

        assert_eq!(
            retargeted.offset_at(now + SMOOTH_SCROLL_FRAME).offset,
            current
        );
        assert_eq!(
            retargeted.target_offset(),
            Some(ViewPoint { x: 120.0, y: 0.0 })
        );
    }

    #[test]
    fn kinetic_scroll_decays_and_clamps_at_bounds() {
        let now = Instant::now();
        let bounds = ScrollBounds::new(100.0, 0.0);
        let mut scroll =
            SmoothScroll::kinetic(ViewPoint { x: 2000.0, y: 0.0 }, bounds, now).unwrap();

        let first = scroll.advance(ViewPoint { x: 90.0, y: 0.0 }, now + SMOOTH_SCROLL_FRAME);
        assert_eq!(first.offset.x, 100.0);

        let second = scroll.advance(first.offset, now + SMOOTH_SCROLL_FRAME * 2);
        assert_eq!(second.offset.x, 100.0);
        assert!(!second.active);
    }

    #[test]
    fn scroll_drag_tracker_reports_last_velocity() {
        let now = Instant::now();
        let mut tracker = ScrollDragTracker::default();
        tracker.sample(ViewPoint { x: 10.0, y: 0.0 }, now);
        tracker.sample(
            ViewPoint { x: 58.0, y: 0.0 },
            now + Duration::from_millis(16),
        );

        assert!((tracker.velocity().x - 3000.0).abs() < 0.5);
    }

    #[test]
    fn smooth_scroll_maximum_match_uses_pixel_tolerance() {
        let now = Instant::now();
        let scroll = SmoothScroll::to_target(
            ViewPoint { x: 0.0, y: 0.0 },
            ViewPoint { x: 100.0, y: 0.0 },
            ScrollBounds::new(500.0, 0.0),
            now,
        );

        assert!(scroll.maximum_matches(ScrollBounds::new(500.4, 0.0)));
        assert!(!scroll.maximum_matches(ScrollBounds::new(500.6, 0.0)));
    }
}
