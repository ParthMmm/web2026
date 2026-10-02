//! Hover fades, because gpui hover styles snap.
//!
//! Ported from Zeron `crates/ui/src/motion.rs` (the `HoverFades` section, MIT,
//! Copyright (c) 2026 Wing). See `docs/photo-gallery/zeron-reference.md` §3.3.
//!
//! gpui's own `with_animation` cannot do this: its clock is keyed by element id and
//! replays from zero on remount, so a hover *leave* would animate from zero instead
//! of from the value the element reached. This keeps per-key progress advanced from
//! wall time, in a `thread_local` so element builders can read it without `cx`.
//! Frames come from the shared clock, leased only while a blend is mid-flight.

use std::{cell::RefCell, collections::HashMap, time::Instant};

use gpui_kit::{App, EntityId};

use super::motion::{HOVER_FADE, lease_while, lerp};

/// One element's hover blend. Progress runs from `origin` toward `target` over
/// the catalog's hover duration, and re-anchors at the current value whenever
/// the pointer flips direction mid-flight so the blend stays continuous.
#[derive(Debug, Clone, Copy)]
struct Fade {
    origin: f32,
    target: f32,
    started: Instant,
    /// Frame counter at the last read. Liveness stamp: a key that goes a whole
    /// frame without a read belongs to an unmounted element whose leave event
    /// can never arrive.
    seen: u64,
}

impl Fade {
    fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= HOVER_FADE.total() {
            return self.target;
        }
        let raw = elapsed.as_secs_f32() / HOVER_FADE.total().as_secs_f32();
        lerp(self.origin, self.target, HOVER_FADE.progress(raw))
    }

    fn settled(&self, now: Instant) -> bool {
        self.origin == self.target
            || now.saturating_duration_since(self.started) >= HOVER_FADE.total()
    }
}

/// Per-key hover progress. The pure core takes an explicit `now` for tests; the
/// thread-local wrappers feed it wall time.
#[derive(Default)]
pub(crate) struct HoverFades {
    entries: HashMap<String, Fade>,
    frame: u64,
}

impl HoverFades {
    /// Record a pointer entering (`hovered`) or leaving the element behind
    /// `key`. Reduced motion snaps straight to the endpoint.
    pub fn set(&mut self, key: &str, hovered: bool, reduced: bool, now: Instant) {
        let target = if hovered { 1.0 } else { 0.0 };
        if target == 0.0 && !self.entries.contains_key(key) {
            // A never-hovered element reporting a leave has nothing to do.
            return;
        }
        let origin = if reduced {
            target
        } else {
            self.entries.get(key).map_or(0.0, |fade| fade.value(now))
        };
        self.stamp(key);
        self.entries.insert(
            key.to_owned(),
            Fade {
                origin,
                target,
                seen: self.frame,
                started: now,
            },
        );
    }

    /// Progress for `key` at `now`, stamping liveness.
    pub fn value(&mut self, key: &str, now: Instant) -> f32 {
        self.stamp(key);
        self.entries.get(key).map_or(0.0, |fade| fade.value(now))
    }

    fn stamp(&mut self, key: &str) {
        if let Some(fade) = self.entries.get_mut(key) {
            fade.seen = self.frame;
        }
    }

    /// Once-per-frame bookkeeping: advance the frame counter, drop entries that
    /// settled at rest, and drop any that went a whole frame unread — an element
    /// that unmounted mid-hover never gets its leave event. Returns whether any
    /// blend is still mid-flight.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.frame += 1;
        let frame = self.frame;
        let mut active = false;
        self.entries.retain(|_, fade| {
            if fade.seen + 1 < frame {
                return false;
            }
            let settled = fade.settled(now);
            active |= !settled;
            !(settled && fade.target == 0.0)
        });
        active
    }
}

thread_local! {
    static HOVER_FADES: RefCell<HoverFades> = RefCell::new(HoverFades::default());
}

