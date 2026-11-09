//! The desktop photo library window.

mod benchmark;
mod cache;
mod components;
pub mod design;
mod format;
mod gallery;
mod layout;
mod regions;
mod view;

pub use gallery::{Gallery, GalleryOptions};

use design::{ActiveDesign as _, glass};
use gpui::{
    App, AppContext as _, Bounds, KeyBinding, Menu, MenuItem, Styled as _, SystemMenuType,
    WindowBounds, WindowOptions, px, size, transparent_black,
};
use gpui_component::{Root, Theme, TitleBar};
use std::path::PathBuf;

const GRID_CONTEXT: &str = "PhotoGrid";
const VIEWER_CONTEXT: &str = "PhotoViewer";
/// The app's name in the menu bar.
pub(super) const APP_NAME: &str = "Photo prototype";

gpui::actions!(
    photo_library,
    [
        Import,
        ImportFromPhotos,
        ShowImportFailures,
        TogglePerformance,
        ToggleSidebar,
        ToggleInspector,
        ZoomIn,
        ZoomOut,
        RevealInFinder,
        Hide,
        HideOthers,
        ShowAll,
        Minimize,
        Zoom,
        ToggleFullScreen,
        CloseWindow,
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
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            design::theme::install(cx);
            // The material is read imperatively at paint time, so the surface
            // preference has to be resolved before the first paint.
            design::init(cx);
            design::motion::sync_reduce_motion(cx);
            bind_keys(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.on_action(|_: &Hide, cx| cx.hide());
            cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
            cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
            cx.set_menus(menus(MenuState::default()));
            let mut window_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(820.)),
                    cx,
                ))),
                window_min_size: Some(size(px(560.), px(420.))),
                // One chrome band, not two. A native titlebar above the toolbar
                // stacks two surfaces of the same material, which is what made
                // the top of the window read as a flat slab. The library's
                // `TitleBar` owns the strip instead: it hides the system
                // titlebar, places the traffic lights itself, and moves the
                // window itself.
                ..TitleBar::window_options()
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
                reapply_window_background(window, cx);
                let gallery = cx.new(|cx| Gallery::new(options, window, cx));
                cx.new(|cx| {
                    // `Root` paints the theme background across the whole
                    // window, which would cover the blurred desktop and turn
                    // window glass into an invisible feature. The design system
                    // owns the planes; the chrome carries the glass, the root
                    // must not.
                    Root::new(gallery, window, cx).bg(transparent_black())
                })
            })
            .expect("open photo window");
            if !benchmark_mode {
                cx.activate(true);
            }
        });
}

/// Re-apply the window's background material.
///
/// This must run again after every appearance change, never once at window
/// creation only: gpui's macOS backend tears the visual-effect view out of the
/// hierarchy whenever the value is anything but `Blurred`, so a window whose
/// material was only ever set once goes permanently opaque the first time the
/// system switches between light and dark.
fn reapply_window_background(window: &mut gpui::Window, cx: &mut App) {
    let design = cx.design();
    let material = glass::window_background(design);
    if std::env::var_os("PHOTO_GLASS_DEBUG").is_some() {
        eprintln!(
            "glass: material={material:?} tint_alpha={:.3}",
            design.glass().a
        );
    }
    window.set_background_appearance(material);
}

fn bind_keys(cx: &mut gpui::App) {
    let grid = Some(GRID_CONTEXT);
    let viewer = Some(VIEWER_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", Import, None),
        KeyBinding::new("cmd-shift-o", ImportFromPhotos, None),
        KeyBinding::new("cmd-alt-p", TogglePerformance, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-ctrl-f", ToggleFullScreen, None),
        KeyBinding::new("cmd-ctrl-s", ToggleSidebar, None),
        KeyBinding::new("cmd-i", ToggleInspector, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-alt-r", RevealInFinder, grid),
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

/// What the menu bar's toggles currently say. macOS menus name the action
/// a toggle will take ("Hide Sidebar"), so the bar is rebuilt when one flips.
#[derive(Clone, Copy)]
pub(super) struct MenuState {
    pub(super) sidebar_visible: bool,
    pub(super) inspector_visible: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            inspector_visible: true,
        }
    }
}

/// The menu bar, in the order and with the names the macOS Human Interface
/// Guidelines give: the app menu, File, View, Window.
pub(super) fn menus(state: MenuState) -> Vec<Menu> {
    let show_hide = |visible: bool, noun: &str| {
        if visible {
            format!("Hide {noun}")
        } else {
            format!("Show {noun}")
        }
    };
    vec![
        Menu {
            name: APP_NAME.into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(format!("Hide {APP_NAME}"), Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action(format!("Quit {APP_NAME}"), Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Import…", Import),
                MenuItem::action("Import from Photos…", ImportFromPhotos),
                MenuItem::separator(),
                MenuItem::action("Show in Finder", RevealInFinder),
                MenuItem::action("Files Not Imported…", ShowImportFailures),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
            disabled: false,
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action(show_hide(state.sidebar_visible, "Sidebar"), ToggleSidebar),
                MenuItem::action(show_hide(state.inspector_visible, "Info"), ToggleInspector),
                MenuItem::separator(),
                MenuItem::action("Open Photo", OpenViewer),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::separator(),
                MenuItem::action("Show or Hide Performance", TogglePerformance),
                MenuItem::separator(),
                MenuItem::action("Enter Full Screen", ToggleFullScreen),
            ],
            disabled: false,
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
            ],
            disabled: false,
        },
    ]
}
