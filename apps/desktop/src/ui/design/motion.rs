//! The motion catalog, and the shared clock that drives everything animated.
//!
//! Ported from Zeron `crates/ui/src/motion.rs` (MIT, Copyright (c) 2026 Wing).
//! See `docs/photo-gallery/zeron-reference.md` Part 3.
//!
//! [`FrameClock`] exists because gpui's `with_animation(..repeat())` re-renders on
//! every display frame for as long as the element is mounted; Zeron measured one
//! spinner pinning a window at 120Hz at 36% CPU. Animated paint here renews a
//! lease against its view, and when no lease renews the clock parks and the window
//! schedules nothing. [`hover`](super::hover) rides the same clock.
//!
//! One-shot animations use gpui's `with_animation` directly; the element helpers
//! below are that path.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use gpui_kit::base::Easing;
use gpui_kit::{
    Animation, AnimationElement, AnimationExt as _, App, ElementId, EntityId, Global, IntoElement,
    Styled, px,
};

/// Repeat-tick interval for the shared clock, about 30fps.
const TICK: Duration = Duration::from_millis(33);

/// How long a view stays on the tick list after its last animated paint.
///
/// A lease outlives a few missed frames, so a dropped frame does not stop an
/// animation mid-flight. One that is never renewed is dropped too, which is what
/// lets the clock park.
const LEASE: Duration = Duration::from_millis(300);

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function.
///
/// Stored as four plain values so the catalog stays `const`, and evaluated by a
/// local solver so per-cell phase work never allocates.
/// [`Curve::easing`] converts to the library's own easing policy for the
/// one-shot path, where no hot loop exists; the tests pin the two against each
/// other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl Curve {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// One axis of the curve, as polynomial coefficients.
    fn coefficients(a: f32, b: f32) -> (f32, f32, f32) {
        let c = 3.0 * a;
        let b_axis = 3.0 * (b - a) - c;
        let a_axis = 1.0 - c - b_axis;
        (a_axis, b_axis, c)
    }

    fn sample_x(self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        ((a * t + b) * t + c) * t
    }

    fn sample_y(self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.y1, self.y2);
        ((a * t + b) * t + c) * t
    }

    fn sample_x_derivative(self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        (3.0 * a * t + 2.0 * b) * t + c
    }

    /// Curve parameter `t` for a given progress `x`, both in `0..=1`.
    ///
    /// Newton-Raphson with a bisection fallback. `x` is monotonic over a valid
    /// CSS bezier, so bisection always terminates.
    fn solve_t_for_x(self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..8 {
            let error = self.sample_x(t) - x;
            if error.abs() < 1e-6 {
                return t;
            }
            let derivative = self.sample_x_derivative(t);
            if derivative.abs() < 1e-6 {
                break;
            }
            t -= error / derivative;
        }
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..32 {
            let mid = (lo + hi) / 2.0;
            if self.sample_x(mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    }

    /// Eased output for input progress `x`, clamped hard to `0..=1`.
    ///
    /// f32 rounding can leave `sample_y` a hair above 1.0 near the end of a
    /// curve, and every caller assumes a unit interval.
    pub fn eval(self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        self.sample_y(self.solve_t_for_x(x)).clamp(0.0, 1.0)
    }

    /// This curve as the library's easing policy. Allocation is fine here:
    /// one-shot element helpers build this once per mount, not per frame.
    pub fn easing(self) -> Easing {
        Easing::cubic_bezier(self.x1, self.y1, self.x2, self.y2)
            .expect("catalog curves are valid cubic beziers")
    }
}

// The catalog names match the CSS shorthand each one is named after, so a
// reader can look them up without this module.
/// CSS `ease-out-expo`. The entrance curve of this design.
pub const EASE_OUT_EXPO: Curve = Curve::new(0.16, 1.0, 0.3, 1.0);
/// CSS `ease-out`.
pub const EASE_OUT: Curve = Curve::new(0.0, 0.0, 0.58, 1.0);
/// CSS `ease`.
pub const EASE: Curve = Curve::new(0.25, 0.1, 0.25, 1.0);
/// CSS `ease-out-quint`.
pub const EASE_OUT_QUINT: Curve = Curve::new(0.22, 1.0, 0.36, 1.0);
/// CSS `ease-in-out`.
pub const EASE_IN_OUT: Curve = Curve::new(0.42, 0.0, 0.58, 1.0);
/// Tailwind's default `transition-colors` curve — the shape every hover wash in
/// the reference rides.
pub const EASE_TAILWIND: Curve = Curve::new(0.4, 0.0, 0.2, 1.0);

/// One catalog entry: a duration, an optional delay, a curve.
///
/// The delay folds into the timeline because gpui's `Animation` has no native
/// delay, and the element helpers would otherwise need a second wrapper for the
/// hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    tween_ms: u64,
    delay_ms: u64,
    curve: Curve,
}

