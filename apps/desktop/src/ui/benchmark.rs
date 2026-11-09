//! Scripted browsing for `benchmark.py`: fixed-duration scrolling, periodic
//! preview requests, and a JSON report on stdout. The report keys are the
//! benchmark's contract.

use super::{frames::FrameSample, images::ImageSlots};
use std::time::{Duration, Instant};

/// Benchmark rows assume this many columns regardless of window width, so
/// preview indices match `benchmark.py`'s warm preparation.
pub(super) const COLUMNS: usize = 4;
const ROWS_PER_SECOND: f64 = 4.0;
const PREVIEW_INTERVAL_SECONDS: u64 = 3;
const PREVIEW_TAIL_SECONDS: u64 = 2;
/// AppKit's pop-up menu window level, above normal windows.
pub(super) const WINDOW_LEVEL: isize = 101;

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
    max_poll_gap: Duration,
    max_poll_gap_before: usize,
    poll_count: usize,
    first_poll_ms: Option<f64>,
    last_poll: Instant,
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
            max_poll_gap: Duration::ZERO,
            max_poll_gap_before: 0,
            poll_count: 0,
            first_poll_ms: None,
            last_poll: now,
        }
    }

    /// Time left in the run, or `None` once it is over.
    pub(super) fn remaining(&self) -> Option<Duration> {
        Duration::from_secs(self.seconds)
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
    }

    /// Records one poll. Rune polls once before every frame, so the gaps
    /// between polls bound the gaps between frames the app could build.
    pub(super) fn record_poll(&mut self) {
        self.poll_count += 1;
        let since_last = self.last_poll.elapsed();
        if self.poll_count == 1 {
            self.first_poll_ms = Some(millis(self.started.elapsed()));
        } else if since_last > self.max_poll_gap {
            self.max_poll_gap = since_last;
            self.max_poll_gap_before = self.poll_count;
        }
        self.last_poll = Instant::now();
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
        self,
        totals: &Totals,
        frames: &[FrameSample],
        thumbnails: &ImageSlots,
        previews: &ImageSlots,
    ) -> serde_json::Value {
        let ended = Instant::now();
        let presents = present_samples(frames, self.started, ended);
        let cpu_ms: Vec<f64> = frames.iter().map(|frame| frame.cpu_ms).collect();
        serde_json::json!({
            "photos": totals.photos,
            "thumbnails_completed": totals.thumbnails_completed,
            "failed": totals.failed,
            "seconds": self.started.elapsed().as_secs_f64(),
            "draw_samples": frames.len(),
            "last_render_age_ms": millis(self.last_poll.elapsed()),
            "draw_p95_ms": p95(&cpu_ms),
            "present_interval_p95_ms": quantile(&presents.intervals_ms, 0.95),
            "present_interval_p99_ms": quantile(&presents.intervals_ms, 0.99),
            "max_render_gap_ms": millis(self.max_poll_gap.max(self.last_poll.elapsed())),
            "max_render_gap_before_frame": self.max_poll_gap_before,
            "render_count": self.poll_count,
            "first_frame_ms": self.first_poll_ms,
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
            "thumbnail_cache_entries": thumbnails.len(),
            "preview_cache_entries": previews.len(),
            "thumbnail_cache_peak_entries": thumbnails.peak_entries,
            "preview_cache_peak_entries": previews.peak_entries,
            "thumbnail_peak_in_flight": thumbnails.peak_loading,
            "preview_peak_in_flight": previews.peak_loading,
            "present_interval_samples": presents.intervals_ms.len(),
            "presented_frames": frames.len(),
            // Rune presents every frame it encodes, so no draw is superseded
            // or left pending; the keys stay for the harness's accounting.
            "trace_draw_events": frames.len(),
            "superseded_draw_events": 0,
            "pending_draw_events": 0,
            "frame_timing_complete": presents.complete(frames.len()),
            "first_present_ms": presents.first_present_ms,
            "last_present_age_ms": presents.last_present_age_ms,
            "max_present_gap_ms": presents.max_gap_ms,
            "timing_definition": "v2 (Rune): drawn frames from App::on_frame_encoded; intervals are gaps between display-paced frame starts; draw is CPU update plus encode; render gaps are gaps between polls; quantile rank ceil((n-1)*q); excludes GPU time and physical display scanout"
        })
    }
}

