//! The gallery's state: the library, its photos in grid order, the selection and its large
//! preview, what covers the window, and the decoded images the view draws.

mod events;
mod import;

use super::{
    benchmark::{self, Benchmark, Totals},
    demo,
    frames::FrameLog,
    images::ImageSlots,
};
use crate::{
    ImportFailure, Library, LibraryOptions, PhotoId, PhotoRecord,
    grid::{self, Movement},
};
use import::PendingImport;
use rune::{AppHandle, ImageCache, Tx, Waker};
use std::{
    cell::RefCell,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

const THUMBNAIL_CACHE_LIMIT: usize = 160;
const BENCHMARK_THUMBNAIL_CACHE_LIMIT: usize = 64;
const THUMBNAIL_DECODES: usize = 4;
const PREVIEW_CACHE_LIMIT: usize = 2;
const PREVIEW_DECODES: usize = 1;
const BUSY_POLL: Duration = Duration::from_millis(16);
const IDLE_POLL: Duration = Duration::from_millis(100);
const PERFORMANCE_REFRESH: Duration = Duration::from_millis(250);

/// Grid geometry. The grid is virtualised by row, so tiles have exact sizes
/// computed here rather than left to flex layout.
pub(super) const GRID_GAP: f32 = 6.0;
/// Padding on each side of the grid. Wider than the gap between tiles,
/// because the window edge is a separate group from the next tile.
pub(super) const GRID_GUTTER: f32 = 16.0;
/// The smallest tile the grid lays out, at the default zoom.
const DEFAULT_TILE_WIDTH: f32 = 168.0;
/// The zoom range, as the smallest width a column may take, and one Zoom In
/// or Zoom Out step.
pub(super) const TILE_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 112.0..=360.0;
const TILE_WIDTH_STEP: f32 = 32.0;
/// Square tiles: every photo gets the same weight in the grid, whatever its
/// orientation.
const TILE_ASPECT: f32 = 1.0;
pub(super) const INSPECTOR_WIDTH: f32 = 300.0;
const MIN_WIDTH_FOR_INSPECTOR: f32 = 760.0;
pub(super) const SIDEBAR_WIDTH: f32 = 232.0;
pub(super) const WINDOW_SIZE: (f32, f32) = (1200.0, 820.0);
/// Rows of thumbnails decoded beyond the visible ones, each side.
const OVERSCAN_ROWS: usize = 1;

pub(super) enum Thumbnail {
    Pending,
    Ready(Arc<Path>),
    Failed,
}

pub(super) struct GalleryPhoto {
    pub(super) record: PhotoRecord,
    pub(super) key: String,
    pub(super) thumbnail: Thumbnail,
}

impl GalleryPhoto {
    pub(super) fn new(record: PhotoRecord, thumbnail: Thumbnail) -> Self {
        Self {
            key: format!("photo-{}", record.id),
            record,
            thumbnail,
        }
    }
}

/// The large rendition of the selected photo.
pub(super) struct Preview {
    pub(super) id: PhotoId,
    pub(super) image: Option<Arc<Path>>,
    pub(super) error: Option<String>,
}

pub(super) enum LibraryState {
    /// Made-up photos for headless renders and tests. Nothing is read from
    /// or written to disk.
    Demo,
    Opening,
    Open(Library),
    Unavailable(String),
}

impl LibraryState {
    /// Whether the grid can show photos and take imports.
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open(_) | Self::Demo)
    }
}

/// What covers the library. One at a time: each is modal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Overlay {
    None,
    Viewer,
    Failures,
}

/// The performance panel's line, and when `poll` last read it from the
/// frame log.
pub(super) struct PerformanceLine {
    pub(super) text: String,
    read_at: Instant,
}

/// Where the grid sits and how its rows divide, from the last layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GridLayout {
    pub(super) columns: usize,
    pub(super) tile_width: f32,
    pub(super) tile_height: f32,
    pub(super) page_rows: usize,
    pub(super) show_sidebar: bool,
    pub(super) show_inspector: bool,
}