impl MotionSpec {
    pub const fn new(tween_ms: u64, curve: Curve) -> Self {
        Self {
            tween_ms,
            delay_ms: 0,
            curve,
        }
    }

    pub const fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Wall-clock span of the whole timeline, delay included.
    pub fn total(self) -> Duration {
        Duration::from_millis(self.delay_ms + self.tween_ms)
    }

    /// Eased progress for a raw timeline delta in `0..=1` across [`total`][1].
    ///
    /// Pure, so sampling on the shared clock needs no window.
    ///
    /// [1]: MotionSpec::total
    pub fn progress(self, raw_delta: f32) -> f32 {
        let total = (self.delay_ms + self.tween_ms) as f32;
        if total <= 0.0 || self.tween_ms == 0 {
            return 1.0;
        }
        let t = (raw_delta.clamp(0.0, 1.0) * total - self.delay_ms as f32) / self.tween_ms as f32;
        self.curve.eval(t.clamp(0.0, 1.0))
    }

    /// A one-shot gpui [`Animation`] for this spec, delay folded in.
    pub fn animation(self) -> Animation {
        let spec = self;
        Animation::new(spec.total()).with_easing(move |delta| spec.progress(delta))
    }
}

// Durations are tuned as a set, not individually.

/// Entrances: 0.5s, a 4px rise, and the tone of the app.
pub const FADE_IN: MotionSpec = MotionSpec::new(500, EASE_OUT_EXPO);
/// Quick fades.
pub const FADE_QUICK: MotionSpec = MotionSpec::new(150, EASE);
/// Hover washes: Tailwind's own default, which is what the reference's design
/// system uses for `transition-colors`.
pub const HOVER_FADE: MotionSpec = MotionSpec::new(150, EASE_TAILWIND);
/// Menus opening.
pub const MENU_IN: MotionSpec = MotionSpec::new(140, EASE);
/// Menus closing. Always shorter than the opening: getting out of the way is
/// what closing means.
pub const MENU_OUT: MotionSpec = MotionSpec::new(100, EASE);
/// Overlays and dialogs.
pub const DIALOG_IN: MotionSpec = MotionSpec::new(180, EASE);

/// Linear interpolation between two numbers, `t` in `0..=1`.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Standard entrance: opacity 0→1 and a 4px rise over [`FADE_IN`].
///
/// The translation is a relative `top` inset rather than a real transform,
/// because this GPUI revision has no scale transform for `div`s. Taffy applies
/// relative insets after layout, so siblings never move — the same behaviour a
/// CSS transform gives, and layout-independent by construction.
pub fn fade_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, FADE_IN.animation(), |element, t| {
        element.relative().opacity(t).top(px(4.0 * (1.0 - t)))
    })
}

/// Quick opacity-only fade over [`FADE_QUICK`].
pub fn fade_quick<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, FADE_QUICK.animation(), |element, t| element.opacity(t))
}

/// Menu entrance: fade plus a 2px drift away from `from`, the signed starting
/// offset. Negative for a dropdown, positive for a menu that opens upward.
pub fn menu_in<E>(id: impl Into<ElementId>, from: f32, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, MENU_IN.animation(), move |element, t| {
        element
            .relative()
            .opacity(0.3 + 0.7 * t)
            .top(px(from * (1.0 - t)))
    })
}

/// Overlay or dialog entrance over [`DIALOG_IN`].
pub fn dialog_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, DIALOG_IN.animation(), |element, t| {
        element.relative().opacity(t).top(px(2.0 * (1.0 - t)))
    })
}

/// A scrim entrance: opacity only, because a backdrop that slides reads as a
/// window sliding rather than a layer arriving.
pub fn scrim_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, DIALOG_IN.animation(), |element, t| element.opacity(t))
}

/// One view's standing order for ticks.
struct Lease {
    /// How long the view may keep drawing without renewing.
    until: Instant,
    stride: u64,
}

impl Lease {
    fn renew(&mut self, now: Instant, stride: u64) {
        self.until = now + LEASE;
        self.stride = self.stride.min(stride);
    }

    /// Whether this tick is one the view should redraw on.
    ///
    /// A tick that is taken resets the stride, so a view that stops painting
    /// stops being notified even though its lease is still fresh. Each paint
    /// renews, which re-establishes the stride — and that is what keeps a
    /// retired fast animation from dragging a slow one up to its own rate.
    fn take_tick(&mut self, tick: u64) -> bool {
        if !tick.is_multiple_of(self.stride) {
            return false;
        }
        self.stride = u64::MAX;
        true
    }
}

/// The process-wide 33ms clock.
#[derive(Default)]
struct FrameClock {
    tick: u64,
    leases: HashMap<EntityId, Lease>,
    running: bool,
}

impl Global for FrameClock {}

