//! The desktop photo library window.

mod benchmark;
mod cache;
mod format;
mod gallery;
mod view;

pub use gallery::{Gallery, GalleryOptions};

use gpui_kit::{
    AppContext as _, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, component::Root, component::Theme, px, size,
};
use std::path::PathBuf;

const GRID_CONTEXT: &str = "PhotoGrid";
const VIEWER_CONTEXT: &str = "PhotoViewer";
const APP_NAME: &str = "Photo prototype";

gpui_kit::actions!(
    photo_library,
    [
        Import,
        ImportFromPhotos,
        ShowImportFailures,
        TogglePerformance,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectPageUp,
        SelectPageDown,
        SelectFirst,
        SelectLast,
        OpenViewer,
        CloseViewer,
        PreviousPhoto,
        NextPhoto,
        Quit,
    ]
);

/// Opens the library window and runs until the app quits. `inputs` are JPEG
/// files or folders to import on launch.
pub fn run(library_root: PathBuf, inputs: Vec<PathBuf>) {
    let benchmark_seconds = benchmark::seconds_from_env();
    let benchmark_mode = benchmark_seconds.is_some();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(menus());
            let mut window_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(820.)),
                    cx,
                ))),
                window_min_size: Some(size(px(560.), px(420.))),
                titlebar: Some(TitlebarOptions {
                    title: Some(APP_NAME.into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if benchmark_mode {
                benchmark::configure_window(&mut window_options);
            }
            let options = GalleryOptions {
                library_root,
                inputs,
                benchmark_seconds,
            };
            cx.open_window(window_options, |window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                let gallery = cx.new(|cx| Gallery::new(options, window, cx));
                cx.new(|cx| Root::new(gallery, window, cx))
            })
            .expect("open photo window");
            if !benchmark_mode {
                cx.activate(true);
            }
        });
}

fn bind_keys(cx: &mut gpui_kit::App) {
    let grid = Some(GRID_CONTEXT);
    let viewer = Some(VIEWER_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", Import, None),
        KeyBinding::new("cmd-shift-o", ImportFromPhotos, None),
        KeyBinding::new("cmd-alt-p", TogglePerformance, None),
        KeyBinding::new("left", SelectLeft, grid),
        KeyBinding::new("right", SelectRight, grid),
        KeyBinding::new("up", SelectUp, grid),
        KeyBinding::new("down", SelectDown, grid),
        KeyBinding::new("pageup", SelectPageUp, grid),
        KeyBinding::new("pagedown", SelectPageDown, grid),
        KeyBinding::new("home", SelectFirst, grid),
        KeyBinding::new("end", SelectLast, grid),
        KeyBinding::new("cmd-up", SelectFirst, grid),
        KeyBinding::new("cmd-down", SelectLast, grid),
        KeyBinding::new("space", OpenViewer, grid),
        KeyBinding::new("enter", OpenViewer, grid),
        KeyBinding::new("escape", CloseViewer, viewer),
        KeyBinding::new("space", CloseViewer, viewer),
        KeyBinding::new("left", PreviousPhoto, viewer),
        KeyBinding::new("right", NextPhoto, viewer),
    ]);
}

fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: APP_NAME.into(),
            items: vec![MenuItem::action(format!("Quit {APP_NAME}"), Quit)],
            disabled: false,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Import…", Import),
                MenuItem::action("Import from Photos…", ImportFromPhotos),
                MenuItem::separator(),
                MenuItem::action("Files not imported…", ShowImportFailures),
            ],
            disabled: false,
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Open photo", OpenViewer),
                MenuItem::separator(),
                MenuItem::action("Show or hide performance", TogglePerformance),
            ],
            disabled: false,
        },
    ]
}