/// Record a pointer flip for `key`. Returns whether the state actually changed,
/// so a caller can skip a redraw for a no-op event.
pub fn set_hover(key: &str, hovered: bool, reduced: bool) -> bool {
    HOVER_FADES.with(|fades| {
        let mut fades = fades.borrow_mut();
        let changed = fades.entries.get(key).map_or(hovered, |fade| {
            fade.target != if hovered { 1.0 } else { 0.0 }
        });
        fades.set(key, hovered, reduced, Instant::now());
        changed
    })
}

/// Eased hover progress for `key`, in `0..=1`.
pub fn hover_t(key: &str) -> f32 {
    HOVER_FADES.with(|fades| fades.borrow_mut().value(key, Instant::now()))
}

/// Bookkeeping for one frame, plus the frame request.
///
/// Call this once per render, with the view that paints the hovered elements.
/// It drops entries for unmounted elements and, while a blend is in flight, asks
/// the shared clock to keep the view on its tick list.
pub fn maintain(view: EntityId, cx: &mut App) {
    HOVER_FADES.with(|fades| {
        let active = fades.borrow_mut().tick(Instant::now());
        lease_while(view, active, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> HoverFades {
        HoverFades::default()
    }

    #[test]
    fn a_never_hovered_leave_never_creates_an_entry() {
        let mut store = fresh();
        store.set("tile", false, false, Instant::now());
        assert!(store.entries.is_empty());
    }

    #[test]
    fn progress_advances_from_the_current_value_on_a_flip_mid_flight() {
        let start = Instant::now();
        let mut store = fresh();
        store.set("tile", true, false, start);
        let mid = start + HOVER_FADE.total() / 2;
        let reached = store.value("tile", mid);
        assert!(reached > 0.0, "a half-finished hover has begun");

        store.set("tile", false, false, mid);
        let after_flip = store.entries.get("tile").expect("still fading").origin;
        assert!(
            (after_flip - reached).abs() < 1e-4,
            "a leave mid-flight blends from where it was: {after_flip} vs {reached}"
        );
    }

    #[test]
    fn reduced_motion_snaps_to_the_endpoint() {
        let mut store = fresh();
        store.set("tile", true, true, Instant::now());
        let entry = store.entries.get("tile").expect("recorded");
        assert_eq!(entry.origin, 1.0);
        assert_eq!(entry.target, 1.0);
        assert_eq!(store.value("tile", Instant::now()), 1.0);
    }

    #[test]
    fn an_unread_entry_is_pruned_after_a_full_frame() {
        let start = Instant::now();
        let mut store = fresh();
        store.set("tile", true, false, start);
        store.value("tile", start);
        assert!(store.tick(start), "a live blend wants more frames");

        // One frame later with no read: the element unmounted, and its leave
        // event can never arrive.
        assert!(!store.tick(start + HOVER_FADE.total()));
        assert!(store.entries.is_empty());
    }

    #[test]
    fn a_settled_hover_entry_is_kept_so_a_removal_can_blend_back_out() {
        let start = Instant::now();
        let mut store = fresh();
        store.set("tile", true, false, start);
        let settled = start + HOVER_FADE.total();
        assert_eq!(store.value("tile", settled), 1.0);
        store.tick(settled);
        // A leave now records a live blend, so the store must still be there.
        store.set("tile", false, false, settled);
        let live = store.entries.get("tile").expect("kept for the leave");
        assert_eq!(live.origin, 1.0);
        assert_eq!(live.target, 0.0);
    }

    #[test]
    fn climbing_and_falling_use_the_same_curve() {
        let start = Instant::now();
        let mut store = fresh();
        store.set("tile", true, false, start);
        let rise = store.value("tile", start + HOVER_FADE.total() / 4);
        assert!(rise > 0.0 && rise < 1.0);

        let flip = start + HOVER_FADE.total() / 2;
        store.set("tile", false, false, flip);
        let fall = store.value("tile", flip + HOVER_FADE.total() / 4);
        assert!(
            fall > 0.0 && fall < 1.0,
            "a falling blend eases out, it does not snap"
        );
    }
}
