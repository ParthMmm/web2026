//! The desktop photo library window.

mod benchmark;
mod demo;
mod format;
mod frames;
mod gallery;
mod icons;
mod images;
mod look;
mod open_panel;
#[cfg(test)]
mod tests;
mod view;

use gallery::{Gallery, GalleryOptions};

use crate::grid::Movement;
use frames::FrameLog;
use gallery::Overlay;
use rune::{AppSpec, Chrome, Key, KeyEvent, MacWindowOptions, MenuItem, Tx, WindowOptions};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// The window's object, shown in the toolbar.
pub(super) const APP_NAME: &str = "Photo prototype";
const MIN_WINDOW_SIZE: (f32, f32) = (560.0, 420.0);

/// JPEG files or folders named on the command line, after Rune's own flags.
pub fn inputs() -> Vec<PathBuf> {
    rune::extra_args().iter().map(PathBuf::from).collect()
}

/// Opens the library window and runs until the app quits. `inputs` are JPEG
/// files or folders to import on launch.
pub fn run(library_root: PathBuf, inputs: Vec<PathBuf>) {
    rune::prewarm_gpu();
    // Any flag but `--native` asks for a headless run (a render, a tree, a
    // script). It shows the demo gallery, so no render holds real photos.
    let headless = std::env::args()
        .skip(1)
        .any(|arg| arg.starts_with("--") && arg != "--native");
    let benchmark_seconds = benchmark::seconds_from_env().filter(|_| !headless);
    let benchmarking = benchmark_seconds.is_some();
    let frames = Rc::new(RefCell::new(FrameLog::default()));
    let spec = AppSpec {
        min_size: Some(MIN_WINDOW_SIZE),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: demo::SCENES,
        fonts: look::FONTS,
        families: look::FAMILIES,
        ..AppSpec::new(APP_NAME, gallery::WINDOW_SIZE, Gallery::demo, view::view)
    };
    let app = rune::App::new(spec);
    let handle = app.handle();
    let recorder = frames.clone();
    let options = GalleryOptions {
        library_root,
        inputs,
        benchmark_seconds,
    };
    let mut app = app
        .init(move || {
            if headless {
                Gallery::demo()
            } else {
                Gallery::new(options, frames, handle)
            }
        })
        // One chrome band: the title bar spans the window, and the traffic
        // lights sit on its centre line.
        .chrome(Chrome {
            full_size_content: true,
            traffic_lights: Some((16.0, (view::TITLE_BAR_HEIGHT - 14.0) / 2.0)),
            drag_regions: true,
        })
        .on_frame_encoded(move |frame| recorder.borrow_mut().record(frame));
    for item in menus(MenuState::default()) {
        app = app.menu_item(item);
    }
    if benchmarking {
        // A non-activating panel at the pop-up level stays visible to the
        // compositor without taking focus, so a covered window doesn't stop
        // drawing mid-run.
        app = app
            .open_inactive()
            .window_options(WindowOptions {
                always_on_top: true,
                ..WindowOptions::default()
            })
            .macos_window(
                MacWindowOptions::default()
                    .nonactivating_panel(true)
                    .level(benchmark::WINDOW_LEVEL),
            );
    }
    app.run();
}

fn setup(gallery: &mut Gallery, waker: rune::Waker) {
    gallery.setup(waker);
}

fn poll(gallery: &mut Gallery, _clock: f64, tx: &mut Tx) -> bool {
    gallery.poll(tx)
}

/// What the menu bar shows that changes: what each View toggle will do, and which photo
/// actions apply to the selection. The gallery rebuilds the bar when it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MenuState {
    pub(super) sidebar_hidden: bool,
    pub(super) inspector_hidden: bool,
    /// The selection is a draft, or a publish that stopped and can be retried.
    pub(super) publish: bool,
    /// The selection is published, or an unpublish that stopped and can be retried.
    pub(super) unpublish: bool,
    /// The selection is queued or uploading.
    pub(super) cancel: bool,
    /// Any photo is queued or uploading.
    pub(super) cancel_all: bool,
}

