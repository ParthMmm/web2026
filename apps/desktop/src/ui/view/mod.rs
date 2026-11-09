//! The window: the title bar across the top, the sidebar, the grid and the inspector below
//! it, the status bar, and what covers them (the viewer, the failures sheet, the performance
//! panel). Each part draws the gallery's state and nothing else.

mod failures;
mod grid;
mod inspector;
mod parts;
mod title_bar;
mod viewer;

use super::{
    format,
    gallery::{Gallery, Overlay, SIDEBAR_WIDTH},
    icons,
    look::{Look, MONO, MONO_SMALL},
};
use failures::failures;
use grid::grid_region;
use inspector::inspector;
use rune::{Cx, El, TextStyle, col, icon, row, text};
use rune_kit::{
    Progress,
    shell::{Sidebar, SidebarRow, SidebarSection},
};
use title_bar::title_bar;
use viewer::viewer;

pub(super) const GRID_KEY: &str = "photo-grid";
pub(super) const TITLE_BAR_HEIGHT: f32 = 44.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;
const PROGRESS_WIDTH: f32 = 120.0;

pub(super) fn view(s: &Gallery, cx: &mut Cx<Gallery>) -> El {
    let look = Look::new(cx.system_dark());
    let layout = s.layout();
    let panes = row()
        .key("panes")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .children(layout.show_sidebar.then(|| sidebar(s, &look)))
        .child(grid_region(s, &look, layout))
        .children(layout.show_inspector.then(|| inspector(s, &look)));
    let body = col()
        .key("window")
        .w_full()
        .h_full()
        .fill(look.plane)
        .on_layout_tx(|s: &mut Gallery, rect, tx| {
            let size = [rect[2], rect[3]];
            if s.window_size == size {
                tx.live_only();
            } else {
                s.window_size = size;
            }
        })
        .child(title_bar(s, &look, layout, cx.titlebar_inset()))
        .child(panes)
        .child(status_bar(s, &look));
    let mut layer = Vec::new();
    match s.overlay {
        Overlay::Viewer => layer.extend(viewer(s, &look)),
        Overlay::Failures => failures(s, &look, &mut layer),
        Overlay::None => {}
    }
    if let Some(line) = &s.performance {
        layer.push(performance(&line.text, &look));
    }
    rune_kit::root(body, layer)
}

/// Persistent navigation beside the grid: one destination per collection the library keeps,
/// with its count, and the weight of the originals in the footer.
fn sidebar(s: &Gallery, look: &Look) -> El {
    let total_bytes: u64 = s.photos.iter().map(|photo| photo.record.byte_size).sum();
    let footer = row()
        .key("library-size")
        .gap(8.0)
        .px(8.0)
        .center_y()
        .child(icon(&icons::DRIVE, 14.0).color(look.faint))
        .child(
            col()
                .min_w(0.0)
                .child(
                    text(format::library_size(total_bytes))
                        .font(look.ty.small)
                        .color(look.muted)
                        .truncate(),
                )
                .child(
                    text("Left where they are, not copied")
                        .font(look.ty.small)
                        .color(look.faint)
                        .truncate(),
                ),
        );
    let mut sidebar = Sidebar::new().label("Library").footer(footer).section(
        SidebarSection::new("Library").key("library").row(
            SidebarRow::new("All photos")
                .icon(&icons::PHOTOS)
                .detail(format::number(s.photos.len()))
                .selected(true)
                .build(look)
                .key("all-photos"),
        ),
    );
    if !s.failures.is_empty() {
        sidebar = sidebar.section(
            SidebarSection::new("Needs attention").key("attention").row(
                SidebarRow::new("Not imported")
                    .icon(&icons::ALERT)
                    .detail(format::number(s.failures.len()))
                    .on_select(|s: &mut Gallery, _| s.open_failures())
                    .build(look)
                    .key("not-imported"),
            ),
        );
    }
    col()
        .key("sidebar-pane")
        .w(SIDEBAR_WIDTH)
        .h_full()
        .min_h(0.0)
        .pt(8.0)
        .fill(look.chrome)
        .border_sides([0.0, 1.0, 0.0, 0.0], look.rule)
        .child(sidebar.build(look))
}

/// What is in view and selected at rest, a transient message when there is one, and import
/// progress.
fn status_bar(s: &Gallery, look: &Look) -> El {
    // The kit fills a progress bar with its active border; the design fills it with the accent.
    let mut bar = look.k.clone();
    bar.c.border_active = look.primary;
    let progress = s.progress.map(|(processed, total)| {
        let scanning = total == 0;
        #[allow(clippy::cast_precision_loss)]
        let value = (!scanning).then(|| processed as f32 / total as f32);
        row()
            .key("import-progress")
            .label("Import progress")
            .gap(10.0)
            .center_y()
            .child(
                text(if scanning {
                    "Looking for JPEGs…"
                } else {
                    "Importing"
                })
                .font(STATUS)
                .color(look.muted),
            )
            .children((!scanning).then(|| {
                text(format!(
                    "{} of {}",
                    format::number(processed),
                    format::number(total)
                ))
                .font(MONO_SMALL)
                .color(look.muted)
            }))
            .child(
                col()
                    .w(PROGRESS_WIDTH)
                    .child(Progress::new(value).build(&bar)),
            )
    });
    let left = if s.status.is_empty() {
        text(selection_summary(s)).font(MONO_SMALL)
    } else {
        text(s.status.clone()).font(STATUS)
    };
    row()
        .key("status-bar")
        .w_full()
        .h(STATUS_BAR_HEIGHT)
        .px(14.0)
        .gap(10.0)
        .center_y()
        .fill(look.chrome)
        .border_sides([1.0, 0.0, 0.0, 0.0], look.rule)
        .child(left.key("status").color(look.muted).truncate().grow(1.0))
        .children(progress)
}

const STATUS: TextStyle = TextStyle::new(11.5, 400.0);

fn selection_summary(s: &Gallery) -> String {
    let photos = format::count(s.photos.len(), "photo", "photos");
    if s.selected.is_some() {
        format!("{photos} · 1 selected")
    } else {
        photos
    }
}

/// Frames drawn in the last second and the slowest one's CPU time, over the window's
/// bottom-right corner.
fn performance(line: &str, look: &Look) -> El {
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
                .radius(look.radius.md)
                .fill(look.c.surface_raised_base)
                .border(1.0, look.rule)
                .child(text(line.to_owned()).font(MONO).color(look.fg)),
        )
}
