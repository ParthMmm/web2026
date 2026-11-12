//! The window: the title bar across the top, the sidebar, the grid and the inspector below
//! it, the status bar, and what covers them (the viewer, the failures sheet, the performance
//! panel). Each part draws the gallery's state and nothing else.

mod cloud;
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
use cloud::progress_kit;
use failures::{cloud_failures, failures};
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
        Overlay::CloudFailures => cloud_failures(s, &look, &mut layer),
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
    let cloud_problems = s.cloud_failures().count();
    if !s.failures.is_empty() || cloud_problems > 0 {
        let mut attention = SidebarSection::new("Needs attention").key("attention");
        if !s.failures.is_empty() {
            attention = attention.row(
                SidebarRow::new("Not imported")
                    .icon(&icons::ALERT)
                    .detail(format::number(s.failures.len()))
                    .on_select(|s: &mut Gallery, _| s.open_failures())
                    .build(look)
                    .key("not-imported"),
            );
        }
        if cloud_problems > 0 {
            attention = attention.row(
                SidebarRow::new("Cloud problems")
                    .icon(&icons::CLOUD_OFF)
                    .detail(format::number(cloud_problems))
                    .on_select(|s: &mut Gallery, _| s.open_cloud_failures())
                    .build(look)
                    .key("cloud-problems"),
            );
        }
        sidebar = sidebar.section(attention);
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

/// What is in view and selected at rest, a transient message when there is one, import and
/// upload progress, and the photos being published or unpublished.
fn status_bar(s: &Gallery, look: &Look) -> El {
    let progress = s.progress.map(|(processed, total)| {
        let scanning = total == 0;
        let words = if scanning {
            "Looking for JPEGs…"
        } else {
            "Importing"
        };
        progress_line(
            look,
            "import-progress",
            "Import progress",
            words,
            (!scanning).then_some((processed, total)),
        )
    });
    let uploads = s.uploads.filter(|run| run.total > 0).map(|run| {
        let words = if s.is_connected() {
            "Uploading"
        } else {
            "Waiting to upload · not connected"
        };
        progress_line(
            look,
            "upload-progress",
            "Upload progress",
            words,
            Some((run.finished(), run.total)),
        )
    });
    let (publishing, unpublishing) = s.publishing();
    let publish =
        format::publishing_status(publishing, unpublishing, s.is_connected()).map(|words| {
            text(words)
                .key("publish-status")
                .font(STATUS)
                .color(look.muted)
                .truncate()
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
        .children(publish)
        .children(progress)
        .children(uploads)
}

/// Words, a count when there is one, and a bar that is indeterminate without it.
fn progress_line(
    look: &Look,
    key: &'static str,
    label: &'static str,
    words: &'static str,
    count: Option<(usize, usize)>,
) -> El {
    #[allow(clippy::cast_precision_loss)]
    let value = count.map(|(done, total)| done as f32 / total as f32);
    row()
        .key(key)
        .label(label)
        .gap(10.0)
        .center_y()
        .child(text(words).font(STATUS).color(look.muted))
        .children(count.map(|(done, total)| {
            text(format!(
                "{} of {}",
                format::number(done),
                format::number(total)
            ))
            .font(MONO_SMALL)
            .color(look.muted)
        }))
        .child(
            col()
                .w(PROGRESS_WIDTH)
                .child(Progress::new(value).build(&progress_kit(look))),
        )
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
