use gpui::{prelude::*, *};
use gpui_kit as gpui;
use photo_prototype::{Event, Library, Pipeline, PreviewKind};
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn benchmark_seconds_from_env() -> Option<u64> {
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

struct BoundedCache {
    inner: Entity<RetainAllImageCache>,
    recent: VecDeque<Resource>,
    capacity: usize,
    loading: HashSet<Resource>,
    concurrency: usize,
    failures: usize,
    peak_entries: usize,
    peak_loading: usize,
}

impl BoundedCache {
    fn new(capacity: usize, concurrency: usize, cx: &mut App) -> Entity<Self> {
        let inner = RetainAllImageCache::new(cx);
        cx.new(|_| Self {
            inner,
            recent: VecDeque::new(),
            capacity,
            loading: HashSet::new(),
            concurrency,
            failures: 0,
            peak_entries: 0,
            peak_loading: 0,
        })
    }
}

impl ImageCache for BoundedCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        // Settle off-screen loads too. Never evict a pending load: GPUI's shared
        // task survives removal and would otherwise escape our concurrency bound.
        for pending in self.loading.clone() {
            if let Some(result) = self
                .inner
                .update(cx, |cache, cx| cache.load(&pending, window, cx))
            {
                self.loading.remove(&pending);
                self.failures += usize::from(result.is_err());
            }
        }
        if let Some(index) = self.recent.iter().position(|r| r == resource) {
            self.recent.remove(index);
        } else {
            if self.loading.len() >= self.concurrency {
                return None;
            }
            if self.recent.len() == self.capacity {
                let Some(index) = self.recent.iter().position(|r| !self.loading.contains(r)) else {
                    return None;
                };
                let old = self.recent.remove(index).unwrap();
                self.inner
                    .update(cx, |cache, cx| cache.remove(&old, window, cx));
            }
        }
        self.recent.push_back(resource.clone());
        let result = self
            .inner
            .update(cx, |cache, cx| cache.load(resource, window, cx));
        if result.is_none() {
            self.loading.insert(resource.clone());
        }
        self.peak_entries = self.peak_entries.max(self.inner.read(cx).len());
        self.peak_loading = self.peak_loading.max(self.loading.len());
        result
    }
}

struct Gallery {
    library: Option<Library>,
    thumbnails: Vec<Option<PathBuf>>,
    selected: Option<usize>,
    large: Option<PathBuf>,
    status: String,
    scanning: bool,
    completed: usize,
    failed: usize,
    thumbnails_cache: Entity<BoundedCache>,
    preview_cache: Entity<BoundedCache>,
    scroll: UniformListScrollHandle,
    started: Instant,
    preview_requested: Option<Instant>,
    preview_started: Option<Instant>,
    preview_attempts: usize,
    preview_failures: usize,
    preview_cache_hits: usize,
    preview_decoded_ms: Vec<f64>,
    max_render_gap: Duration,
    max_render_gap_before_frame: usize,
    render_count: usize,
    first_frame_ms: Option<f64>,
    inactive_frames: usize,
    preview_latencies_ms: Vec<f64>,
    preview_samples: Vec<(usize, f64)>,
    benchmark_seconds: Option<u64>,
    next_preview_at: u64,
    show_fps: bool,
    last_render: Instant,
}

