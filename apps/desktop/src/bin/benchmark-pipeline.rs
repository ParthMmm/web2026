use anyhow::{Context, Result};
use photo_prototype::{DerivativeSize, Event, ImportOutcome, Library, LibraryOptions};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(120);

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .context("Usage: benchmark-pipeline <library-directory> <JPEG-or-folder>...")?,
    );
    let inputs: Vec<PathBuf> = args.map(PathBuf::from).collect();
    anyhow::ensure!(!inputs.is_empty(), "Provide JPEGs or a folder");
    let start = Instant::now();
    let library = Library::open(LibraryOptions::new(root))?;
    library.import(inputs);
    let mut durations = Vec::new();
    let summary = loop {
        match library.recv_timeout(EVENT_TIMEOUT)? {
            Event::Imported {
                source,
                outcome,
                elapsed,
                ..
            } => match outcome {
                ImportOutcome::Failed(message) => {
                    eprintln!("{}: {message}", source.display());
                }
                _ => durations.push(elapsed.as_secs_f64() * 1000.),
            },
            Event::ImportFinished { summary } => break summary,
            _ => {}
        }
    };
    let photos = library.current_photos()?;
    anyhow::ensure!(!photos.is_empty(), "No JPEGs found");
    // The GUI harness explicitly warms the same large-preview workload it will
    // measure. Grid hits alone say nothing about large-preview hits.
    let mut large_previews = 0;
    if let Ok(indices) = std::env::var("PHOTO_PREWARM_INDICES") {
        for index in indices.split(',').filter(|s| !s.is_empty()) {
            let photo = photos
                .get(index.parse::<usize>()?)
                .context("Prewarm index is outside the library")?;
            library.request_preview(&photo.id);
            loop {
                match library.recv_timeout(EVENT_TIMEOUT)? {
                    Event::Ready {
                        size: DerivativeSize::Desktop,
                        id,
                        ..
                    } if id == photo.id => {
                        large_previews += 1;
                        break;
                    }
                    event @ Event::DerivativeFailed { .. } => {
                        anyhow::bail!("Large-preview warming failed: {event:?}")
                    }
                    _ => {}
                }
            }
        }
    }
    durations.sort_by(f64::total_cmp);
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({
            "pipeline": "Vips", "photos": photos.len(), "failures": summary.failed,
            "added": summary.added, "duplicates": summary.duplicates,
            "unchanged": summary.unchanged,
            "large_previews_warmed": large_previews,
            "seconds": seconds,
            "photos_per_second": photos.len() as f64 / seconds,
            "thumbnail_p95_ms": durations.get((durations.len().saturating_sub(1) as f64 * 0.95).ceil() as usize)
        })
    );
    anyhow::ensure!(summary.failed == 0, "{} failed JPEGs", summary.failed);
    Ok(())
}
