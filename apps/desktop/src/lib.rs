use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub enum Pipeline {
    Rust,
    Vips,
}

#[derive(Clone, Debug)]
pub struct Photo {
    pub source: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Thumbnail,
    Large,
}

impl PreviewKind {
    fn edge(self) -> u32 {
        match self {
            Self::Thumbnail => 480,
            Self::Large => 2400,
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Ready {
        index: usize,
        kind: PreviewKind,
        path: PathBuf,
        elapsed: Duration,
        cache_hit: bool,
    },
    Failed {
        index: usize,
        kind: PreviewKind,
        message: String,
    },
}

#[derive(Default)]
struct Jobs {
    thumbnails: VecDeque<usize>,
    preview: Option<usize>,
    preview_running: bool,
    stopped: bool,
}

pub struct Library {
    photos: Vec<Photo>,
    receiver: Receiver<Event>,
    jobs: Arc<(Mutex<Jobs>, Condvar)>,
}

impl Library {
    pub fn open(inputs: &[PathBuf], cache: PathBuf, pipeline: Pipeline) -> Result<Self> {
        if matches!(pipeline, Pipeline::Vips) {
            require_vips()?;
        }
        fs::create_dir_all(&cache)?;
        let mut sources = Vec::new();
        for input in inputs {
            collect_jpegs(input, &mut sources)?;
        }
        sources.sort();
        sources.dedup();
        let photos: Vec<_> = sources.into_iter().map(|source| Photo { source }).collect();
        let jobs = Arc::new((
            Mutex::new(Jobs {
                thumbnails: (0..photos.len()).collect(),
                ..Jobs::default()
            }),
            Condvar::new(),
        ));
        let (sender, receiver) = mpsc::sync_channel(16);
        // One lane per kind bounds processing to two jobs. New thumbnails wait
        // while a preview is pending/running; an in-flight thumbnail cannot delay
        // interactive work or be cancelled halfway through a cache write.
        for kind in [PreviewKind::Thumbnail, PreviewKind::Large] {
            let worker_jobs = jobs.clone();
            let sources = photos.clone();
            let sender = sender.clone();
            let cache = cache.clone();
            std::thread::spawn(move || {
                loop {
                    let (lock, wake) = &*worker_jobs;
                    let mut queue = lock.lock().unwrap();
                    let index = loop {
                        if queue.stopped {
                            return;
                        }
                        if kind == PreviewKind::Large {
                            if let Some(index) = queue.preview.take() {
                                queue.preview_running = true;
                                break index;
                            }
                        } else if queue.preview.is_none() && !queue.preview_running {
                            if let Some(index) = queue.thumbnails.pop_front() {
                                break index;
                            }
                        }
                        queue = wake.wait(queue).unwrap();
                    };
                    drop(queue);
                    let start = Instant::now();
                    let event =
                        match cached_preview(&sources[index].source, &cache, kind.edge(), pipeline)
                        {
                            Ok((path, cache_hit)) => Event::Ready {
                                index,
                                kind,
                                path,
                                elapsed: start.elapsed(),
                                cache_hit,
                            },
                            Err(error) => Event::Failed {
                                index,
                                kind,
                                message: error.to_string(),
                            },
                        };
                    if kind == PreviewKind::Large {
                        lock.lock().unwrap().preview_running = false;
                        wake.notify_all();
                    }
                    if sender.send(event).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Self {
            photos,
            receiver,
            jobs,
        })
    }

    pub fn photos(&self) -> &[Photo] {
        &self.photos
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn request_preview(&self, index: usize) -> Result<()> {
        anyhow::ensure!(index < self.photos.len(), "Photo does not exist");
        let (jobs, wake) = &*self.jobs;
        jobs.lock().unwrap().preview = Some(index);
        wake.notify_all();
        Ok(())
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let (jobs, wake) = &*self.jobs;
        jobs.lock().unwrap().stopped = true;
        wake.notify_all();
    }
}

fn collect_jpegs(path: &Path, sources: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        if path
            .extension()
            .is_some_and(|e| e == "photoslibrary" || e == "photolibrary")
            || path.file_name().is_some_and(|n| n == "Photo Booth Library")
        {
            return Ok(());
        }
        for entry in fs::read_dir(path)? {
            collect_jpegs(&entry?.path(), sources)?;
        }
    } else if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
    {
        sources.push(path.canonicalize()?);
    }
    Ok(())
}

fn cached_preview(
    source: &Path,
    cache: &Path,
    edge: u32,
    pipeline: Pipeline,
) -> Result<(PathBuf, bool)> {
    let metadata = fs::metadata(source)?;
    // Prototype cache identity, not content deduplication (that belongs to the catalog slice).
    let stamp = format!(
        "v1:{source:?}:{}:{:?}:{edge}:{pipeline:?}",
        metadata.len(),
        metadata.modified()?
    );
    let key = blake3::hash(stamp.as_bytes());
    let path = cache.join(format!("{key}.jpg"));
    if image::image_dimensions(&path).is_ok() {
        return Ok((path, true));
    }
    let temporary = cache.join(format!("{key}.{}.tmp.jpg", std::process::id()));
    let result = render_preview(source, &temporary, edge, pipeline);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    fs::rename(temporary, &path)?;
    Ok((path, false))
}

fn require_vips() -> Result<()> {
    static VERIFIED: OnceLock<()> = OnceLock::new();
    if VERIFIED.get().is_some() {
        return Ok(());
    }
    let output = std::process::Command::new("vips")
        .arg("--version")
        .output()
        .context("libvips 8.18.3 is required; see apps/desktop/README.md")?;
    let version = String::from_utf8_lossy(&output.stdout);
    anyhow::ensure!(
        output.status.success() && version.trim() == "vips-8.18.3",
        "Unsupported libvips: expected vips-8.18.3, found {}. See apps/desktop/README.md",
        version.trim()
    );
    let _ = VERIFIED.set(());
    Ok(())
}

pub fn render_preview(source: &Path, output: &Path, edge: u32, pipeline: Pipeline) -> Result<()> {
    if matches!(pipeline, Pipeline::Vips) {
        require_vips()?;
        let output = std::process::Command::new("vips")
            .env("VIPS_CONCURRENCY", "2")
            .arg("thumbnail")
            .arg(source)
            .arg(format!("{}[Q=85,keep=none]", output.display()))
            .arg(edge.to_string())
            .args([
                "--height",
                &edge.to_string(),
                "--size",
                "down",
                "--output-profile",
                "srgb",
            ])
            .output()
            .context("libvips is required: brew install vips")?;
        anyhow::ensure!(
            output.status.success(),
            "libvips: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let mut decoder = ImageReader::open(source)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let resized = image.resize(
        edge.min(image.width()),
        edge.min(image.height()),
        FilterType::Triangle,
    );
    JpegEncoder::new_with_quality(fs::File::create(output)?, 85)
        .encode_image(&resized)
        .context("encode preview")?;
    Ok(())
}
