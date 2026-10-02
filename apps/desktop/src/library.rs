//! The persistent photo library: import, duplicate detection, derivative
//! generation, and preview scheduling. All file and image work runs on two
//! worker lanes owned by [`Library`]; callers receive [`Event`]s.

use crate::{
    catalog::{
        Catalog, DerivativeRecord, ImportFailure, NewPhoto, PhotoId, PhotoRecord, SourceStamp,
    },
    derivative::{self, DerivativeSize, RECIPE_VERSION},
    metadata::read_source_exif,
};
use anyhow::{Context, Result};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    },
    time::{Duration, Instant},
};

/// Where a library lives and how much background work it does.
#[derive(Clone, Debug)]
pub struct LibraryOptions {
    root: PathBuf,
    prepare_web_sizes: bool,
}

impl LibraryOptions {
    /// `root` holds `catalog.sqlite` and the `derivatives` directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            prepare_web_sizes: false,
        }
    }

    /// Also render mobile and desktop sizes for every photo once imports and
    /// grid repairs are idle. Interactive previews always take priority.
    pub fn prepare_web_sizes(mut self, prepare: bool) -> Self {
        self.prepare_web_sizes = prepare;
        self
    }
}

/// What happened to one source file during an import.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportOutcome {
    /// New content, now in the library with a grid derivative.
    Added(PhotoRecord),
    /// Bytes already in the library under another path.
    Duplicate(PhotoRecord),
    /// Bytes whose every known path was missing; this path reconnects them.
    Relinked(PhotoRecord),
    /// Already imported from this path and unchanged.
    Unchanged(PhotoId),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    pub duplicates: usize,
    pub relinked: usize,
    pub unchanged: usize,
    pub failed: usize,
}

impl ImportSummary {
    fn count(&mut self, outcome: &ImportOutcome) {
        match outcome {
            ImportOutcome::Added(_) => self.added += 1,
            ImportOutcome::Duplicate(_) => self.duplicates += 1,
            ImportOutcome::Relinked(_) => self.relinked += 1,
            ImportOutcome::Unchanged(_) => self.unchanged += 1,
            ImportOutcome::Failed(_) => self.failed += 1,
        }
    }
}

#[derive(Debug)]
pub enum Event {
    /// Scanning discovered more files; `total` counts the whole import run.
    ImportProgress {
        processed: usize,
        total: usize,
    },
    Imported {
        source: PathBuf,
        outcome: ImportOutcome,
        processed: usize,
        total: usize,
        elapsed: Duration,
    },
    /// Every queued import has been processed.
    ImportFinished {
        summary: ImportSummary,
    },
    /// A photo lost its last source because that file's content changed.
    Removed {
        id: PhotoId,
    },
    Ready {
        id: PhotoId,
        size: DerivativeSize,
        path: PathBuf,
        elapsed: Duration,
        cache_hit: bool,
    },
    DerivativeFailed {
        id: PhotoId,
        size: DerivativeSize,
        message: String,
    },
    FailuresCleared,
}

struct PendingSource {
    path: PathBuf,
    /// App-owned copies (for example from the Photos picker) are deleted when
    /// they duplicate existing content or fail, instead of lingering unused.
    owned: bool,
}

struct ImportRequest {
    inputs: Vec<PathBuf>,
    owned: bool,
}

#[derive(Default)]
struct Queue {
    requests: VecDeque<ImportRequest>,
    sources: VecDeque<PendingSource>,
    grid_repairs: VecDeque<PhotoId>,
    web_sizes: VecDeque<(PhotoId, DerivativeSize)>,
    clear_failures: bool,
    preview: Option<PhotoId>,
    preview_running: bool,
    processed: usize,
    total: usize,
    summary: ImportSummary,
    stopped: bool,
}

impl Queue {
    fn has_background_work(&self, prepare_web_sizes: bool) -> bool {
        self.clear_failures
            || !self.requests.is_empty()
            || !self.sources.is_empty()
            || !self.grid_repairs.is_empty()
            || (prepare_web_sizes && !self.web_sizes.is_empty())
    }
}

