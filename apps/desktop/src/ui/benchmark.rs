//! Scripted browsing for `benchmark.py`: fixed-duration scrolling, periodic
//! preview requests, and a JSON report on stdout. The report keys are the
//! benchmark's contract.

use super::cache::BoundedCache;
use gpui_kit::gpui::{FrameEvent, FrameTimingCollector, WindowId, profiler};
use gpui_kit::{App, Entity, Window, WindowKind, WindowOptions};
use std::time::{Duration, Instant};

/// Benchmark rows assume this many columns regardless of window width, so
/// preview indices match `benchmark.py`'s warm preparation.
pub(super) const COLUMNS: usize = 4;
const ROWS_PER_SECOND: f64 = 4.0;
const PREVIEW_INTERVAL_SECONDS: u64 = 3;
const PREVIEW_TAIL_SECONDS: u64 = 2;

pub(super) fn seconds_from_env() -> Option<u64> {
    match std::env::var("PHOTO_BENCH_SECONDS") {
        Ok(value) => {
            let seconds = value
                .parse::<u64>()
                .expect("PHOTO_BENCH_SECONDS must be a positive integer");
            assert!(
                seconds > 0,
                "PHOTO_BENCH_SECONDS must be a positive integer"
            );
            Some(seconds)
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("PHOTO_BENCH_SECONDS must be valid UTF-8")
        }
    }
}

pub(super) fn configure_window(window_options: &mut WindowOptions) {
    // GPUI 0.3.4's macOS frame source follows NSWindowOcclusionState. A
    // covered normal window stops receiving CVDisplayLink ticks even when it
    // remains dirty. PopUp uses a non-activating panel at the popup level, so
    // the benchmark stays visible to the compositor without taking focus.
    window_options.focus = false;
    window_options.inactive_frame_interval = None;
    window_options.kind = WindowKind::PopUp;
}

/// What the script should do on this frame.
pub(super) struct Step {
    pub(super) scroll_row: usize,
    pub(super) preview_index: Option<usize>,
}

pub(super) struct Benchmark {
    seconds: u64,
    present_trace: PresentTrace,
    started: Instant,
    next_preview_at: u64,
    preview_requested: Option<Instant>,
    preview_started: Option<Instant>,
    preview_attempts: usize,
    preview_failures: usize,
    preview_cache_hits: usize,
    preview_decoded_ms: Vec<f64>,
    preview_latencies_ms: Vec<f64>,
    preview_samples: Vec<(usize, f64)>,
    max_render_gap: Duration,
    max_render_gap_before_frame: usize,
    render_count: usize,
    first_frame_ms: Option<f64>,
    inactive_frames: usize,
    last_render: Instant,
}

struct PresentTrace {
    collector: FrameTimingCollector,
    changed_trace_setting: bool,
}

impl PresentTrace {
    fn new() -> Self {
        let changed_trace_setting = profiler::set_trace_enabled(true);
        Self {
            collector: FrameTimingCollector::new(),
            changed_trace_setting,
        }
    }

    fn finish(&mut self, window_id: WindowId, started: Instant, ended: Instant) -> PresentSamples {
        present_samples(&self.collector.collect_unseen(), window_id, started, ended)
    }
}

impl Drop for PresentTrace {
    fn drop(&mut self) {
        if self.changed_trace_setting {
            profiler::set_trace_enabled(false);
        }
    }
}

struct PresentSamples {
    draw_events: u64,
    presented_frames: u64,
    superseded_draw_events: u64,
    pending_draw_events: u64,
    intervals_ms: Vec<f64>,
    durations_ms: Vec<f64>,
    ordered: bool,
    first_present_ms: Option<f64>,
    last_present_age_ms: Option<f64>,
    max_gap_ms: Option<f64>,
}

impl PresentSamples {
    fn complete(&self, expected_draws: u64) -> bool {
        self.ordered
            && self.presented_frames > 0
            && self.draw_events == expected_draws
            && self.pending_draw_events == 0
            && self.draw_events == self.presented_frames + self.superseded_draw_events
            && u64::try_from(self.intervals_ms.len()).ok() == Some(self.presented_frames - 1)
    }
}