impl Gallery {
    fn new(
        inputs: Vec<PathBuf>,
        benchmark_seconds: Option<u64>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut gallery = Self {
            library: None,
            thumbnails: Vec::new(),
            selected: None,
            large: None,
            status: "Choose JPEGs or a folder to import".into(),
            scanning: false,
            completed: 0,
            failed: 0,
            thumbnails_cache: BoundedCache::new(64, 4, cx),
            preview_cache: BoundedCache::new(2, 1, cx),
            scroll: UniformListScrollHandle::default(),
            started: Instant::now(),
            preview_requested: None,
            preview_started: None,
            preview_attempts: 0,
            preview_failures: 0,
            preview_cache_hits: 0,
            preview_decoded_ms: Vec::new(),
            max_render_gap: Duration::ZERO,
            max_render_gap_before_frame: 0,
            render_count: 0,
            first_frame_ms: None,
            inactive_frames: 0,
            preview_latencies_ms: Vec::new(),
            preview_samples: Vec::new(),
            benchmark_seconds,
            next_preview_at: 1,
            show_fps: benchmark_seconds.is_none(),
            last_render: Instant::now(),
        };
        if let Some(seconds) = gallery.benchmark_seconds {
            // Completion must not depend on another frame: occluded windows can stop drawing.
            cx.spawn_in(window, async move |this, cx| {
                smol::Timer::after(Duration::from_secs(seconds)).await;
                let _ = this.update_in(cx, |view, window, cx| view.finish_benchmark(window, cx));
            })
            .detach();
        }
        #[cfg(feature = "visual-check")]
        if let Some(directory) = std::env::var_os("PHOTO_SNAPSHOT_DIR") {
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
        if !inputs.is_empty() {
            gallery.import(inputs, cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;
                let idle = this.update(cx, |view, cx| {
                    view.drain(cx);
                    if view.benchmark_seconds.is_some() {
                        // Scripted scrolling needs a clock even after the import queue drains.
                        cx.notify();
                        false
                    } else {
                        view.completed + view.failed == view.thumbnails.len()
                            && view.preview_requested.is_none()
                    }
                });
                match idle {
                    Err(_) => break,
                    Ok(true) => {
                        smol::Timer::after(Duration::from_millis(84)).await;
                    }
                    Ok(false) => {}
                }
            }
        })
        .detach();
        gallery
    }

