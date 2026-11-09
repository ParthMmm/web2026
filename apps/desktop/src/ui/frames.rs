use rune::{FrameEncoded, FrameOutcome};
use std::{collections::VecDeque, time::Instant};

/// Frames kept for the performance panel: about two seconds at 120 Hz.
const RECENT_FRAMES: usize = 240;

/// One drawn frame of the window.
#[derive(Clone, Copy, Debug)]
pub(super) struct FrameSample {
    /// When the frame started, after its drawable came. A display-paced
    /// window starts each frame on a refresh, so the gaps between starts are
    /// the gaps between frames on screen.
    pub(super) started: Instant,
    /// When encoding finished and the frame was submitted for presentation.
    pub(super) submitted: Instant,
    /// CPU time to build and encode the frame: events, poll, view, layout,
    /// paint, then the renderer's prepare and draw. Not GPU time.
    pub(super) cpu_ms: f64,
}

/// The window's drawn frames, from `App::on_frame_encoded`.
#[derive(Default)]
pub(super) struct FrameLog {
    recent: VecDeque<FrameSample>,
    recording: Option<Vec<FrameSample>>,
}

impl FrameLog {
    pub(super) fn record(&mut self, frame: &FrameEncoded) {
        if frame.outcome() != FrameOutcome::Drawn {
            return;
        }
        let Some(started) = frame.started() else {
            return;
        };
        let sample = FrameSample {
            started,
            submitted: frame.at(),
            cpu_ms: frame.update_ms().unwrap_or(0.0) + frame.encode_ms().unwrap_or(0.0),
        };
        if self.recent.len() == RECENT_FRAMES {
            self.recent.pop_front();
        }
        self.recent.push_back(sample);
        if let Some(recording) = &mut self.recording {
            recording.push(sample);
        }
    }

    pub(super) fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    pub(super) fn stop_recording(&mut self) -> Vec<FrameSample> {
        self.recording.take().unwrap_or_default()
    }

    /// Frames drawn in the last second, and the slowest frame's CPU time
    /// among them. `None` when nothing drew in that second.
    pub(super) fn last_second(&self, now: Instant) -> Option<(usize, f64)> {
        let frames: Vec<&FrameSample> = self
            .recent
            .iter()
            .filter(|frame| now.duration_since(frame.submitted).as_secs_f64() <= 1.0)
            .collect();
        let slowest = frames
            .iter()
            .map(|frame| frame.cpu_ms)
            .max_by(f64::total_cmp)?;
        Some((frames.len(), slowest))
    }
}
