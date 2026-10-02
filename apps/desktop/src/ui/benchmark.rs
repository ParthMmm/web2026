//! Scripted browsing for `benchmark.py`: fixed-duration scrolling, periodic
//! preview requests, and a JSON report on stdout. The report keys are the
//! benchmark's contract.

use super::cache::BoundedCache;
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

/// Gallery facts the report needs.
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
        &self,
        totals: &Totals,
        thumbnails: &Entity<BoundedCache>,
        previews: &Entity<BoundedCache>,
        window: &Window,
        cx: &App,
    ) -> serde_json::Value {
        let frames = window.frame_duration_snapshot();
        let thumbnails = thumbnails.read(cx);
        let previews = previews.read(cx);
        #[allow(clippy::cast_precision_loss)]
        let histogram_ms = |nanos: u64| nanos as f64 / 1e6;
        serde_json::json!({
            "photos": totals.photos,
            "thumbnails_completed": totals.thumbnails_completed,
            "failed": totals.failed,
            "seconds": self.started.elapsed().as_secs_f64(),
            "draw_samples": frames.draw_duration_histogram.len(),
            "last_render_age_ms": millis(self.last_render.elapsed()),
            "draw_p95_ms": histogram_ms(frames.draw_duration_histogram.value_at_quantile(0.95)),
            "present_interval_p95_ms": histogram_ms(frames.present_interval_histogram.value_at_quantile(0.95)),
            "present_interval_p99_ms": histogram_ms(frames.present_interval_histogram.value_at_quantile(0.99)),
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
        })
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn p95(samples: &[f64]) -> Option<f64> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let index = (sorted.len().saturating_sub(1) as f64 * 0.95).ceil() as usize;
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

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
