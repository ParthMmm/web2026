//! The grid of photos, virtualised by row, and what shows in its place while the library
//! opens, when it can't open, and when it holds nothing yet.

use super::super::{
    gallery::{
        GRID_GAP, GRID_PADDING, Gallery, GalleryPhoto, GridLayout, LibraryState, Thumbnail, row_key,
    },
    icons,
};
use super::GRID_KEY;
use rune::{Color, El, GrowthSpec, ScrollSpec, SurfaceFit, Tx, col, el, icon, row, text};
use rune_kit::{Button, Kit};
use std::borrow::Cow;

const EDGE_FADE: f32 = 24.0;

pub(super) fn grid_region(s: &Gallery, k: &Kit, layout: GridLayout) -> El {
    let region = col()
        .key("grid-region")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0);
    let content = match &s.library {
        LibraryState::Opening => note("Opening library…", k),
        LibraryState::Unavailable(message) => unavailable(message, k),
        _ if s.photos.is_empty() && s.is_importing() => note("Looking for JPEGs…", k),
        _ if s.photos.is_empty() => empty_state(s, k),
        LibraryState::Open(_) | LibraryState::Demo => grid(s, k, layout),
    };
    region.child(content)
}

fn grid(s: &Gallery, k: &Kit, layout: GridLayout) -> El {
    let columns = layout.columns;
    let rows = s.photos.len().div_ceil(columns);
    let kit = k.clone();
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
                    move |s: &Gallery, row| Cow::Owned(row_key(&s.photos, row * columns, columns)),
                    move |s: &Gallery, row| grid_row(s, row, &kit, layout),
                )
                .on_visible_rows(|s: &mut Gallery, rows| {
                    s.set_visible_rows(rows);
                    false
                }),
        )
}

fn grid_row(s: &Gallery, row_index: usize, k: &Kit, layout: GridLayout) -> El {
    let start = row_index * layout.columns;
    let end = (start + layout.columns).min(s.photos.len());
    row()
        .w_full()
        .h(layout.row_height())
        .pt(GRID_GAP)
        .px(GRID_PADDING)
        .gap(GRID_GAP)
        .children(
            s.photos[start..end]
                .iter()
                .map(|photo| tile(s, photo, k, layout)),
        )
}

fn tile(s: &Gallery, photo: &GalleryPhoto, k: &Kit, layout: GridLayout) -> El {
    let selected = s.selected.as_ref() == Some(&photo.record.id);
    let ring = if selected {
        k.c.border_focus
    } else {
        Color::TRANSPARENT
    };
    let content = match &photo.thumbnail {
        Thumbnail::Ready(path) => s.thumbnails.pixels(path).map(|pixels| {
            el().absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .radius(k.radius.xl)
                .pixels_fit(pixels, SurfaceFit::Fit)
        }),
        Thumbnail::Pending => None,
        Thumbnail::Failed => Some(
            col()
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .center()
                .gap(4.0)
                .child(icon(&icons::ALERT, 14.0).color(k.c.icon_muted))
                .child(
                    text("Preview unavailable")
                        .font(k.ty.small)
                        .color(k.c.text_muted),
                ),
        ),
    };
    let missing = photo.record.source_missing.then(|| {
        el().absolute(layout.tile_width - 28.0, 6.0)
            .size(22.0, 22.0)
            .radius(11.0)
            .center()
            .fill(k.c.surface_raised)
            .child(icon(&icons::ALERT, 12.0).color(k.c.warning.icon))
    });
    let id = photo.record.id.clone();
    let open = photo.record.id.clone();
    let hit = el()
        .key("hit")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .radius(k.radius.xl)
        .border(2.0, ring)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .role("button")
        .label(photo.record.file_name())
        .checked(selected)
        .on_press_tx(move |s: &mut Gallery, tx: &mut Tx| {
            if s.benchmark.is_none() {
                s.select_id(&id, tx);
            }
        })
        .on_double_press_tx(move |s: &mut Gallery, tx: &mut Tx| {
            s.select_id(&open, tx);
            s.open_viewer();
        });
    el().key(photo.key.clone())
        .size(layout.tile_width, layout.tile_height)
        .radius(k.radius.xl)
        .fill(k.c.layer_02)
        .children(content)
        .children(missing)
        .child(hit)
}

fn empty_state(s: &Gallery, k: &Kit) -> El {
    col()
        .key("empty")
        .w_full()
        .h_full()
        .center()
        .gap(16.0)
        .padding(32.0)
        .child(icon(&icons::PHOTOS, 32.0).color(k.c.icon_muted))
        .child(
            col()
                .center()
                .gap(4.0)
                .child(
                    text("No photos yet")
                        .font(k.ty.body_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text(
                        "Import JPEGs from folders or from Photos. Originals stay where they are.",
                    )
                    .font(k.ty.ui)
                    .color(k.c.text_muted),
                ),
        )
        .child(
            row()
                .gap(8.0)
                .child(
                    Button::new("Import files…")
                        .disabled(s.picking)
                        .on_press(|s: &mut Gallery| s.choose_files())
                        .build(k)
                        .key("empty-import-files"),
                )
                .child(
                    Button::new("Import from Photos…")
                        .disabled(s.picking)
                        .on_press(|s: &mut Gallery| s.choose_from_photos())
                        .build(k)
                        .key("empty-import-photos"),
                ),
        )
}

fn unavailable(message: &str, k: &Kit) -> El {
    col().w_full().h_full().center().padding(32.0).child(
        col()
            .key("library-unavailable")
            .role("alert")
            .max_w(480.0)
            .gap(4.0)
            .padding(16.0)
            .radius(k.radius.lg)
            .fill(k.c.layer_01)
            .border(1.0, k.c.danger.border)
            .child(
                text("The library couldn't be opened")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base),
            )
            .child(
                text(message.to_owned())
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
            ),
    )
}

fn note(message: &'static str, k: &Kit) -> El {
    col()
        .w_full()
        .h_full()
        .center()
        .child(text(message).font(k.ty.ui).color(k.c.text_muted))
}
