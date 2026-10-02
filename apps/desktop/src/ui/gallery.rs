use super::{
    benchmark::{self, Benchmark, Totals},
    cache::BoundedCache,
    format,
};
use crate::{
    DerivativeSize, Event, ImportFailure, ImportOutcome, Library, LibraryOptions, PhotoId,
    PhotoRecord,
    grid::{self, Movement},
    photos,
};
use gpui_kit::{
    Context, Entity, FocusHandle, PathPromptOptions, ScrollStrategy, SharedString, Subscription,
    UniformListScrollHandle, Window, component::Theme,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const THUMBNAIL_CACHE_LIMIT: usize = 160;
const BENCHMARK_THUMBNAIL_CACHE_LIMIT: usize = 64;
const THUMBNAIL_DECODES: usize = 4;
const PREVIEW_CACHE_LIMIT: usize = 2;
const PREVIEW_DECODES: usize = 1;
const EVENTS_PER_TICK: usize = 32;
const BUSY_POLL: Duration = Duration::from_millis(16);
const IDLE_POLL: Duration = Duration::from_millis(100);

pub(super) enum Thumbnail {
    Pending,
    Ready(Arc<Path>),
    Failed,
}

pub(super) struct GalleryPhoto {
    pub(super) record: PhotoRecord,
    pub(super) element_id: SharedString,
    pub(super) thumbnail: Thumbnail,
}

impl GalleryPhoto {
    fn new(record: PhotoRecord, thumbnail: Thumbnail) -> Self {
        Self {
            element_id: format!("photo-{}", record.id).into(),
            record,
            thumbnail,
        }
    }
}

/// The large rendition of the selected photo.
pub(super) struct Preview {
    pub(super) id: PhotoId,
    pub(super) image: Option<Arc<Path>>,
    pub(super) error: Option<SharedString>,
}

pub(super) enum LibraryState {
    Opening,
    Open(Library),
    Unavailable(SharedString),
}

struct PendingImport {
    inputs: Vec<PathBuf>,
    owned: bool,
}

/// Grid geometry from the last layout, used by keyboard navigation.
#[derive(Clone, Copy)]
pub(super) struct GridMetrics {
    pub(super) columns: usize,
    pub(super) page_rows: usize,
}

pub struct GalleryOptions {
    pub library_root: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub benchmark_seconds: Option<u64>,
}

pub struct Gallery {
    pub(super) library_root: PathBuf,
    pub(super) library: LibraryState,
    pub(super) photos: Vec<GalleryPhoto>,
    pub(super) failures: Vec<ImportFailure>,
    pub(super) selected: Option<PhotoId>,
    pub(super) preview: Option<Preview>,
    pub(super) viewer_open: bool,
    /// Whether the sidebar is collapsed to icon width. The library's own
    /// sidebar component owns the 200ms transition; this is only the state.
    pub(super) sidebar_collapsed: bool,
    pub(super) progress: Option<(usize, usize)>,
    pub(super) status: SharedString,
    pub(super) picking: bool,
    pub(super) show_performance: bool,
    pub(super) thumbnails_cache: Entity<BoundedCache>,
    pub(super) preview_cache: Entity<BoundedCache>,
    pub(super) scroll: UniformListScrollHandle,
    pub(super) grid_focus: FocusHandle,
    pub(super) viewer_focus: FocusHandle,
    pub(super) metrics: GridMetrics,
    pub(super) benchmark: Option<Benchmark>,
    pending_imports: Vec<PendingImport>,
    session_failures: usize,
    _subscriptions: Vec<Subscription>,
}

impl Gallery {
    pub fn new(options: GalleryOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let benchmark = options.benchmark_seconds.map(Benchmark::new);
        let thumbnail_limit = if benchmark.is_some() {
            BENCHMARK_THUMBNAIL_CACHE_LIMIT
        } else {
            THUMBNAIL_CACHE_LIMIT
        };
        let grid_focus = cx.focus_handle();
        window.focus(&grid_focus, cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            // Every appearance change strips the window's visual-effect view, so
            // the material has to be re-applied here too or the chrome goes
            // permanently opaque the first time macOS flips light and dark.
            super::reapply_window_background(window, cx);
        });
        let mut gallery = Self {
            library_root: options.library_root,
            library: LibraryState::Opening,
            photos: Vec::new(),
            failures: Vec::new(),
            selected: None,
            preview: None,
            viewer_open: false,
            sidebar_collapsed: false,
            progress: None,
            status: "Opening library…".into(),
            picking: false,
            show_performance: false,
            thumbnails_cache: BoundedCache::new(thumbnail_limit, THUMBNAIL_DECODES, cx),
            preview_cache: BoundedCache::new(PREVIEW_CACHE_LIMIT, PREVIEW_DECODES, cx),
            scroll: UniformListScrollHandle::default(),
            grid_focus,
            viewer_focus: cx.focus_handle(),
            metrics: GridMetrics {
                columns: benchmark::COLUMNS,
                page_rows: 1,
            },
            benchmark,
            pending_imports: Vec::new(),
            session_failures: 0,
            _subscriptions: vec![appearance],
        };
        if !options.inputs.is_empty() {
            gallery.import(options.inputs, false, cx);
        }
        gallery.open_library(cx);
        if let Some(duration) = gallery.benchmark.as_ref().map(Benchmark::duration) {
            // Completion must not depend on another frame: occluded windows can stop drawing.
            cx.spawn_in(window, async move |this, cx| {
                smol::Timer::after(duration).await;
                let _ = this.update_in(cx, |view, window, cx| view.finish_benchmark(window, cx));
            })
            .detach();
        }
        #[cfg(feature = "visual-check")]
        capture_snapshots(window, cx);
        gallery
    }

    fn open_library(&mut self, cx: &mut Context<Self>) {
        let options = LibraryOptions::new(self.library_root.clone())
            .prepare_web_sizes(self.benchmark.is_none());
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let library = Library::open(options)?;
                    let photos: Vec<GalleryPhoto> = library
                        .photos()
                        .iter()
                        .map(|record| {
                            let grid = library.derivative_path(&record.id, DerivativeSize::Grid);
                            let thumbnail = if grid.is_file() {
                                Thumbnail::Ready(grid.into())
                            } else {
                                Thumbnail::Pending
                            };
                            GalleryPhoto::new(record.clone(), thumbnail)
                        })
                        .collect();
                    anyhow::Ok((library, photos))
                })
                .await;
            let _ = this.update(cx, |view, cx| view.library_opened(opened, cx));
            loop {
                let Ok(busy) = this.update(cx, Self::drain) else {
                    break;
                };
                smol::Timer::after(if busy { BUSY_POLL } else { IDLE_POLL }).await;
            }
        })
        .detach();
    }

    fn library_opened(
        &mut self,
        opened: anyhow::Result<(Library, Vec<GalleryPhoto>)>,
        cx: &mut Context<Self>,
    ) {
        match opened {
            Ok((library, photos)) => {
                self.failures = library.failures().to_vec();
                self.photos = photos;
                self.library = LibraryState::Open(library);
                // The next action is always the primary task: a photo is
                // selected on open, so the inspector carries content and the
                // first press of an arrow key moves somewhere. An app that opens
                // with nothing selected asks the user to do its work first.
                if self.selected.is_none() {
                    self.select_index(0, cx);
                }
                // The count lives on the sidebar's "All photos" row and the
                // weight in its footer, so the status line is free for
                // transient state. Opening the library is not a state worth
                // reporting: the grid is the proof.
                self.status = if self.photos.is_empty() {
                    "Originals stay where they are · everything is stored on this Mac".into()
                } else {
                    SharedString::from("")
                };
                for pending in std::mem::take(&mut self.pending_imports) {
                    self.import(pending.inputs, pending.owned, cx);
                }
            }
            Err(error) => {
                let message: SharedString = format!("{error:#}").into();
                self.status = format!("Couldn't open the library: {message}").into();
                self.library = LibraryState::Unavailable(message);
                for pending in std::mem::take(&mut self.pending_imports) {
                    if pending.owned {
                        photos::cleanup_import(&pending.inputs);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Applies queued library events. Returns whether work is outstanding.
    fn drain(&mut self, cx: &mut Context<Self>) -> bool {
        let LibraryState::Open(library) = &self.library else {
            return false;
        };
        let events: Vec<Event> = (0..EVENTS_PER_TICK)
            .map_while(|_| library.try_recv().ok())
            .collect();
        let received = !events.is_empty();
        for event in events {
            self.apply(event, cx);
        }
        if received || self.benchmark.is_some() {
            // Scripted scrolling needs a clock even after the import queue drains.
            cx.notify();
        }
        received
            || self.benchmark.is_some()
            || self.progress.is_some()
            || self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.image.is_none() && preview.error.is_none())
    }

    fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::ImportProgress { processed, total } => {
                self.progress = Some((processed, total));
            }
            Event::Imported {
                source,
                outcome,
                processed,
                total,
                ..
            } => {
                self.progress = Some((processed, total));
                self.apply_outcome(source, outcome, cx);
            }
            Event::ImportFinished { summary } => {
                self.progress = None;
                self.status = format::import_summary(&summary).into();
            }
            Event::Removed { id } => self.remove(&id),
            Event::Ready {
                id,
                size,
                path,
                cache_hit,
                ..
            } => match size {
                DerivativeSize::Grid => {
                    if let Some(photo) = self.photo_mut(&id) {
                        photo.thumbnail = Thumbnail::Ready(path.into());
                    }
                }
                DerivativeSize::Desktop => self.preview_ready(&id, path, cache_hit),
                DerivativeSize::Mobile => {}
            },
            Event::DerivativeFailed { id, size, message } => match size {
                DerivativeSize::Grid => {
                    if let Some(photo) = self.photo_mut(&id) {
                        photo.thumbnail = Thumbnail::Failed;
                    }
                }
                DerivativeSize::Desktop => self.preview_failed(&id, message),
                DerivativeSize::Mobile => {}
            },
            Event::MetadataUpdated { photo: record } => {
                if let Some(photo) = self.photo_mut(&record.id) {
                    photo.record = record;
                }
            }
            Event::FilmMetadataFinished => {}
            Event::FailuresCleared => self.failures.clear(),
        }
    }

    fn apply_outcome(&mut self, source: PathBuf, outcome: ImportOutcome, cx: &mut Context<Self>) {
        match outcome {
            ImportOutcome::Added(record) => {
                let LibraryState::Open(library) = &self.library else {
                    return;
                };
                let grid = library.derivative_path(&record.id, DerivativeSize::Grid);
                self.upsert(record, Thumbnail::Ready(grid.into()), cx);
            }
            ImportOutcome::Duplicate(record) | ImportOutcome::Relinked(record) => {
                self.upsert(record, Thumbnail::Pending, cx);
            }
            ImportOutcome::Unchanged(_) => {}
            ImportOutcome::Failed(message) => {
                self.session_failures += 1;
                self.failures.retain(|failure| failure.path != source);
                let index = self
                    .failures
                    .partition_point(|failure| failure.path < source);
                self.failures.insert(
                    index,
                    ImportFailure {
                        path: source,
                        message,
                        failed_at: unix_seconds(),
                    },
                );
                return;
            }
        }
        self.failures.retain(|failure| failure.path != source);
    }

    /// Inserts or updates a photo, keeping the grid ordered by original path.
    fn upsert(&mut self, record: PhotoRecord, thumbnail: Thumbnail, cx: &mut Context<Self>) {
        let existing = self
            .photos
            .iter()
            .position(|photo| photo.record.id == record.id)
            .map(|index| self.photos.remove(index));
        let photo = match existing {
            Some(mut photo) => {
                photo.record = record;
                photo
            }
            None => GalleryPhoto::new(record, thumbnail),
        };
        let index = self
            .photos
            .partition_point(|other| other.record.source < photo.record.source);
        self.photos.insert(index, photo);
        // A library that opens while its first import is still running has an
        // empty photo list at open time, so the select-on-open pass has nothing
        // to select and the app would end up with a stale selection: no ring on
        // any tile and an empty inspector. The first photo to arrive repairs it.
        if self.selected.is_none() && !self.photos.is_empty() {
            self.select_index(0, cx);
        }
    }

    fn remove(&mut self, id: &PhotoId) {
        self.photos.retain(|photo| &photo.record.id != id);
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
            self.preview = None;
            self.viewer_open = false;
        }
    }

    fn photo_mut(&mut self, id: &PhotoId) -> Option<&mut GalleryPhoto> {
        self.photos.iter_mut().find(|photo| &photo.record.id == id)
    }

    pub(super) fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.photos
            .iter()
            .position(|photo| &photo.record.id == selected)
    }

    pub(super) fn selected_photo(&self) -> Option<&GalleryPhoto> {
        self.selected_index().map(|index| &self.photos[index])
    }

    pub(super) fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(photo) = self.photos.get(index) else {
            return;
        };
        if self.selected.as_ref() == Some(&photo.record.id) {
            return;
        }
        self.selected = Some(photo.record.id.clone());
        self.request_preview();
        self.scroll
            .scroll_to_item(index / self.metrics.columns.max(1), ScrollStrategy::Nearest);
        cx.notify();
    }

    fn request_preview(&mut self) {
        let (LibraryState::Open(library), Some(id)) = (&self.library, &self.selected) else {
            return;
        };
        library.request_preview(id);
        self.preview = Some(Preview {
            id: id.clone(),
            image: None,
            error: None,
        });
        if let Some(benchmark) = &mut self.benchmark {
            benchmark.preview_requested();
        }
    }

    fn preview_ready(&mut self, id: &PhotoId, path: PathBuf, cache_hit: bool) {
        let index = self.selected_index();
        let Some(preview) = self.preview.as_mut().filter(|preview| &preview.id == id) else {
            return;
        };
        preview.image = Some(path.into());
        if let (Some(benchmark), Some(index)) = (&mut self.benchmark, index) {
            benchmark.preview_ready(index, cache_hit);
        }
    }

    fn preview_failed(&mut self, id: &PhotoId, message: String) {
        let Some(preview) = self.preview.as_mut().filter(|preview| &preview.id == id) else {
            return;
        };
        preview.error = Some(message.into());
        if let Some(benchmark) = &mut self.benchmark {
            benchmark.preview_failed();
        }
    }

    pub(super) fn move_selection(&mut self, movement: Movement, cx: &mut Context<Self>) {
        let GridMetrics { columns, page_rows } = self.metrics;
        if let Some(index) = grid::move_selection(
            self.selected_index(),
            self.photos.len(),
            columns,
            page_rows,
            movement,
        ) {
            self.select_index(index, cx);
        }
    }

    pub(super) fn open_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_none() {
            return;
        }
        self.viewer_open = true;
        window.focus(&self.viewer_focus, cx);
        cx.notify();
    }

    pub(super) fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewer_open = false;
        window.focus(&self.grid_focus, cx);
        cx.notify();
    }

    pub(super) fn is_importing(&self) -> bool {
        self.progress.is_some()
    }

    fn import(&mut self, inputs: Vec<PathBuf>, owned: bool, cx: &mut Context<Self>) {
        match &self.library {
            LibraryState::Open(library) => {
                if owned {
                    library.import_copies(inputs);
                } else {
                    library.import(inputs);
                }
                self.progress.get_or_insert((0, 0));
                self.status = "Importing…".into();
            }
            LibraryState::Opening => self.pending_imports.push(PendingImport { inputs, owned }),
            LibraryState::Unavailable(_) => {
                if owned {
                    photos::cleanup_import(&inputs);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn choose_files(&mut self, cx: &mut Context<Self>) {
        if self.picking {
            return;
        }
        self.picking = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = paths.await;
            let _ = this.update(cx, |view, cx| {
                view.picking = false;
                match result {
                    Ok(Ok(Some(paths))) => view.import(paths, false, cx),
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        view.status = format!("Couldn't choose files: {error:#}").into();
                    }
                    Err(error) => {
                        view.status = format!("Couldn't choose files: {error:#}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn choose_from_photos(&mut self, cx: &mut Context<Self>) {
        if self.picking {
            return;
        }
        let receiver = match photos::open(photos::import_root(&self.library_root)) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.status = format!("Couldn't open Photos: {error:#}").into();
                cx.notify();
                return;
            }
        };
        self.picking = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = smol::unblock(move || receiver.recv()).await;
            let copies = match &result {
                Ok(Ok(paths)) => paths.clone(),
                _ => Vec::new(),
            };
            let updated = this.update(cx, |view, cx| {
                view.picking = false;
                match result {
                    Ok(Ok(paths)) if paths.is_empty() => {}
                    Ok(Ok(paths)) => view.import(paths, true, cx),
                    Ok(Err(error)) => {
                        view.status = format!("Couldn't import from Photos: {error}").into();
                    }
                    // The picker closed without delivering a result.
                    Err(_) => {}
                }
                cx.notify();
            });
            if updated.is_err() {
                photos::cleanup_import(&copies);
            }
        })
        .detach();
    }

    pub(super) fn retry_failures(&mut self, cx: &mut Context<Self>) {
        let paths = self
            .failures
            .iter()
            .map(|failure| failure.path.clone())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            self.import(paths, false, cx);
        }
    }

    pub(super) fn clear_failures(&mut self, cx: &mut Context<Self>) {
        if let LibraryState::Open(library) = &self.library {
            library.clear_failures();
        }
        self.failures.clear();
        cx.notify();
    }

    pub(super) fn toggle_performance(&mut self, cx: &mut Context<Self>) {
        self.show_performance = !self.show_performance;
        cx.notify();
    }

    /// Runs the benchmark script for this frame and measures preview decode.
    pub(super) fn benchmark_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(benchmark) = &mut self.benchmark else {
            return;
        };
        benchmark.record_frame(window);
        if let Some(step) = benchmark.step(self.photos.len()) {
            self.scroll
                .scroll_to_item(step.scroll_row, ScrollStrategy::Top);
            if let Some(index) = step.preview_index
                && let Some(photo) = self.photos.get(index)
            {
                // Every scripted request counts, even when it repeats a selection.
                self.selected = Some(photo.record.id.clone());
                self.request_preview();
            }
            window.request_animation_frame();
        }
        let image = self
            .preview
            .as_ref()
            .and_then(|preview| preview.image.clone());
        if let (Some(image), true) = (
            image,
            self.benchmark
                .as_ref()
                .is_some_and(Benchmark::awaiting_decode),
        ) {
            let resource = gpui_kit::Resource::from(image.to_path_buf());
            if let Some(result) = self.preview_cache.update(cx, |cache, cx| {
                gpui_kit::ImageCache::load(cache, &resource, window, cx)
            }) && let Some(benchmark) = &mut self.benchmark
            {
                benchmark.preview_decoded(result.is_ok());
            }
        }
    }

    fn finish_benchmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(benchmark) = &self.benchmark else {
            return;
        };
        let totals = Totals {
            photos: self.photos.len(),
            thumbnails_completed: self
                .photos
                .iter()
                .filter(|photo| matches!(photo.thumbnail, Thumbnail::Ready(_)))
                .count(),
            failed: self.session_failures
                + self
                    .photos
                    .iter()
                    .filter(|photo| matches!(photo.thumbnail, Thumbnail::Failed))
                    .count(),
        };
        let report = benchmark.report(
            &totals,
            &self.thumbnails_cache,
            &self.preview_cache,
            window,
            cx,
        );
        println!("{report}");
        cx.quit();
    }
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(feature = "visual-check")]
fn capture_snapshots(window: &mut Window, cx: &mut Context<Gallery>) {
    let Some(directory) = std::env::var_os("PHOTO_SNAPSHOT_DIR") else {
        return;
    };
    // Capture only this app's Metal scene; no desktop/screen-recording API.
    cx.spawn_in(window, async move |this, cx| {
        let directory = PathBuf::from(directory);
        for frame in 1..=2 {
            smol::Timer::after(Duration::from_secs(4)).await;
            let _ = this.update_in(cx, |_, window, _| {
                let result = (|| -> anyhow::Result<()> {
                    std::fs::create_dir_all(&directory)?;
                    window
                        .render_to_image()?
                        .save(directory.join(format!("frame-{frame}.png")))?;
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("Scene capture failed: {error:#}");
                }
            });
        }
    })
    .detach();
}
