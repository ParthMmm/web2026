//! The grid of photos, virtualised by row, and what shows in its place while the library
//! opens, when it can't open, and when it holds nothing yet.

use super::super::{
    gallery::{
        Destination, GRID_GAP, GRID_GUTTER, Gallery, GalleryPhoto, GridLayout, LibraryState,
        Thumbnail, row_key,
    },
    icons,
    look::Look,
};
use super::{GRID_KEY, cloud::tile_mark};
use crate::cloud::CloudStep;
use rune::{
    Color, El, GrowthSpec, Mods, PopupItem, PopupMenu, ScrollSpec, SurfaceFit, TextStyle, Tx, col,
    el, icon, row, text,
};
use rune_kit::Button;
use std::borrow::Cow;

const EDGE_FADE: f32 = 24.0;
const TILE_RADIUS: f32 = 4.0;
/// The gap between a tile and its selection ring.
const RING_OFFSET: f32 = 3.0;

pub(super) fn grid_region(s: &Gallery, look: &Look, layout: GridLayout) -> El {
    let region = col()
        .key("grid-region")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .fill(look.plane);
    let content = match &s.library {
        LibraryState::Opening => note("Opening library…", look),
        LibraryState::Unavailable(message) => unavailable(message, look),
        _ if s.photos.is_empty() && s.is_importing() => note("Looking for JPEGs…", look),
        _ if s.photos.is_empty() => empty_state(s, look),
        _ if s.shown_len() == 0 => note(empty_destination(s.destination), look),
        LibraryState::Open(_) | LibraryState::Demo => grid(s, look, layout),
    };
    region.child(content)
}