enum Job {
    ClearFailures,
    Scan(ImportRequest),
    Import(PendingSource),
    Derivative(PhotoId, DerivativeSize),
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    catalog: Mutex<Catalog>,
    derivatives: PathBuf,
    prepare_web_sizes: bool,
}

impl Shared {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn catalog(&self) -> MutexGuard<'_, Catalog> {
        self.catalog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn derivative_path(&self, id: &PhotoId, size: DerivativeSize) -> PathBuf {
        self.derivatives.join(format!("{id}-{}.webp", size.key()))
    }

    fn has_current_derivative(&self, id: &PhotoId, size: DerivativeSize) -> Result<bool> {
        let recorded = self.catalog().derivative(id, size)?;
        Ok(
            recorded.is_some_and(|record| record.recipe == RECIPE_VERSION)
                && self.derivative_path(id, size).is_file(),
        )
    }
}

pub struct Library {
    root: PathBuf,
    photos: Vec<PhotoRecord>,
    failures: Vec<ImportFailure>,
    receiver: Receiver<Event>,
    shared: Arc<Shared>,
}

impl Library {
    /// Opens or creates the library, marks sources that are no longer on disk
    /// as missing, and schedules repairs for absent derivatives.
    pub fn open(options: LibraryOptions) -> Result<Self> {
        derivative::require_vips()?;
        let derivatives = options.root.join("derivatives");
        fs::create_dir_all(&derivatives)
            .with_context(|| format!("Couldn't create {}", derivatives.display()))?;
        let mut catalog = Catalog::open(&options.root.join("catalog.sqlite"))?;
        for (path, was_missing) in catalog.source_paths()? {
            let missing = !path.is_file();
            if missing != was_missing {
                catalog.set_source_missing(&path, missing)?;
            }
        }
        let photos = catalog.photos()?;
        let failures = catalog.failures()?;
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            catalog: Mutex::new(catalog),
            derivatives,
            prepare_web_sizes: options.prepare_web_sizes,
        });
        {
            let mut queue = shared.queue();
            for photo in &photos {
                if !shared.has_current_derivative(&photo.id, DerivativeSize::Grid)? {
                    queue.grid_repairs.push_back(photo.id.clone());
                }
                if options.prepare_web_sizes {
                    for size in [DerivativeSize::Mobile, DerivativeSize::Desktop] {
                        if !photo.source_missing
                            && !shared.has_current_derivative(&photo.id, size)?
                        {
                            queue.web_sizes.push_back((photo.id.clone(), size));
                        }
                    }
                }
            }
        }
        let (sender, receiver) = mpsc::sync_channel(64);
        // Two lanes bound encoder work to two helper processes. Background work
        // waits while an interactive preview is pending or running; a running
        // background job is never cancelled halfway through a write.
        let background = (shared.clone(), sender.clone());
        std::thread::Builder::new()
            .name("library-background".into())
            .spawn(move || run_background(&background.0, &background.1))?;
        let interactive = shared.clone();
        std::thread::Builder::new()
            .name("library-interactive".into())
            .spawn(move || run_interactive(&interactive, &sender))?;
        Ok(Self {
            root: options.root,
            photos,
            failures,
            receiver,
            shared,
        })
    }

    /// Photos present when the library was opened, ordered by source path.
    pub fn photos(&self) -> &[PhotoRecord] {
        &self.photos
    }

    /// Reads the catalog again. Blocks on the catalog lock; not for UI threads.
    pub fn current_photos(&self) -> Result<Vec<PhotoRecord>> {
        self.shared.catalog().photos()
    }

    /// Import failures recorded before this session, ordered by path.
    pub fn failures(&self) -> &[ImportFailure] {
        &self.failures
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn derivative_path(&self, id: &PhotoId, size: DerivativeSize) -> PathBuf {
        self.shared.derivative_path(id, size)
    }

    /// Queues JPEG files and folders (searched recursively). Originals are
    /// only read, never modified or moved.
    pub fn import(&self, inputs: Vec<PathBuf>) {
        self.enqueue(ImportRequest {
            inputs,
            owned: false,
        });
    }

    /// Like [`Self::import`], for copies the app created itself. Copies that
    /// duplicate existing content or fail to import are deleted.
    pub fn import_copies(&self, inputs: Vec<PathBuf>) {
        self.enqueue(ImportRequest {
            inputs,
            owned: true,
        });
    }

    fn enqueue(&self, request: ImportRequest) {
        self.shared.queue().requests.push_back(request);
        self.shared.wake.notify_all();
    }

    /// Requests the large preview. A newer request replaces an older pending one.
    pub fn request_preview(&self, id: &PhotoId) {
        self.shared.queue().preview = Some(id.clone());
        self.shared.wake.notify_all();
    }

    pub fn clear_failures(&self) {
        self.shared.queue().clear_failures = true;
        self.shared.wake.notify_all();
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        self.shared.queue().stopped = true;
        self.shared.wake.notify_all();
    }
}