impl GridLayout {
    pub(super) fn row_height(&self) -> f32 {
        self.tile_height + GRID_GAP
    }
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
    pub(super) overlay: Overlay,
    /// The person hid the sidebar or the inspector from the View menu.
    pub(super) sidebar_hidden: bool,
    pub(super) inspector_hidden: bool,
    /// The grid's zoom: the smallest width a tile may take.
    pub(super) min_tile_width: f32,
    pub(super) progress: Option<(usize, usize)>,
    pub(super) status: String,
    pub(super) picking: bool,
    /// The performance panel's line while it shows.
    pub(super) performance: Option<PerformanceLine>,
    pub(super) frames: Rc<RefCell<FrameLog>>,
    pub(super) images: Option<ImageCache>,
    pub(super) thumbnails: ImageSlots,
    pub(super) previews: ImageSlots,
    visible_rows: Range<usize>,
    /// The window's and the grid viewport's sizes, from their last layout.
    pub(super) window_size: [f32; 2],
    pub(super) grid_size: Option<[f32; 2]>,
    pub(super) benchmark: Option<Benchmark>,
    /// How threads land work on the gallery. `None` for a demo gallery,
    /// which starts no threads.
    handle: Option<AppHandle<Self>>,
    pending_imports: Vec<PendingImport>,
    session_failures: usize,
}

impl Gallery {
    pub(super) fn new(
        options: GalleryOptions,
        frames: Rc<RefCell<FrameLog>>,
        handle: AppHandle<Self>,
    ) -> Self {
        let benchmark = options.benchmark_seconds.map(Benchmark::new);
        if benchmark.is_some() {
            frames.borrow_mut().start_recording();
        }
        let mut gallery = Self::blank(
            LibraryState::Opening,
            options.library_root,
            frames,
            benchmark,
        );
        gallery.handle = Some(handle);
        if !options.inputs.is_empty() {
            gallery.import(options.inputs, false);
        }
        gallery
    }

    /// The gallery headless renders and tests start from: made-up photos,
    /// the first one selected, and no library on disk.
    pub(super) fn demo() -> Self {
        let mut gallery = Self::blank(LibraryState::Demo, PathBuf::new(), Rc::default(), None);
        gallery.photos = demo::photos();
        gallery.selected = gallery.photos.first().map(|photo| photo.record.id.clone());
        gallery.request_preview();
        gallery
    }

    fn blank(
        library: LibraryState,
        library_root: PathBuf,
        frames: Rc<RefCell<FrameLog>>,
        benchmark: Option<Benchmark>,
    ) -> Self {
        let thumbnail_limit = if benchmark.is_some() {
            BENCHMARK_THUMBNAIL_CACHE_LIMIT
        } else {
            THUMBNAIL_CACHE_LIMIT
        };
        Self {
            library_root,
            library,
            photos: Vec::new(),
            failures: Vec::new(),
            selected: None,
            preview: None,
            overlay: Overlay::None,
            sidebar_hidden: false,
            inspector_hidden: false,
            min_tile_width: DEFAULT_TILE_WIDTH,
            progress: None,
            status: String::new(),
            picking: false,
            performance: None,
            frames,
            images: None,
            thumbnails: ImageSlots::new(thumbnail_limit, THUMBNAIL_DECODES),
            previews: ImageSlots::new(PREVIEW_CACHE_LIMIT, PREVIEW_DECODES),
            visible_rows: 0..0,
            window_size: [WINDOW_SIZE.0, WINDOW_SIZE.1],
            grid_size: None,
            benchmark,
            handle: None,
            pending_imports: Vec::new(),
            session_failures: 0,
        }
    }