/// The menu bar's own items, in the order the macOS Human Interface Guidelines
/// give. A View toggle names what it will do, and a photo action is on only
/// when it applies, so the bar is rebuilt when either changes.
pub(super) fn menus(state: MenuState) -> Vec<MenuItem<Gallery>> {
    let MenuState {
        sidebar_hidden,
        inspector_hidden,
        ..
    } = state;
    let shortcut = |chord: &str| chord.parse().expect("menu shortcut parses");
    let show_hide = |hidden: bool, noun: &str| {
        if hidden {
            format!("Show {noun}")
        } else {
            format!("Hide {noun}")
        }
    };
    vec![
        MenuItem::new("Import…", |gallery: &mut Gallery, _| {
            gallery.choose_files()
        })
        .id("import")
        .menu("File")
        .key_equivalent(shortcut("cmd+o")),
        MenuItem::new("Import from Photos…", |gallery: &mut Gallery, _| {
            gallery.choose_from_photos();
        })
        .id("import-photos")
        .menu("File")
        .key_equivalent(shortcut("cmd+shift+o")),
        MenuItem::separator().menu("File"),
        MenuItem::new("Show in Finder", |gallery: &mut Gallery, _| {
            gallery.reveal_selected();
        })
        .id("reveal")
        .menu("File")
        .key_equivalent(shortcut("cmd+alt+r")),
        MenuItem::new("Files Not Imported…", |gallery: &mut Gallery, _| {
            gallery.open_failures();
        })
        .id("failures")
        .menu("File"),
        MenuItem::separator().menu("File"),
        MenuItem::new("Upload Photo", |gallery: &mut Gallery, _| {
            gallery.upload_selected();
        })
        .id("upload")
        .menu("File")
        .key_equivalent(shortcut("cmd+u")),
        MenuItem::new("Upload All Photos", |gallery: &mut Gallery, _| {
            gallery.upload_all();
        })
        .id("upload-all")
        .menu("File")
        .key_equivalent(shortcut("cmd+shift+u")),
        MenuItem::new("Cancel Upload", |gallery: &mut Gallery, _| {
            gallery.cancel_selected();
        })
        .id("cancel-upload")
        .menu("File")
        .enabled(state.cancel),
        MenuItem::new("Cancel All Uploads", |gallery: &mut Gallery, _| {
            gallery.cancel_all();
        })
        .id("cancel-all-uploads")
        .menu("File")
        .enabled(state.cancel_all),
        MenuItem::separator().menu("File"),
        MenuItem::new("Publish Photo", |gallery: &mut Gallery, _| {
            gallery.publish_selected();
        })
        .id("publish")
        .menu("File")
        .key_equivalent(shortcut("cmd+p"))
        .enabled(state.publish),
        MenuItem::new("Unpublish Photo", |gallery: &mut Gallery, _| {
            gallery.unpublish_selected();
        })
        .id("unpublish")
        .menu("File")
        .key_equivalent(shortcut("cmd+shift+p"))
        .enabled(state.unpublish),
        MenuItem::new("Cloud Problems…", |gallery: &mut Gallery, _| {
            gallery.open_cloud_failures();
        })
        .id("cloud-failures")
        .menu("File"),
        MenuItem::new(
            show_hide(sidebar_hidden, "Sidebar"),
            |gallery: &mut Gallery, _| {
                gallery.toggle_sidebar();
            },
        )
        .id("toggle-sidebar")
        .menu("View")
        .key_equivalent(shortcut("ctrl+cmd+s")),
        MenuItem::new(
            show_hide(inspector_hidden, "Info"),
            |gallery: &mut Gallery, _| {
                gallery.toggle_inspector();
            },
        )
        .id("toggle-info")
        .menu("View")
        .key_equivalent(shortcut("cmd+i")),
        MenuItem::separator().menu("View"),
        MenuItem::new("Open Photo", |gallery: &mut Gallery, _| {
            gallery.open_viewer()
        })
        .id("open-photo")
        .menu("View"),
        MenuItem::new("Zoom In", |gallery: &mut Gallery, _| gallery.zoom(1))
            .id("zoom-in")
            .menu("View")
            .key_equivalent(shortcut("cmd+equal")),
        MenuItem::new("Zoom Out", |gallery: &mut Gallery, _| gallery.zoom(-1))
            .id("zoom-out")
            .menu("View")
            .key_equivalent(shortcut("cmd+minus")),
        MenuItem::separator().menu("View"),
        MenuItem::new("Show or Hide Performance", |gallery: &mut Gallery, _| {
            gallery.toggle_performance();
        })
        .id("performance")
        .menu("View")
        .key_equivalent(shortcut("cmd+alt+p")),
    ]
}

fn on_key(gallery: &mut Gallery, event: &KeyEvent, tx: &mut Tx) -> bool {
    if rune_kit::shell::is_sidebar_toggle(event) {
        gallery.toggle_sidebar();
        return true;
    }
    let space = matches!(&event.key, Key::Text(typed) if typed == " ") && !event.mods.cmd;
    match gallery.overlay {
        Overlay::Viewer => {
            match event.key {
                Key::Escape => gallery.close_overlay(),
                Key::Left => gallery.move_selection(Movement::Left, tx),
                Key::Right => gallery.move_selection(Movement::Right, tx),
                _ if space => gallery.close_overlay(),
                _ => return false,
            }
            true
        }
        Overlay::Failures | Overlay::CloudFailures => false,
        Overlay::None if gallery.benchmark.is_some() => false,
        Overlay::None => {
            if space || event.key == Key::Enter {
                gallery.open_viewer();
                return true;
            }
            let Some(movement) = grid_movement(event) else {
                return false;
            };
            gallery.move_selection(movement, tx);
            true
        }
    }
}

fn grid_movement(event: &KeyEvent) -> Option<Movement> {
    Some(match (&event.key, event.mods.cmd) {
        (Key::Left, false) => Movement::Left,
        (Key::Right, false) => Movement::Right,
        (Key::Up, false) => Movement::Up,
        (Key::Down, false) => Movement::Down,
        (Key::Up, true) | (Key::Home, _) => Movement::First,
        (Key::Down, true) | (Key::End, _) => Movement::Last,
        (Key::PageUp, _) => Movement::PageUp,
        (Key::PageDown, _) => Movement::PageDown,
        _ => return None,
    })
}