fn grid(s: &Gallery, look: &Look, layout: GridLayout) -> El {
    let columns = layout.columns;
    let rows = s.shown_len().div_ceil(columns);
    let look = look.clone();
    col()
        .key(GRID_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .scroll_edge_fade(EDGE_FADE)
        .on_layout_tx(|s: &mut Gallery, rect, tx| {
            let size = Some([rect[2], rect[3]]);
            if s.grid_size == size {
                tx.live_only();
            } else {
                s.grid_size = size;
            }
        })
        .child(
            col()
                .key("grid-rows")
                .w_full()
                .growth(GrowthSpec::default())
                .virtual_rows(
                    rows,
                    layout.row_height(),
                    move |s: &Gallery, row| Cow::Owned(row_key(s, row * columns, columns)),
                    move |s: &Gallery, row| grid_row(s, row, &look, layout),
                )
                .on_visible_rows(|s: &mut Gallery, rows| {
                    s.set_visible_rows(rows);
                    false
                }),
        )
}

fn grid_row(s: &Gallery, row_index: usize, look: &Look, layout: GridLayout) -> El {
    let start = row_index * layout.columns;
    let end = (start + layout.columns).min(s.shown_len());
    row()
        .w_full()
        .h(layout.row_height())
        .pt(GRID_GAP)
        .px(GRID_GUTTER)
        .gap(GRID_GAP)
        .children(
            (start..end)
                .filter_map(|position| s.shown(position))
                .map(|photo| tile(s, photo, look, layout)),
        )
}

/// A square crop of the photo on a plate. A selected tile's ring stands off the photo
/// instead of covering its edge.
fn tile(s: &Gallery, photo: &GalleryPhoto, look: &Look, layout: GridLayout) -> El {
    let selected = s.selection.contains(&photo.record.id);
    let missing = photo.record.source_missing;
    let content = match &photo.thumbnail {
        Thumbnail::Ready(path) => s.thumbnails.pixels(path).map(|pixels| {
            el().absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .radius(TILE_RADIUS)
                .opacity(if missing { 0.4 } else { 1.0 })
                .pixels_fit(pixels, SurfaceFit::Fill)
        }),
        // The plate alone: a pending preview draws nothing that moves.
        Thumbnail::Pending => None,
        Thumbnail::Failed => Some(
            col()
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .center()
                .gap(4.0)
                .child(icon(&icons::ALERT, 14.0).color(look.muted))
                .child(
                    text("Preview unavailable")
                        .font(look.ty.small)
                        .color(look.muted),
                ),
        ),
    };
    let badge = missing.then(|| {
        row()
            .key("missing")
            .absolute(6.0, layout.tile_height - 26.0)
            .h(20.0)
            .px(6.0)
            .gap(4.0)
            .center_y()
            .radius(5.0)
            .fill(Color::BLACK.mix(Color::WHITE, 0.04).alpha(0.62))
            .child(icon(&icons::ALERT, 12.0).color(look.warning))
            .child(
                text("Original missing")
                    .font(TextStyle::new(10.5, 400.0))
                    .color(Color::WHITE.mix(Color::BLACK, 0.07)),
            )
    });
    let ring = selected.then(|| {
        el().key("ring")
            .absolute(-RING_OFFSET, -RING_OFFSET)
            .size(
                layout.tile_width + RING_OFFSET * 2.0,
                layout.tile_height + RING_OFFSET * 2.0,
            )
            .radius(TILE_RADIUS + RING_OFFSET)
            .border(2.0, look.selection)
            .inert()
    });
    let id = photo.record.id.clone();
    let open = photo.record.id.clone();
    let menu = photo.record.id.clone();
    let hit = el()
        .key("hit")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .radius(TILE_RADIUS)
        .hover(look.c.hover)
        .press(look.c.pressed)
        .role("button")
        .label(photo.record.file_name())
        .checked(selected)
        .on_press_tx(move |s: &mut Gallery, tx: &mut Tx| {
            if s.benchmark.is_none() {
                let mods = tx.mods();
                s.click(&id, mods, tx);
            }
        })
        .on_double_press_tx(move |s: &mut Gallery, tx: &mut Tx| {
            if s.focus() != Some(&open) {
                s.click(&open, Mods::default(), tx);
            }
            s.open_viewer();
        })
        // A right click on a photo outside the selection selects it first, so the menu acts on
        // the photo under the pointer, as in Finder and Photos. Inside the selection, the menu
        // acts on every selected photo.
        .on_secondary_press(move |s: &mut Gallery, press, tx: &mut Tx| {
            if s.benchmark.is_some() {
                return;
            }
            s.select_for_menu(&menu, tx);
            let info = if s.inspector_hidden {
                "Show Info"
            } else {
                "Hide Info"
            };
            let connected = s.is_connected();
            let can = |count: usize| connected && count > 0;
            let items = PopupMenu::new()
                .item(PopupItem::new("open", "Open"))
                .item(PopupItem::new("info", info))
                .separator()
                .item(PopupItem::new("reveal", "Show in Finder"))
                .separator()
                .item(
                    PopupItem::new("upload", "Upload")
                        .enabled(can(s.selected_for(CloudStep::Upload))),
                )
                .item(
                    PopupItem::new("publish", "Publish")
                        .enabled(can(s.selected_for(CloudStep::Publish))),
                )
                .item(
                    PopupItem::new("unpublish", "Unpublish")
                        .enabled(can(s.selected_for(CloudStep::Unpublish))),
                )
                .item(PopupItem::new("retry", "Retry").enabled(can(s.selected_retries())));
            tx.popup_menu(items, press, |s: &mut Gallery, picked, _| match picked {
                Some("open") => s.open_viewer(),
                Some("info") => s.toggle_inspector(),
                Some("reveal") => s.reveal_selected(),
                Some("upload") => s.upload_selected(),
                Some("publish") => s.publish_selected(),
                Some("unpublish") => s.unpublish_selected(),
                Some("retry") => s.retry_selected(),
                _ => {}
            });
        });
    el().key(photo.key.clone())
        .size(layout.tile_width, layout.tile_height)
        .radius(TILE_RADIUS)
        .fill(look.plate)
        .children(content)
        .children(badge)
        .children(tile_mark(photo, look, layout.tile_width))
        .child(hit)
        .children(ring)
}

fn empty_state(s: &Gallery, look: &Look) -> El {
    col()
        .key("empty")
        .w_full()
        .h_full()
        .center()
        .gap(16.0)
        .padding(32.0)
        .child(icon(&icons::PHOTOS, 32.0).color(look.c.icon_muted))
        .child(
            col()
                .center()
                .gap(4.0)
                .child(
                    text("No photos yet")
                        .font(look.ty.body_medium)
                        .color(look.c.text_base),
                )
                .child(
                    text(
                        "Import JPEGs from folders or from Photos. Originals stay where they are.",
                    )
                    .font(look.ty.ui)
                    .color(look.c.text_muted),
                ),
        )
        .child(
            row()
                .gap(8.0)
                .child(
                    Button::new("Import files…")
                        .disabled(s.picking)
                        .on_press(|s: &mut Gallery| s.choose_files())
                        .build(look)
                        .key("empty-import-files"),
                )
                .child(
                    Button::new("Import from Photos…")
                        .disabled(s.picking)
                        .on_press(|s: &mut Gallery| s.choose_from_photos())
                        .build(look)
                        .key("empty-import-photos"),
                ),
        )
}

fn unavailable(message: &str, look: &Look) -> El {
    col().w_full().h_full().center().padding(32.0).child(
        col()
            .key("library-unavailable")
            .role("alert")
            .max_w(480.0)
            .gap(4.0)
            .padding(16.0)
            .radius(look.radius.lg)
            .fill(look.c.layer_01)
            .border(1.0, look.c.danger.border)
            .child(
                text("The library couldn't be opened")
                    .font(look.ty.ui_medium)
                    .color(look.c.text_base),
            )
            .child(
                text(message.to_owned())
                    .font(look.ty.ui)
                    .color(look.c.text_muted)
                    .wrap(),
            ),
    )
}

/// What the grid says for a destination that holds no photos.
fn empty_destination(destination: Destination) -> &'static str {
    match destination {
        Destination::All => "No photos yet",
        Destination::Drafts => "No drafts. Upload a photo to make one.",
        Destination::Published => "Nothing is published. Publish a draft to add it.",
    }
}

fn note(message: &'static str, look: &Look) -> El {
    col()
        .w_full()
        .h_full()
        .center()
        .child(text(message).font(look.ty.ui).color(look.c.text_muted))
}
