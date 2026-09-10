use anyhow::{Context, Result};
use photo_prototype::{Event, Library, Pipeline, PreviewKind};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let pipeline = match args.next().as_deref().and_then(|s| s.to_str()) {
        Some("rust") => Pipeline::Rust,
        Some("vips") => Pipeline::Vips,
        _ => anyhow::bail!(
            "Usage: benchmark-pipeline <rust|vips> <cache-directory> <JPEG-or-folder>..."
        ),
    };
    let cache = PathBuf::from(args.next().context("Missing cache directory")?);
    let inputs: Vec<PathBuf> = args.map(PathBuf::from).collect();
    anyhow::ensure!(!inputs.is_empty(), "Provide JPEGs or a folder");
    let start = Instant::now();
    let library = Library::open(&inputs, cache, pipeline)?;
    let count = library.photos().len();
    anyhow::ensure!(count > 0, "No JPEGs found");
    let mut failures = 0;
    let mut durations = Vec::new();
    for _ in 0..count {
        match library.recv_timeout(Duration::from_secs(120))? {
            Event::Ready { elapsed, .. } => durations.push(elapsed.as_secs_f64() * 1000.),
            Event::Failed { index, message, .. } => {
                failures += 1;
                eprintln!("Photo {index}: {message}");
            }
        }
    }
    // The GUI harness explicitly warms the same large-preview workload it will
    // measure. Thumbnail hits alone say nothing about large-preview hits.
    let mut large_previews = 0;
    if let Ok(indices) = std::env::var("PHOTO_PREWARM_INDICES") {
        for index in indices.split(',').filter(|s| !s.is_empty()) {
            let index = index.parse::<usize>()?;
            library.request_preview(index)?;
            match library.recv_timeout(Duration::from_secs(120))? {
                Event::Ready {
                    kind: PreviewKind::Large,
                    ..
                } => large_previews += 1,
                event => anyhow::bail!("Large-preview warming failed: {event:?}"),
            }
        }
    }
    durations.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "pipeline": format!("{pipeline:?}"), "photos": count, "failures": failures,
            "large_previews_warmed": large_previews,
            "seconds": start.elapsed().as_secs_f64(),
            "photos_per_second": count as f64 / start.elapsed().as_secs_f64(),
            "thumbnail_p95_ms": durations.get((durations.len().saturating_sub(1) as f64 * 0.95).ceil() as usize)
        })
    );
    anyhow::ensure!(failures == 0, "{failures} failed JPEGs");
    Ok(())
}