/// Keep a view on the shared clock's tick list.
///
/// Call this from the paint that needs frames, and only there. The clock starts
/// its ticker on the first lease and parks once the last expires.
pub fn lease(view: EntityId, cx: &mut App) {
    lease_every(view, 1, cx);
}

/// Renew a view only when it still needs frames.
///
/// A hover tween calls this while its blend is mid-flight and stops when it
/// settles, so the clock does not keep a window awake for a fourth frame of
/// nothing.
pub fn lease_while(view: EntityId, active: bool, cx: &mut App) {
    if active {
        lease(view, cx);
    }
}

fn lease_every(view: EntityId, stride: u64, cx: &mut App) {
    if cx.reduce_motion() {
        return;
    }
    let now = Instant::now();
    let clock = cx.default_global::<FrameClock>();
    if let Some(lease) = clock.leases.get_mut(&view) {
        lease.renew(now, stride);
    } else {
        clock.leases.insert(
            view,
            Lease {
                until: now + LEASE,
                stride,
            },
        );
    }
    if clock.running {
        return;
    }
    clock.running = true;
    cx.spawn(async move |app| {
        loop {
            app.background_executor().timer(TICK).await;
            let parked = app.update(|app| {
                let clock = app.default_global::<FrameClock>();
                let now = Instant::now();
                clock.leases.retain(|_, lease| lease.until > now);
                if clock.leases.is_empty() {
                    clock.running = false;
                    return true;
                }
                clock.tick = clock.tick.wrapping_add(1);
                let tick = clock.tick;
                let due = clock
                    .leases
                    .iter_mut()
                    .filter_map(|(view, lease)| lease.take_tick(tick).then_some(*view))
                    .collect::<Vec<_>>();
                for view in due {
                    app.notify(view);
                }
                false
            });
            if parked {
                break;
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_curves_evaluate_their_endpoints() {
        for curve in [
            EASE_OUT_EXPO,
            EASE_OUT,
            EASE,
            EASE_OUT_QUINT,
            EASE_IN_OUT,
            EASE_TAILWIND,
        ] {
            assert_eq!(curve.eval(f32::NEG_INFINITY), 0.0, "{curve:?}");
            assert_eq!(curve.eval(0.0), 0.0, "{curve:?}");
            assert_eq!(curve.eval(1.0), 1.0, "{curve:?}");
            assert_eq!(curve.eval(2.0), 1.0, "{curve:?}");
        }
    }

    #[test]
    fn the_local_solver_and_the_library_easing_agree() {
        // Both handlers solve the same bezier, so they must agree within the
        // f32 noise the library's own solver leaves.
        for curve in [EASE_OUT_EXPO, EASE_OUT_QUINT, EASE_TAILWIND] {
            let library = curve.easing();
            for step in 0..=10 {
                #[allow(clippy::cast_precision_loss)]
                let x = step as f32 / 10.0;
                let difference = curve.eval(x) - library.sample(x);
                assert!(difference.abs() < 1e-4, "{curve:?} at {x}: {difference}");
            }
        }
    }

    #[test]
    fn stay_in_unit_interval_everywhere() {
        for curve in [EASE_OUT_EXPO, EASE_TAILWIND] {
            let steps = 200;
            #[allow(clippy::cast_precision_loss)]
            for step in 0..=steps {
                let x = step as f32 / steps as f32;
                let y = curve.eval(x);
                assert!((0.0..=1.0).contains(&y), "{curve:?} at {x} gave {y}");
            }
        }
    }

    #[test]
    fn progress_respects_a_delay() {
        let held = MotionSpec::new(200, EASE_OUT).with_delay(150);
        assert_eq!(held.progress(0.0), 0.0);
        // 25% of the timeline is inside the hold, not inside the tween.
        assert_eq!(held.progress(0.25), 0.0);
        // 50% is 25ms into a 200ms tween: barely started.
        assert!(held.progress(0.5) < 0.25, "the hold moved the timeline");
        // Only the end of the timeline is the end of the tween.
        assert_eq!(held.progress(1.0), 1.0);
        assert_eq!(held.total(), Duration::from_millis(350));
    }

    #[test]
    fn a_zero_duration_spec_reports_settled() {
        let spec = MotionSpec::new(0, EASE_OUT);
        assert_eq!(spec.progress(0.7), 1.0);
    }

    #[test]
    fn a_renewal_sets_the_stride_to_the_fastest_animation_asked_for() {
        let now = Instant::now();
        let mut lease = Lease {
            until: now + LEASE,
            stride: 2,
        };
        lease.renew(now, 1);
        assert_eq!(lease.stride, 1);

        // Taking a tick reserves the lease: only a further renewal, which means
        // a further paint, keeps this view on the tick list.
        assert!(lease.take_tick(4));
        assert!(!lease.take_tick(5));
        lease.renew(now, 2);
        assert!(lease.take_tick(6));
    }
}