fn present_samples(
    events: &[FrameEvent],
    window_id: WindowId,
    started: Instant,
    ended: Instant,
) -> PresentSamples {
    let mut samples = PresentSamples {
        draw_events: 0,
        presented_frames: 0,
        superseded_draw_events: 0,
        pending_draw_events: 0,
        intervals_ms: Vec::new(),
        durations_ms: Vec::new(),
        ordered: ended >= started,
        first_present_ms: None,
        last_present_age_ms: None,
        max_gap_ms: None,
    };
    let mut previous = None;
    for event in events {
        match event {
            FrameEvent::Draw(frame) if frame.window_id == window_id => {
                samples.draw_events += 1;
                samples.pending_draw_events += 1;
            }
            FrameEvent::Present(frame) if frame.window_id == window_id => {
                samples.presented_frames += 1;
                if samples.pending_draw_events == 0 {
                    samples.ordered = false;
                } else {
                    samples.superseded_draw_events += samples.pending_draw_events - 1;
                    samples.pending_draw_events = 0;
                }
                samples.ordered &= frame.present_end >= started && frame.present_end <= ended;
                if let Some(duration) = frame
                    .present_end
                    .checked_duration_since(frame.present_start)
                {
                    samples.durations_ms.push(millis(duration));
                } else {
                    samples.ordered = false;
                }
                if let Some(previous) = previous {
                    if frame.present_end > previous {
                        samples
                            .intervals_ms
                            .push(millis(frame.present_end.duration_since(previous)));
                    } else {
                        samples.ordered = false;
                    }
                } else {
                    samples.first_present_ms = frame
                        .present_end
                        .checked_duration_since(started)
                        .map(millis);
                }
                previous = Some(frame.present_end);
            }
            _ => {}
        }
    }
    samples.last_present_age_ms = previous
        .and_then(|last| ended.checked_duration_since(last))
        .map(millis);
    samples.max_gap_ms = samples
        .first_present_ms
        .into_iter()
        .chain(samples.last_present_age_ms)
        .chain(samples.intervals_ms.iter().copied())
        .max_by(f64::total_cmp);
    samples
}

pub(super) struct Totals {
    pub(super) photos: usize,
    pub(super) thumbnails_completed: usize,
    pub(super) failed: usize,
}

impl Benchmark {
    pub(super) fn new(seconds: u64) -> Self {
        let now = Instant::now();
        Self {
            seconds,
            present_trace: PresentTrace::new(),
            started: now,
            next_preview_at: 1,
            preview_requested: None,
            preview_started: None,
            preview_attempts: 0,
            preview_failures: 0,
            preview_cache_hits: 0,
            preview_decoded_ms: Vec::new(),
            preview_latencies_ms: Vec::new(),
            preview_samples: Vec::new(),
            max_render_gap: Duration::ZERO,
            max_render_gap_before_frame: 0,
            render_count: 0,
            first_frame_ms: None,
            inactive_frames: 0,
            last_render: now,
        }
    }

    pub(super) fn duration(&self) -> Duration {
        Duration::from_secs(self.seconds)
    }

    pub(super) fn record_frame(&mut self, window: &Window) {
        self.render_count += 1;
        let since_last = self.last_render.elapsed();
        if self.render_count == 1 {
            self.first_frame_ms = Some(millis(self.started.elapsed()));
        } else if since_last > self.max_render_gap {
            self.max_render_gap = since_last;
            self.max_render_gap_before_frame = self.render_count;
        }
        self.last_render = Instant::now();
        if self.started.elapsed().as_secs() > 1 && !window.is_window_active() {
            self.inactive_frames += 1;
        }
    }

    /// Advances the script. `None` once the run is over or nothing is loaded.
    pub(super) fn step(&mut self, photos: usize) -> Option<Step> {
        let elapsed = self.started.elapsed();
        if elapsed.as_secs() >= self.seconds || photos == 0 {
            return None;
        }
        let rows = photos.div_ceil(COLUMNS);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scroll_row = (elapsed.as_secs_f64() * ROWS_PER_SECOND) as usize % rows;
        let mut preview_index = None;
        if elapsed.as_secs() >= self.next_preview_at
            && self.next_preview_at + PREVIEW_TAIL_SECONDS <= self.seconds
        {
            #[allow(clippy::cast_possible_truncation)]
            let preview_row = (self.next_preview_at as usize * ROWS_PER_SECOND as usize) % rows;
            preview_index = Some((preview_row * COLUMNS).min(photos - 1));
            self.next_preview_at += PREVIEW_INTERVAL_SECONDS;
        }
        Some(Step {
            scroll_row,
            preview_index,
        })
    }

    pub(super) fn preview_requested(&mut self) {
        let now = Instant::now();
        self.preview_attempts += 1;
        self.preview_requested = Some(now);
        self.preview_started = Some(now);
    }

    pub(super) fn preview_ready(&mut self, index: usize, cache_hit: bool) {
        self.preview_cache_hits += usize::from(cache_hit);
        if let Some(start) = self.preview_requested.take() {
            let latency = millis(start.elapsed());
            self.preview_latencies_ms.push(latency);
            self.preview_samples.push((index, latency));
        }
    }

    pub(super) fn preview_failed(&mut self) {
        self.preview_failures += 1;
        self.preview_requested = None;
        self.preview_started = None;
    }

    /// True while a ready preview has not yet finished decoding.
    pub(super) fn awaiting_decode(&self) -> bool {
        self.preview_started.is_some()
    }