fn run_background(shared: &Shared, sender: &SyncSender<Event>) {
    loop {
        let job = {
            let mut queue = shared.queue();
            loop {
                if queue.stopped {
                    return;
                }
                let interactive_busy = queue.preview.is_some() || queue.preview_running;
                if !interactive_busy && queue.has_background_work(shared.prepare_web_sizes) {
                    break;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if std::mem::take(&mut queue.clear_failures) {
                Job::ClearFailures
            } else if let Some(request) = queue.requests.pop_front() {
                Job::Scan(request)
            } else if let Some(source) = queue.sources.pop_front() {
                Job::Import(source)
            } else if let Some(id) = queue.grid_repairs.pop_front() {
                Job::Derivative(id, DerivativeSize::Grid)
            } else if let Some((id, size)) = queue.web_sizes.pop_front() {
                Job::Derivative(id, size)
            } else {
                continue;
            }
        };
        let events = match job {
            Job::ClearFailures => {
                let _ = shared.catalog().clear_failures();
                vec![Event::FailuresCleared]
            }
            Job::Scan(request) => scan(shared, request),
            Job::Import(source) => import_one(shared, source),
            Job::Derivative(id, size) => vec![derivative_event(shared, id, size)],
        };
        for event in events {
            if sender.send(event).is_err() {
                return;
            }
        }
    }
}

fn run_interactive(shared: &Shared, sender: &SyncSender<Event>) {
    loop {
        let id = {
            let mut queue = shared.queue();
            loop {
                if queue.stopped {
                    return;
                }
                if let Some(id) = queue.preview.take() {
                    queue.preview_running = true;
                    break id;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let event = derivative_event(shared, id, DerivativeSize::Desktop);
        shared.queue().preview_running = false;
        shared.wake.notify_all();
        if sender.send(event).is_err() {
            return;
        }
    }
}

fn scan(shared: &Shared, request: ImportRequest) -> Vec<Event> {
    let mut found = Vec::new();
    let mut events = Vec::new();
    let mut failed = Vec::new();
    for input in &request.inputs {
        if let Err(error) = collect_jpegs(input, &mut found) {
            failed.push((input.clone(), format!("{error:#}")));
        }
    }
    found.sort();
    found.dedup();
    let mut queue = shared.queue();
    let already_queued: HashSet<PathBuf> = queue
        .sources
        .iter()
        .map(|source| source.path.clone())
        .collect();
    found.retain(|path| !already_queued.contains(path));
    queue.total += found.len() + failed.len();
    queue
        .sources
        .extend(found.into_iter().map(|path| PendingSource {
            path,
            owned: request.owned,
        }));
    for (input, message) in failed {
        let _ = shared.catalog().record_failure(&input, &message);
        let outcome = ImportOutcome::Failed(message);
        queue.summary.count(&outcome);
        queue.processed += 1;
        events.push(Event::Imported {
            source: input,
            outcome,
            processed: queue.processed,
            total: queue.total,
            elapsed: Duration::ZERO,
        });
    }
    events.push(Event::ImportProgress {
        processed: queue.processed,
        total: queue.total,
    });
    events.extend(finish_if_idle(&mut queue));
    events
}

fn finish_if_idle(queue: &mut Queue) -> Option<Event> {
    let idle = queue.requests.is_empty() && queue.sources.is_empty();
    if !idle || queue.total == 0 {
        return None;
    }
    let summary = std::mem::take(&mut queue.summary);
    queue.processed = 0;
    queue.total = 0;
    Some(Event::ImportFinished { summary })
}

fn import_one(shared: &Shared, source: PendingSource) -> Vec<Event> {
    let start = Instant::now();
    let mut events = Vec::new();
    let outcome = match import_source(shared, &source, &mut events) {
        Ok(outcome) => outcome,
        Err(error) => {
            let message = format!("{error:#}");
            let _ = shared.catalog().record_failure(&source.path, &message);
            ImportOutcome::Failed(message)
        }
    };
    let discard_copy = matches!(
        outcome,
        ImportOutcome::Duplicate(_) | ImportOutcome::Failed(_)
    );
    if source.owned && discard_copy {
        remove_copy(shared, &source.path);
    }
    if shared.prepare_web_sizes
        && let ImportOutcome::Added(photo) | ImportOutcome::Relinked(photo) = &outcome
    {
        let mut queue = shared.queue();
        for size in [DerivativeSize::Mobile, DerivativeSize::Desktop] {
            queue.web_sizes.push_back((photo.id.clone(), size));
        }
    }
    let mut queue = shared.queue();
    queue.summary.count(&outcome);
    queue.processed += 1;
    events.push(Event::Imported {
        source: source.path,
        outcome,
        processed: queue.processed,
        total: queue.total,
        elapsed: start.elapsed(),
    });
    events.extend(finish_if_idle(&mut queue));
    events
}

fn import_source(
    shared: &Shared,
    source: &PendingSource,
    events: &mut Vec<Event>,
) -> Result<ImportOutcome> {
    let path = source.path.as_path();
    let stamp = SourceStamp::read(path).context("Couldn't read the file")?;
    let known = shared.catalog().source(path)?;
    // Bind lookups before branching: a catalog guard held in an `if let`
    // condition would live through the branch and deadlock nested locks.
    let unchanged_photo = match &known {
        Some(known) if known.stamp == stamp => shared.catalog().photo(&known.photo_id)?,
        _ => None,
    };
    if let (Some(known), Some(photo)) = (&known, unchanged_photo) {
        ensure_grid(shared, &photo.id, path)?;
        if known.missing || photo.source_missing {
            shared.catalog().set_source_missing(path, false)?;
            return relinked(shared, &photo.id);
        }
        return Ok(ImportOutcome::Unchanged(photo.id));
    }

    let id = hash_file(path)?;
    let existing = shared.catalog().photo(&id)?;
    if let Some(existing) = existing {
        let same_path = known.as_ref().is_some_and(|known| known.photo_id == id);
        let was_missing = existing.source_missing;
        if source.owned && !was_missing && !same_path {
            return Ok(ImportOutcome::Duplicate(existing));
        }
        shared.catalog().link_source(&id, path, stamp)?;
        remove_orphans(shared, events)?;
        ensure_grid(shared, &id, path)?;
        return if same_path {
            Ok(ImportOutcome::Unchanged(id))
        } else if was_missing {
            relinked(shared, &id)
        } else {
            let photo = shared.catalog().photo(&id)?.context("Photo disappeared")?;
            Ok(ImportOutcome::Duplicate(photo))
        };
    }

    let exif = read_source_exif(path);
    let (mut width, mut height) = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
        .map_err(|_| anyhow::anyhow!("Not a readable JPEG"))?;
    if exif.swaps_dimensions() {
        std::mem::swap(&mut width, &mut height);
    }
    let grid = shared.derivative_path(&id, DerivativeSize::Grid);
    let (grid_width, grid_height) = derivative::render(path, &grid, DerivativeSize::Grid)?;
    {
        let mut catalog = shared.catalog();
        catalog.insert_photo(NewPhoto {
            id: &id,
            source: path,
            stamp,
            width,
            height,
            metadata: &exif.metadata,
        })?;
        catalog.record_derivative(
            &id,
            DerivativeSize::Grid,
            DerivativeRecord {
                width: grid_width,
                height: grid_height,
                recipe: RECIPE_VERSION,
            },
        )?;
    }
    remove_orphans(shared, events)?;
    let photo = shared.catalog().photo(&id)?.context("Photo disappeared")?;
    Ok(ImportOutcome::Added(photo))
}

/// A path whose bytes changed in place now belongs to different content; a
/// photo left without any path is removed with its derivatives.
fn remove_orphans(shared: &Shared, events: &mut Vec<Event>) -> Result<()> {
    let removed = shared.catalog().remove_orphans()?;
    for orphan in removed {
        for size in DerivativeSize::ALL {
            let _ = fs::remove_file(shared.derivative_path(&orphan, size));
        }
        events.push(Event::Removed { id: orphan });
    }
    Ok(())
}

fn relinked(shared: &Shared, id: &PhotoId) -> Result<ImportOutcome> {
    let photo = shared.catalog().photo(id)?.context("Photo disappeared")?;
    Ok(ImportOutcome::Relinked(photo))
}

/// Re-renders a grid derivative that was deleted or made by an older recipe.
fn ensure_grid(shared: &Shared, id: &PhotoId, source: &Path) -> Result<()> {
    if shared.has_current_derivative(id, DerivativeSize::Grid)? {
        return Ok(());
    }
    let output = shared.derivative_path(id, DerivativeSize::Grid);
    let (width, height) = derivative::render(source, &output, DerivativeSize::Grid)?;
    shared.catalog().record_derivative(
        id,
        DerivativeSize::Grid,
        DerivativeRecord {
            width,
            height,
            recipe: RECIPE_VERSION,
        },
    )
}

fn derivative_event(shared: &Shared, id: PhotoId, size: DerivativeSize) -> Event {
    let start = Instant::now();
    match ensure_derivative(shared, &id, size) {
        Ok((path, cache_hit)) => Event::Ready {
            id,
            size,
            path,
            elapsed: start.elapsed(),
            cache_hit,
        },
        Err(error) => Event::DerivativeFailed {
            id,
            size,
            message: format!("{error:#}"),
        },
    }
}

fn ensure_derivative(
    shared: &Shared,
    id: &PhotoId,
    size: DerivativeSize,
) -> Result<(PathBuf, bool)> {
    let output = shared.derivative_path(id, size);
    if shared.has_current_derivative(id, size)? {
        return Ok((output, true));
    }
    let photo = shared
        .catalog()
        .photo(id)?
        .context("This photo is no longer in the library")?;
    anyhow::ensure!(
        !photo.source_missing,
        "The original is missing from {}. Import it from its new location to reconnect it.",
        photo.source.display()
    );
    let recorded = shared.catalog().source(&photo.source)?;
    let current = SourceStamp::read(&photo.source).context("Couldn't read the original")?;
    anyhow::ensure!(
        recorded.is_some_and(|recorded| recorded.stamp == current),
        "The original changed after import. Import it again to refresh this photo."
    );
    let (width, height) = derivative::render(&photo.source, &output, size)?;
    shared.catalog().record_derivative(
        id,
        size,
        DerivativeRecord {
            width,
            height,
            recipe: RECIPE_VERSION,
        },
    )?;
    Ok((output, false))
}

fn hash_file(path: &Path) -> Result<PhotoId> {
    let file = fs::File::open(path).context("Couldn't read the file")?;
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_reader(file)
        .context("Couldn't read the file")?;
    Ok(PhotoId::from_hash(hasher.finalize()))
}

fn remove_copy(shared: &Shared, path: &Path) {
    let _ = fs::remove_file(path);
    let _ = shared.catalog().clear_failure(path);
    if let Some(parent) = path.parent() {
        // Only succeeds once the import session directory is empty.
        let _ = fs::remove_dir(parent);
    }
}

fn collect_jpegs(path: &Path, sources: &mut Vec<PathBuf>) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Couldn't read {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        let skipped = path
            .extension()
            .is_some_and(|extension| extension == "photoslibrary" || extension == "photolibrary")
            || path
                .file_name()
                .is_some_and(|name| name == "Photo Booth Library");
        if skipped {
            return Ok(());
        }
        let entries = fs::read_dir(path)
            .with_context(|| format!("Couldn't open folder {}", path.display()))?;
        for entry in entries {
            collect_jpegs(&entry?.path(), sources)?;
        }
    } else if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg")
        })
    {
        sources.push(path.canonicalize()?);
    }
    Ok(())
}