    fn import(&mut self, inputs: Vec<PathBuf>, cx: &mut Context<Self>) {
        // One import session at a time: no overlapping workers or stale completion messages.
        if self.library.is_some() || self.scanning {
            return;
        }
        self.scanning = true;
        self.status = "Scanning…".into();
        let cache = std::env::var_os("PHOTO_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("HOME is required"))
                    .join("Library/Caches/dev.parth.photo-prototype/v1")
            });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Library::open(&inputs, cache, Pipeline::Vips) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.scanning = false;
                match result {
                    Ok(library) => {
                        view.thumbnails = vec![None; library.photos().len()];
                        view.library = Some(library);
                        view.status = format!("Importing {} JPEGs", view.thumbnails.len());
                    }
                    Err(error) => view.status = format!("Import failed: {error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn choose(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Import JPEGs".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |view, cx| view.import(paths, cx));
            }
        })
        .detach();
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(library) = &self.library else { return };
        if library.request_preview(index).is_ok() {
            self.selected = Some(index);
            self.large = None;
            self.preview_requested = Some(Instant::now());
            self.preview_started = self.preview_requested;
            self.preview_attempts += 1;
            cx.notify();
        }
    }

    fn drain(&mut self, cx: &mut Context<Self>) {
        let Some(library) = &self.library else { return };
        let mut changed = false;
        // Bound foreground work even when a warm cache completes very quickly.
        for _ in 0..16 {
            let Ok(event) = library.recv_timeout(Duration::ZERO) else {
                break;
            };
            changed = true;
            match event {
                Event::Ready {
                    index,
                    kind: PreviewKind::Thumbnail,
                    path,
                    ..
                } => {
                    self.thumbnails[index] = Some(path);
                    self.completed += 1;
                }
                Event::Ready {
                    index,
                    kind: PreviewKind::Large,
                    path,
                    cache_hit,
                    ..
                } if self.selected == Some(index) => {
                    self.large = Some(path);
                    self.preview_cache_hits += usize::from(cache_hit);
                    if let Some(start) = self.preview_requested.take() {
                        let latency = start.elapsed().as_secs_f64() * 1000.0;
                        self.preview_latencies_ms.push(latency);
                        self.preview_samples.push((index, latency));
                    }
                }
                Event::Failed {
                    index,
                    kind,
                    message,
                } => {
                    if kind == PreviewKind::Thumbnail {
                        self.failed += 1;
                    } else {
                        self.preview_failures += 1;
                        if self.selected == Some(index) {
                            self.preview_requested = None;
                            self.preview_started = None;
                        }
                    }
                    self.status = format!("Photo {}: {message}", index + 1);
                }
                _ => {}
            }
        }
        if changed {
            if self.completed == self.thumbnails.len() {
                self.status = "Import complete · originals unchanged · local only".into();
            }
            cx.notify();
        }
    }

    fn rows(
        &mut self,
        range: std::ops::Range<usize>,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        range
            .map(|row| {
                div().flex().w_full().h(px(180.)).gap_2().px_2().children(
                    (row * columns..((row + 1) * columns).min(self.thumbnails.len())).map(
                        |index| {
                            let tile = div()
                                .id(index)
                                .relative()
                                .overflow_hidden()
                                .flex_1()
                                .min_w_0()
                                .h(px(172.))
                                .bg(rgb(0x222222))
                                .cursor_pointer()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view.benchmark_seconds.is_none() {
                                        view.open(index, cx);
                                    }
                                }));
                            if let Some(path) = &self.thumbnails[index] {
                                tile.child(
                                    img(Arc::<std::path::Path>::from(path.as_path()))
                                        .absolute()
                                        .size_full()
                                        .object_fit(ObjectFit::Contain)
                                        .image_cache(&self.thumbnails_cache),
                                )
                                .into_any_element()
                            } else {
                                tile.flex()
                                    .items_center()
                                    .justify_center()
                                    .child(format!("{}", index + 1))
                                    .into_any_element()
                            }
                        },
                    ),
                )
            })
            .collect()
    }

    fn benchmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(seconds) = self.benchmark_seconds else {
            return;
        };
        let elapsed = self.started.elapsed();
        if elapsed.as_secs() >= seconds {
            return;
        }
        if !self.thumbnails.is_empty() {
            let rows = self.thumbnails.len().div_ceil(4);
            let row = ((elapsed.as_secs_f64() * 4.) as usize) % rows;
            self.scroll.scroll_to_item(row, ScrollStrategy::Top);
            if elapsed.as_secs() >= self.next_preview_at && self.next_preview_at + 2 <= seconds {
                let preview_row = (self.next_preview_at as usize * 4) % rows;
                self.open((preview_row * 4).min(self.thumbnails.len() - 1), cx);
                self.next_preview_at += 3;
            }
        }
        window.request_animation_frame();
    }

    fn finish_benchmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let elapsed = self.started.elapsed();
        let frames = window.frame_duration_snapshot();
        let mut latencies = self.preview_latencies_ms.clone();
        latencies.sort_by(f64::total_cmp);
        let mut decoded = self.preview_decoded_ms.clone();
        decoded.sort_by(f64::total_cmp);
        let thumbnails = self.thumbnails_cache.read(cx);
        let previews = self.preview_cache.read(cx);
        println!(
            "{}",
            serde_json::json!({
                "photos": self.thumbnails.len(), "thumbnails_completed": self.completed,
                "failed": self.failed, "seconds": elapsed.as_secs_f64(),
                "draw_samples": frames.draw_duration_histogram.len(),
                "last_render_age_ms": self.last_render.elapsed().as_secs_f64() * 1000.,
                "draw_p95_ms": frames.draw_duration_histogram.value_at_quantile(0.95) as f64 / 1e6,
                "present_interval_p95_ms": frames.present_interval_histogram.value_at_quantile(0.95) as f64 / 1e6,
                "present_interval_p99_ms": frames.present_interval_histogram.value_at_quantile(0.99) as f64 / 1e6,
                "max_render_gap_ms": self.max_render_gap.max(self.last_render.elapsed()).as_secs_f64() * 1000.,
                "max_render_gap_before_frame": self.max_render_gap_before_frame,
                "render_count": self.render_count,
                "first_frame_ms": self.first_frame_ms,
                "inactive_frames": self.inactive_frames,
                "preview_attempts": self.preview_attempts,
                "preview_failures": self.preview_failures,
                "preview_cache_hits": self.preview_cache_hits,
                "preview_decoded_samples": decoded.len(),
                "preview_decoded_p95_ms": decoded.get((decoded.len().saturating_sub(1) as f64 * 0.95).ceil() as usize),
                "decode_failures": thumbnails.failures + previews.failures,
                "preview_ready_samples": latencies.len(),
                "preview_samples": self.preview_samples,
                "preview_ready_p95_ms": latencies.get((latencies.len().saturating_sub(1) as f64 * 0.95).ceil() as usize),
                "thumbnail_cache_limit": 64, "large_cache_limit": 2,
                "thumbnail_cache_entries": thumbnails.inner.read(cx).len(),
                "preview_cache_entries": previews.inner.read(cx).len(),
                "thumbnail_cache_peak_entries": thumbnails.peak_entries,
                "preview_cache_peak_entries": previews.peak_entries,
                "thumbnail_peak_in_flight": thumbnails.peak_loading,
                "preview_peak_in_flight": previews.peak_loading
            })
        );
        cx.quit();
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count += 1;
        if self.render_count == 1 {
            self.first_frame_ms = Some(self.started.elapsed().as_secs_f64() * 1000.);
        } else if self.last_render.elapsed() > self.max_render_gap {
            self.max_render_gap = self.last_render.elapsed();
            self.max_render_gap_before_frame = self.render_count;
        }
        self.last_render = Instant::now();
        if self.started.elapsed().as_secs() > 1 && !window.is_window_active() {
            self.inactive_frames += 1;
        }
        self.benchmark(window, cx);
        if let (Some(start), Some(path)) = (self.preview_started, &self.large) {
            let resource = Resource::from(path.clone());
            if let Some(result) = self
                .preview_cache
                .update(cx, |cache, cx| cache.load(&resource, window, cx))
            {
                self.preview_started = None;
                if result.is_ok() {
                    self.preview_decoded_ms
                        .push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
        }
        let entity = cx.entity();
        let columns = 4;
        let preview = self
            .large
            .as_ref()
            .or_else(|| self.selected.and_then(|i| self.thumbnails[i].as_ref()));
        let mut viewer = div()
            .w(px(380.))
            .h_full()
            .bg(rgb(0x111111))
            .p_3()
            .flex()
            .flex_col()
            .gap_3();
        if let Some(index) = self.selected {
            viewer = viewer.child(format!("Photo {}", index + 1));
        }
        if let Some(path) = preview {
            viewer = viewer.child(
                div().relative().w_full().flex_1().min_h_0().child(
                    img(Arc::<std::path::Path>::from(path.as_path()))
                        .absolute()
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .image_cache(&self.preview_cache),
                ),
            );
        } else {
            viewer = viewer.child("Select a photo to preview");
        }
        div()
            .relative()
            .size_full()
            .bg(rgb(0x181818))
            .text_color(rgb(0xeeeeee))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(64.))
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .id("import")
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| view.choose(cx)))
                            .child("Import JPEGs…"),
                    )
                    .child(
                        div()
                            .id("fps-toggle")
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.show_fps = !view.show_fps;
                                cx.notify();
                            }))
                            .child(if self.show_fps {
                                "Hide performance"
                            } else {
                                "Show performance"
                            }),
                    )
                    .child(format!(
                        "{} / {} ready · {} failed",
                        self.completed,
                        self.thumbnails.len(),
                        self.failed
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        uniform_list(
                            "photos",
                            self.thumbnails.len().div_ceil(columns),
                            move |range, _, cx| {
                                entity.update(cx, |view, cx| view.rows(range, columns, cx))
                            },
                        )
                        .track_scroll(&self.scroll)
                        .flex_1()
                        .h_full(),
                    )
                    .child(viewer),
            )
            .child(div().h(px(36.)).px_4().text_sm().child(self.status.clone()))
            .when(self.show_fps, |view| {
                view.child(gpui_fps::fps_monitor(window, cx))
            })
    }
}

actions!(photos, [Quit]);

fn main() {
    let inputs = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let benchmark_seconds = benchmark_seconds_from_env();
    let benchmark_mode = benchmark_seconds.is_some();
    gpui_kit::application().run(move |cx| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "Photos".into(),
            items: vec![MenuItem::action("Quit", Quit)],
            disabled: false,
        }]);
        let mut window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1200.), px(820.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Photo prototype".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        if benchmark_mode {
            // GUI benchmarks must not activate or repeatedly raise the
            // window over the user's work. They also opt out of GPUI's
            // inactive-window throttle so focus is not a test prerequisite.
            window_options.focus = false;
            window_options.inactive_frame_interval = None;
        }
        cx.open_window(window_options, |window, cx| {
            cx.new(|cx| Gallery::new(inputs, benchmark_seconds, window, cx))
        })
        .expect("open photo window");
        if !benchmark_mode {
            cx.activate(true);
        }
    });
}