    pub(super) fn preview_decoded(&mut self, succeeded: bool) {
        if let Some(start) = self.preview_started.take()
            && succeeded
        {
            self.preview_decoded_ms.push(millis(start.elapsed()));
        }
    }

    pub(super) fn report(
        mut self,
        totals: &Totals,
        thumbnails: &Entity<BoundedCache>,
        previews: &Entity<BoundedCache>,
        window: &Window,
        cx: &App,
    ) -> serde_json::Value {
        let ended = Instant::now();
        let frames = window.frame_duration_snapshot();
        let presents =
            self.present_trace
                .finish(window.window_handle().window_id(), self.started, ended);
        let thumbnails = thumbnails.read(cx);
        let previews = previews.read(cx);
        #[allow(clippy::cast_precision_loss)]
        let histogram_ms = |nanos: u64| nanos as f64 / 1e6;
        let mut report = serde_json::json!({
            "photos": totals.photos,
            "thumbnails_completed": totals.thumbnails_completed,
            "failed": totals.failed,
            "seconds": self.started.elapsed().as_secs_f64(),
            "draw_samples": frames.draw_duration_histogram.len(),
            "last_render_age_ms": millis(self.last_render.elapsed()),
            "draw_p95_ms": histogram_ms(frames.draw_duration_histogram.value_at_quantile(0.95)),
            "present_interval_p95_ms": quantile(&presents.intervals_ms, 0.95),
            "present_interval_p99_ms": quantile(&presents.intervals_ms, 0.99),
            "max_render_gap_ms": millis(self.max_render_gap.max(self.last_render.elapsed())),
            "max_render_gap_before_frame": self.max_render_gap_before_frame,
            "render_count": self.render_count,
            "first_frame_ms": self.first_frame_ms,
            "inactive_frames": self.inactive_frames,
            "preview_attempts": self.preview_attempts,
            "preview_failures": self.preview_failures,
            "preview_cache_hits": self.preview_cache_hits,
            "preview_decoded_samples": self.preview_decoded_ms.len(),
            "preview_decoded_p95_ms": p95(&self.preview_decoded_ms),
            "decode_failures": thumbnails.failures + previews.failures,
            "preview_ready_samples": self.preview_latencies_ms.len(),
            "preview_samples": self.preview_samples,
            "preview_ready_p95_ms": p95(&self.preview_latencies_ms),
            "thumbnail_cache_limit": thumbnails.capacity(),
            "large_cache_limit": previews.capacity(),
            "thumbnail_cache_entries": thumbnails.entries(cx),
            "preview_cache_entries": previews.entries(cx),
            "thumbnail_cache_peak_entries": thumbnails.peak_entries,
            "preview_cache_peak_entries": previews.peak_entries,
            "thumbnail_peak_in_flight": thumbnails.peak_loading,
            "preview_peak_in_flight": previews.peak_loading
        });
        report["present_interval_samples"] = serde_json::json!(presents.intervals_ms.len());
        report["presented_frames"] = serde_json::json!(presents.presented_frames);
        report["trace_draw_events"] = serde_json::json!(presents.draw_events);
        report["superseded_draw_events"] = serde_json::json!(presents.superseded_draw_events);
        report["pending_draw_events"] = serde_json::json!(presents.pending_draw_events);
        report["frame_timing_complete"] =
            serde_json::json!(presents.complete(frames.draw_duration_histogram.len()));
        report["first_present_ms"] = serde_json::json!(presents.first_present_ms);
        report["last_present_age_ms"] = serde_json::json!(presents.last_present_age_ms);
        report["max_present_gap_ms"] = serde_json::json!(presents.max_gap_ms);
        report["present_duration_p95_ms"] = serde_json::json!(p95(&presents.durations_ms));
        report["max_present_duration_ms"] =
            serde_json::json!(presents.durations_ms.iter().copied().max_by(f64::total_cmp));
        report["dirty_to_present_p95_ms"] = serde_json::json!(histogram_ms(
            frames.dirty_to_present_histogram.value_at_quantile(0.95)
        ));
        report["timing_definition"] = serde_json::json!(
            "v1: target-window newly drawn frame submission end intervals; includes inactive frames and all gaps; quantile rank ceil((n-1)*q); excludes physical display scanout"
        );
        report
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn p95(samples: &[f64]) -> Option<f64> {
    quantile(samples, 0.95)
}

fn quantile(samples: &[f64], q: f64) -> Option<f64> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let index = (sorted.len().saturating_sub(1) as f64 * q).ceil() as usize;
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(window_id: WindowId, at: Instant) -> FrameEvent {
        FrameEvent::Draw(gpui_kit::gpui::FrameTiming {
            window_id,
            dirty_at: None,
            invalidations: 1,
            draw_start: at,
            draw_end: at,
        })
    }

    fn present(window_id: WindowId, at: Instant) -> FrameEvent {
        FrameEvent::Present(gpui_kit::gpui::PresentTiming {
            window_id,
            present_start: at,
            present_end: at,
            animation_interval: None,
        })
    }

    #[::core::prelude::v1::test]
    fn inactive_presentations_include_long_gaps_and_boundary_ages() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let first = start + Duration::from_millis(10);
        let last = first + Duration::from_millis(500);
        let samples = present_samples(
            &[
                draw(id, first),
                present(id, first),
                draw(id, last),
                present(id, last),
            ],
            id,
            start,
            last + Duration::from_millis(20),
        );
        assert!(samples.complete(2));
        assert_eq!(samples.intervals_ms, vec![500.0]);
        assert_eq!(p95(&samples.intervals_ms), Some(500.0));
        assert_eq!(samples.first_present_ms, Some(10.0));
        assert_eq!(samples.last_present_age_ms, Some(20.0));
        assert_eq!(samples.max_gap_ms, Some(500.0));
    }