struct PresentSamples {
    intervals_ms: Vec<f64>,
    ordered: bool,
    first_present_ms: Option<f64>,
    last_present_age_ms: Option<f64>,
    max_gap_ms: Option<f64>,
}

impl PresentSamples {
    fn complete(&self, frames: usize) -> bool {
        self.ordered && frames > 0 && self.intervals_ms.len() == frames - 1
    }
}

fn present_samples(frames: &[FrameSample], started: Instant, ended: Instant) -> PresentSamples {
    let mut ordered = ended >= started;
    let mut intervals_ms = Vec::new();
    for pair in frames.windows(2) {
        if pair[1].started > pair[0].started {
            intervals_ms.push(millis(pair[1].started.duration_since(pair[0].started)));
        } else {
            ordered = false;
        }
    }
    ordered &= frames
        .iter()
        .all(|frame| frame.submitted >= started && frame.submitted <= ended);
    let first_present_ms = frames
        .first()
        .and_then(|frame| frame.submitted.checked_duration_since(started))
        .map(millis);
    let last_present_age_ms = frames
        .last()
        .and_then(|frame| ended.checked_duration_since(frame.submitted))
        .map(millis);
    let max_gap_ms = first_present_ms
        .into_iter()
        .chain(last_present_age_ms)
        .chain(intervals_ms.iter().copied())
        .max_by(f64::total_cmp);
    PresentSamples {
        intervals_ms,
        ordered,
        first_present_ms,
        last_present_age_ms,
        max_gap_ms,
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

    fn frame(started: Instant, offset_ms: u64) -> FrameSample {
        let at = started + Duration::from_millis(offset_ms);
        FrameSample {
            started: at,
            submitted: at + Duration::from_millis(2),
            cpu_ms: 2.0,
        }
    }

    #[test]
    fn boundary_ages_count_as_gaps() {
        let started = Instant::now();
        let frames = [frame(started, 300), frame(started, 316)];
        let samples = present_samples(&frames, started, started + Duration::from_millis(1000));

        assert!(samples.complete(frames.len()));
        assert_eq!(samples.intervals_ms, vec![16.0]);
        assert_eq!(samples.first_present_ms, Some(302.0));
        assert_eq!(samples.last_present_age_ms, Some(682.0));
        assert_eq!(samples.max_gap_ms, Some(682.0));
    }

    #[test]
    fn no_frames_have_no_interval_and_are_incomplete() {
        let started = Instant::now();
        let samples = present_samples(&[], started, started + Duration::from_millis(10));

        assert!(!samples.complete(0));
        assert_eq!(quantile(&samples.intervals_ms, 0.95), None);
        assert_eq!(samples.max_gap_ms, None);
    }

    #[test]
    fn repeated_frame_starts_fail_coverage() {
        let started = Instant::now();
        let frames = [frame(started, 10), frame(started, 10)];
        let samples = present_samples(&frames, started, started + Duration::from_millis(100));

        assert!(!samples.complete(frames.len()));
    }

    #[test]
    fn frames_after_the_run_ended_fail_coverage() {
        let started = Instant::now();
        let frames = [frame(started, 10), frame(started, 200)];
        let samples = present_samples(&frames, started, started + Duration::from_millis(100));

        assert!(!samples.complete(frames.len()));
    }

    #[test]
    fn p95_uses_the_nearest_rank_at_or_above() {
        assert_eq!(p95(&[]), None);
        assert_eq!(p95(&[3.0, 1.0, 2.0]), Some(3.0));
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(p95(&samples), Some(96.0));
    }
}