    /// Runs once, before the first frame, with the waker image decodes use.
    pub(super) fn setup(&mut self, waker: Waker) {
        self.images = Some(ImageCache::new(waker));
        let Some(handle) = self.handle.clone() else {
            demo::decode(self);
            return;
        };
        let options = LibraryOptions::new(self.library_root.clone())
            .prepare_web_sizes(self.benchmark.is_none());
        std::thread::spawn(move || {
            let opened = events::open_library(options);
            handle.update(move |gallery: &mut Self, tx| gallery.library_opened(opened, tx));
        });
    }

    /// Runs before every frame and on every wake. Returns whether anything the
    /// view shows changed.
    pub(super) fn poll(&mut self, tx: &mut Tx) -> bool {
        let busy = self.drain(tx);
        let mut changed = busy.received;
        changed |= self.settle_images();
        changed |= self.step_benchmark(tx);
        if let Some(line) = &self.performance {
            if line.read_at.elapsed() >= PERFORMANCE_REFRESH {
                changed |= self.refresh_performance();
            }
            tx.wake_after(PERFORMANCE_REFRESH);
        }
        tx.wake_after(if busy.outstanding {
            BUSY_POLL
        } else {
            IDLE_POLL
        });
        changed
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

    pub(super) fn select_index(&mut self, index: usize, tx: &mut Tx) {
        let Some(photo) = self.photos.get(index) else {
            return;
        };
        if self.selected.as_ref() == Some(&photo.record.id) {
            return;
        }
        self.selected = Some(photo.record.id.clone());
        self.request_preview();
        tx.scroll_into_view(row_key(&self.photos, index, self.layout().columns));
    }

    pub(super) fn select_id(&mut self, id: &PhotoId, tx: &mut Tx) {
        if let Some(index) = self.photos.iter().position(|photo| &photo.record.id == id) {
            self.select_index(index, tx);
        }
    }

    pub(super) fn request_preview(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let image = match &self.library {
            LibraryState::Open(library) => {
                library.request_preview(&id);
                None
            }
            LibraryState::Demo => Some(demo::preview_path(&id)),
            LibraryState::Opening | LibraryState::Unavailable(_) => return,
        };
        self.preview = Some(Preview {
            id,
            image,
            error: None,
        });
        if let Some(benchmark) = &mut self.benchmark {
            benchmark.preview_requested();
        }
        if matches!(self.library, LibraryState::Demo) {
            demo::decode(self);
        }
    }

    fn preview_ready(&mut self, id: &PhotoId, path: PathBuf, cache_hit: bool) {
        let index = self.selected_index();
        let Some(preview) = self.preview.as_mut().filter(|preview| &preview.id == id) else {
            return;
        };
        let path: Arc<Path> = path.into();
        preview.image = Some(path.clone());
        if let (Some(benchmark), Some(index)) = (&mut self.benchmark, index) {
            benchmark.preview_ready(index, cache_hit);
        }
        if let Some(cache) = &self.images {
            self.previews.request(cache, &path);
        }
        // A repeated preview is already decoded, so it never settles again.
        if self.previews.pixels(&path).is_some()
            && let Some(benchmark) = &mut self.benchmark
            && benchmark.awaiting_decode()
        {
            benchmark.preview_decoded(true);
        }
    }

    fn preview_failed(&mut self, id: &PhotoId, message: String) {
        let Some(preview) = self.preview.as_mut().filter(|preview| &preview.id == id) else {
            return;
        };
        preview.error = Some(message);
        if let Some(benchmark) = &mut self.benchmark {
            benchmark.preview_failed();
        }
    }

    pub(super) fn move_selection(&mut self, movement: Movement, tx: &mut Tx) {
        let GridLayout {
            columns, page_rows, ..
        } = self.layout();
        if let Some(index) = grid::move_selection(
            self.selected_index(),
            self.photos.len(),
            columns,
            page_rows,
            movement,
        ) {
            self.select_index(index, tx);
        }
    }

    pub(super) fn open_viewer(&mut self) {
        if self.selected.is_some() && self.benchmark.is_none() {
            self.overlay = Overlay::Viewer;
        }
    }

    pub(super) fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    pub(super) fn open_failures(&mut self) {
        self.overlay = Overlay::Failures;
    }

    pub(super) fn is_importing(&self) -> bool {
        self.progress.is_some()
    }

    pub(super) fn toggle_sidebar(&mut self) {
        self.sidebar_hidden = !self.sidebar_hidden;
        self.layout_changed();
    }

    pub(super) fn toggle_inspector(&mut self) {
        self.inspector_hidden = !self.inspector_hidden;
        self.layout_changed();
    }

    /// One Zoom In (`steps` 1) or Zoom Out (-1) step, or several.
    pub(super) fn zoom(&mut self, steps: i32) {
        #[allow(clippy::cast_precision_loss)]
        self.set_zoom(self.min_tile_width + steps as f32 * TILE_WIDTH_STEP);
    }

    pub(super) fn set_zoom(&mut self, min_tile_width: f32) {
        let width = min_tile_width.clamp(*TILE_WIDTH_RANGE.start(), *TILE_WIDTH_RANGE.end());
        if width != self.min_tile_width {
            self.min_tile_width = width;
            self.layout_changed();
        }
    }

    /// The grid's viewport is measured again after the change, and the menu
    /// bar names what each View toggle will do next.
    fn layout_changed(&mut self) {
        self.grid_size = None;
        if let Some(handle) = &self.handle {
            let _ = handle.set_menu(super::menus(self.sidebar_hidden, self.inspector_hidden));
        }
    }

    /// Shows the selected photo's original in Finder.
    pub(super) fn reveal_selected(&self) {
        if self.handle.is_none() {
            return;
        }
        if let Some(photo) = self.selected_photo() {
            let source = photo.record.source.clone();
            std::thread::spawn(move || {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(source)
                    .status();
            });
        }
    }

    pub(super) fn toggle_performance(&mut self) {
        if self.performance.take().is_none() {
            self.refresh_performance();
        }
    }

    /// Rereads the frame log into the panel's line. Returns whether it changed.
    fn refresh_performance(&mut self) -> bool {
        let read_at = Instant::now();
        let text = self.frames.borrow().last_second(read_at).map_or_else(
            || "Idle: no frames in the last second".to_owned(),
            |(frames, slowest)| format!("{frames} frames/s · slowest {slowest:.1} ms CPU"),
        );
        let changed = self
            .performance
            .as_ref()
            .is_none_or(|line| line.text != text);
        self.performance = Some(PerformanceLine { text, read_at });
        changed
    }

    /// The grid's geometry for the current window. The benchmark's scripted
    /// scroll assumes a fixed column count and hides the sidebar, so its
    /// geometry stays what `benchmark.py` measured.
    pub(super) fn layout(&self) -> GridLayout {
        let benchmarking = self.benchmark.is_some();
        let show_sidebar = !benchmarking && !self.sidebar_hidden;
        let show_inspector = benchmarking
            || (!self.inspector_hidden
                && !self.photos.is_empty()
                && self.window_size[0] >= MIN_WIDTH_FOR_INSPECTOR);
        let [grid_width, grid_height] = self.grid_size.unwrap_or_else(|| {
            let sidebar = if show_sidebar { SIDEBAR_WIDTH } else { 0.0 };
            let inspector = if show_inspector { INSPECTOR_WIDTH } else { 0.0 };
            [
                self.window_size[0] - sidebar - inspector,
                self.window_size[1],
            ]
        });
        let min_tile = self.min_tile_width;
        let usable = (grid_width - GRID_GUTTER * 2.0).max(min_tile);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = if benchmarking {
            benchmark::COLUMNS
        } else {
            (((usable + GRID_GAP) / (min_tile + GRID_GAP)).floor() as usize).max(1)
        };
        #[allow(clippy::cast_precision_loss)]
        let tile_width = ((usable - GRID_GAP * (columns - 1) as f32) / columns as f32).floor();
        let tile_height = (tile_width * TILE_ASPECT).floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let page_rows = (grid_height / (tile_height + GRID_GAP)).floor() as usize;
        GridLayout {
            columns,
            tile_width,
            tile_height,
            page_rows: page_rows.saturating_sub(1).max(1),
            show_sidebar,
            show_inspector,
        }
    }

    pub(super) fn set_visible_rows(&mut self, rows: Range<usize>) {
        self.visible_rows = rows;
        self.request_visible();
    }

    /// Starts decodes for the visible rows' thumbnails and the preview.
    fn request_visible(&mut self) {
        let Some(cache) = &self.images else {
            return;
        };
        let columns = self.layout().columns;
        let start = self.visible_rows.start.saturating_sub(OVERSCAN_ROWS) * columns;
        let end = ((self.visible_rows.end + OVERSCAN_ROWS) * columns).min(self.photos.len());
        for photo in self.photos.get(start..end).unwrap_or_default() {
            if let Thumbnail::Ready(path) = &photo.thumbnail
                && !self.thumbnails.request(cache, path)
            {
                break;
            }
        }
        if let Some(path) = self
            .preview
            .as_ref()
            .and_then(|preview| preview.image.clone())
        {
            self.previews.request(cache, &path);
        }
    }

    /// Settles finished decodes. Returns whether any finished.
    fn settle_images(&mut self) -> bool {
        let thumbnails = self.thumbnails.settle();
        let previews = self.previews.settle();
        let current = self
            .preview
            .as_ref()
            .and_then(|preview| preview.image.clone());
        for (path, decoded) in &previews {
            if current.as_ref() == Some(path)
                && let Some(benchmark) = &mut self.benchmark
                && benchmark.awaiting_decode()
            {
                benchmark.preview_decoded(*decoded);
            }
        }
        let settled = !thumbnails.is_empty() || !previews.is_empty();
        if settled {
            // A finished decode frees a slot for a tile that had to wait.
            self.request_visible();
        }
        settled
    }

    /// Advances the benchmark script. Returns whether it moved anything.
    fn step_benchmark(&mut self, tx: &mut Tx) -> bool {
        if self.benchmark.is_none() {
            return false;
        }
        let row_height = self.layout().row_height();
        let photos = self.photos.len();
        let Some(benchmark) = &mut self.benchmark else {
            return false;
        };
        benchmark.record_poll();
        if let Some(remaining) = benchmark.remaining() {
            // Completion must not depend on another frame: a covered window
            // stops drawing, but its wakes still run.
            tx.wake_after(remaining);
        } else {
            self.finish_benchmark(tx);
            return false;
        }
        let Some(benchmark) = &mut self.benchmark else {
            return false;
        };
        let Some(step) = benchmark.step(photos) else {
            return false;
        };
        #[allow(clippy::cast_precision_loss)]
        tx.scroll_to_y(super::view::GRID_KEY, step.scroll_row as f32 * row_height);
        if let Some(index) = step.preview_index
            && let Some(photo) = self.photos.get(index)
        {
            // Every scripted request counts, even when it repeats a selection.
            self.selected = Some(photo.record.id.clone());
            self.request_preview();
        }
        // Sustained frame demand: ask for the next frame now.
        tx.wake_after(Duration::ZERO);
        true
    }

    fn finish_benchmark(&mut self, tx: &mut Tx) {
        let Some(benchmark) = self.benchmark.take() else {
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
        let frames = self.frames.borrow_mut().stop_recording();
        let report = benchmark.report(&totals, &frames, &self.thumbnails, &self.previews);
        println!("{report}");
        tx.quit();
    }
}

/// A grid row's key: its first photo's, so a row keeps its key while photos
/// arrive after it.
pub(super) fn row_key(photos: &[GalleryPhoto], index: usize, columns: usize) -> String {
    let first = index - index % columns.max(1);
    photos.get(first).map_or_else(
        || format!("row-{first}"),
        |photo| format!("row-{}", photo.key),
    )
}