    #[::core::prelude::v1::test]
    fn other_windows_do_not_interrupt_target_intervals() {
        let id = WindowId::from(1);
        let other = WindowId::from(2);
        let start = Instant::now();
        let last = start + Duration::from_millis(16);
        let samples = present_samples(
            &[
                draw(id, start),
                present(id, start),
                draw(other, last),
                present(other, last),
                draw(id, last),
                present(id, last),
            ],
            id,
            start,
            last,
        );
        assert!(samples.complete(2));
        assert_eq!(samples.intervals_ms, vec![16.0]);
    }

    #[::core::prelude::v1::test]
    fn empty_and_single_presentations_have_no_interval_quantile() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let empty = present_samples(&[], id, start, start);
        assert!(!empty.complete(0));
        assert_eq!(empty.max_gap_ms, None);
        assert_eq!(p95(&empty.intervals_ms), None);
        let single = present_samples(&[draw(id, start), present(id, start)], id, start, start);
        assert!(single.complete(1));
        assert_eq!(p95(&single.intervals_ms), None);
    }

    #[::core::prelude::v1::test]
    fn equal_or_backward_present_timestamps_fail_coverage() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let later = start + Duration::from_millis(10);
        for last in [start, later] {
            let samples = present_samples(
                &[
                    draw(id, later),
                    present(id, later),
                    draw(id, last),
                    present(id, last),
                ],
                id,
                start,
                later,
            );
            assert!(!samples.complete(2));
        }
    }

    #[::core::prelude::v1::test]
    fn missing_prefix_draws_fail_snapshot_coverage() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let samples = present_samples(&[draw(id, start), present(id, start)], id, start, start);
        assert!(!samples.complete(2));
    }

    #[::core::prelude::v1::test]
    fn consecutive_draws_before_presentation_have_complete_coverage() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let later = start + Duration::from_millis(16);
        let samples = present_samples(
            &[
                draw(id, start),
                draw(id, start),
                present(id, start),
                draw(id, later),
                present(id, later),
            ],
            id,
            start,
            later,
        );
        assert!(samples.complete(3));
        assert_eq!(samples.superseded_draw_events, 1);
        assert_eq!(samples.pending_draw_events, 0);
        assert_eq!(samples.intervals_ms, vec![16.0]);
    }

    #[::core::prelude::v1::test]
    fn presentation_without_pending_draw_fails_coverage() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let later = start + Duration::from_millis(16);
        let samples = present_samples(
            &[draw(id, start), present(id, start), present(id, later)],
            id,
            start,
            later,
        );
        assert!(!samples.complete(1));
        assert_eq!(samples.intervals_ms, vec![16.0]);
    }

    #[::core::prelude::v1::test]
    fn final_draw_without_presentation_fails_coverage() {
        let id = WindowId::from(1);
        let start = Instant::now();
        let samples = present_samples(
            &[draw(id, start), present(id, start), draw(id, start)],
            id,
            start,
            start,
        );
        assert!(!samples.complete(2));
        assert_eq!(samples.pending_draw_events, 1);
    }

    #[::core::prelude::v1::test]
    fn benchmark_window_is_non_activating_and_unthrottled() {
        let mut options = WindowOptions::default();
        configure_window(&mut options);

        assert!(!options.focus);
        assert_eq!(options.inactive_frame_interval, None);
        assert_eq!(options.kind, WindowKind::PopUp);
    }

    #[::core::prelude::v1::test]
    fn p95_uses_the_nearest_rank_at_or_above() {
        assert_eq!(p95(&[]), None);
        assert_eq!(p95(&[3.0, 1.0, 2.0]), Some(3.0));
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(p95(&samples), Some(96.0));
    }
}
