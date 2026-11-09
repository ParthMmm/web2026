//! The window: the sidebar, the toolbar, the grid with its inspector, the status bar, and
//! what covers them (the viewer, the failures sheet, the performance panel). Each part draws
//! the gallery's state and nothing else.

mod grid;
mod inspector;
mod overlays;

use super::{
    APP_NAME, format,
    gallery::{Gallery, Overlay},
    icons,
};
use grid::grid_region;
use inspector::inspector;
use overlays::{failures, viewer};
use rune::{Cx, El, PopupItem, PopupMenu, Tx, col, el, icon, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, Progress,
    shell::{Sidebar, SidebarRow, SidebarSection, SplitView, Toolbar},
};

pub(super) const GRID_KEY: &str = "photo-grid";
const STATUS_BAR_HEIGHT: f32 = 24.0;
const PROGRESS_WIDTH: f32 = 120.0;
/// Counts take tabular figures, so a changing number keeps its width.
const TABULAR: &str = "'tnum' 1";

pub(super) fn view(s: &Gallery, cx: &mut Cx<Gallery>) -> El {
    let k = Kit::neutral(cx.system_dark());
    let layout = s.layout();
    let main = row()
        .key("main")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(grid_region(s, &k, layout))
        .children(layout.show_inspector.then(|| inspector(s, &k)));
    let mut split = SplitView::new(main)
        .toolbar(toolbar(s, &k))
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .reduced_motion(cx.is_reduced_motion());
    split = if s.benchmark.is_some() {
        split.hide_sidebar_toggle()
    } else {
        split.sidebar(&s.sidebar, |s: &mut Gallery| &mut s.sidebar, sidebar(s, &k))
    };
    let body = col()
        .key("window")
        .w_full()
        .h_full()
        .fill(k.c.bg_deep)
        .on_layout_tx(|s: &mut Gallery, rect, tx| {
            let size = [rect[2], rect[3]];
            if s.window_size == size {
                tx.live_only();
            } else {
                s.window_size = size;
            }
        })
        .child(split.build(&k).grow(1.0).min_h(0.0))
        .child(status_bar(s, &k));
    let mut layer = Vec::new();
    match s.overlay {
        Overlay::Viewer => layer.extend(viewer(s)),
        Overlay::Failures => failures(s, &k, &mut layer),
        Overlay::None => {}
    }
    if let Some(line) = &s.performance {
        layer.push(performance(&line.text, &k));
    }
    rune_kit::root(body, layer)
}

fn toolbar(s: &Gallery, k: &Kit) -> Toolbar {
    let library_open = s.library.is_open();
    let performance_label = if s.performance.is_some() {
        "Hide performance"
    } else {
        "Show performance"
    };
    let import = Button::new("Import…")
        .icon(&icons::PLUS)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Large)
        .disabled(s.picking || !library_open)
        .on_press_tx(|_: &mut Gallery, tx: &mut Tx| {
            let menu = PopupMenu::new()
                .item(PopupItem::new("files", "Files or folders…"))
                .item(PopupItem::new("photos", "From Photos…"));
            tx.popup_menu(menu, "import", |s: &mut Gallery, picked, _| match picked {
                Some("files") => s.choose_files(),
                Some("photos") => s.choose_from_photos(),
                _ => {}
            });
        })
        .build(k)
        .key("import");
    let performance = IconButton::new(&icons::CPU)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Large)
        .expanded(s.performance.is_some())
        .label(performance_label)
        .help(performance_label)
        .on_press(|s: &mut Gallery| s.toggle_performance())
        .build(k)
        .key("performance-toggle");
    Toolbar::new()
        .title(APP_NAME)
        .trailing(row().gap(4.0).center_y().child(import).child(performance))
}

/// Persistent navigation: one destination per collection the library keeps,
/// with its count beside it.
fn sidebar(s: &Gallery, k: &Kit) -> El {
    let total_bytes: u64 = s.photos.iter().map(|photo| photo.record.byte_size).sum();
    let footer = row()
        .key("library-size")
        .gap(8.0)
        .px(8.0)
        .center_y()
        .child(icon(&icons::DRIVE, 14.0).color(k.c.icon_muted))
        .child(
            text(format::library_size(total_bytes))
                .font(k.ty.small)
                .color(k.c.text_muted)
                .truncate(),
        );
    let mut sidebar = Sidebar::new().label("Library").footer(footer).child(
        SidebarRow::new("All photos")
            .icon(&icons::PHOTOS)
            .detail(format::number(s.photos.len()))
            .selected(true)
            .build(k)
            .key("all-photos"),
    );
    if !s.failures.is_empty() {
        sidebar = sidebar.section(
            SidebarSection::new("Attention").key("attention").row(
                SidebarRow::new("Not imported")
                    .icon(&icons::ALERT)
                    .detail(format::number(s.failures.len()))
                    .on_select(|s: &mut Gallery, _| s.open_failures())
                    .build(k)
                    .key("not-imported"),
            ),
        );
    }
    sidebar.build(k)
}

fn status_bar(s: &Gallery, k: &Kit) -> El {
    let progress = s.progress.map(|(processed, total)| {
        let scanning = total == 0;
        #[allow(clippy::cast_precision_loss)]
        let value = (!scanning).then(|| processed as f32 / total as f32);
        let label = if scanning {
            "Looking for JPEGs…".to_owned()
        } else {
            format!("{} of {}", format::number(processed), format::number(total))
        };
        row()
            .key("import-progress")
            .gap(8.0)
            .center_y()
            .child(
                el().w(PROGRESS_WIDTH)
                    .label("Import progress")
                    .child(Progress::new(value).build(k)),
            )
            .child(
                text(label)
                    .font(k.ty.small.features(TABULAR))
                    .color(k.c.text_muted),
            )
    });
    row()
        .key("status-bar")
        .w_full()
        .h(STATUS_BAR_HEIGHT)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .child(
            text(s.status.clone())
                .key("status")
                .font(k.ty.small)
                .color(k.c.text_muted)
                .truncate()
                .grow(1.0),
        )
        .children(progress)
}

/// Frames drawn in the last second and the slowest one's CPU time, over the
/// window's bottom-right corner.
fn performance(line: &str, k: &Kit) -> El {
    col()
        .key("performance-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .items_end()
        .justify_end()
        .padding(16.0)
        .pb(STATUS_BAR_HEIGHT + 16.0)
        .inert()
        .child(
            row()
                .key("performance")
                .padding(8.0)
                .radius(k.radius.md)
                .fill(k.c.surface_float)
                .border(1.0, k.c.border_muted)
                .child(text(line.to_owned()).font(k.ty.mono).color(k.c.text_base)),
        )
}
