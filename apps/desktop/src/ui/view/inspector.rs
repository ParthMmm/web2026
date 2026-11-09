//! The inspector beside the grid: the selected photo large, its name and size, its camera
//! settings, its details, and where its original lives.

use super::super::{
    format,
    gallery::{Gallery, GalleryPhoto, INSPECTOR_WIDTH, Thumbnail},
    icons,
    look::{Look, MONO_SMALL},
};
use super::{
    TITLE_BAR_HEIGHT,
    parts::{detail, detail_text, exposure_readout, film_chip},
};
use rune::{El, ScrollSpec, SurfaceFit, TextStyle, col, el, icon, row, text};

const PREVIEW_HEIGHT: f32 = 184.0;
const PREVIEW_RADIUS: f32 = 8.0;
const BODY_PADDING: f32 = 12.0;

pub(super) fn inspector(s: &Gallery, look: &Look) -> El {
    let panel = col()
        .key("inspector")
        .w(INSPECTOR_WIDTH)
        .h_full()
        .min_h(0.0)
        .fill(look.chrome)
        .border_sides([0.0, 0.0, 0.0, 1.0], look.rule)
        .child(
            row()
                .key("inspector-header")
                .w_full()
                .h(TITLE_BAR_HEIGHT)
                .px(16.0)
                .center_y()
                .border_sides([0.0, 0.0, 1.0, 0.0], look.rule)
                .child(
                    text("Info")
                        .font(TextStyle::new(12.0, 500.0))
                        .color(look.muted),
                ),
        );
    let Some(photo) = s.selected_photo() else {
        // Only reachable when the library has photos but none is selected.
        return panel.child(
            col()
                .w_full()
                .grow(1.0)
                .center()
                .gap(8.0)
                .child(icon(&icons::PHOTOS, 24.0).color(look.faint))
                .child(text("Select a photo").font(look.ty.ui).color(look.muted)),
        );
    };
    let record = &photo.record;
    let content = col()
        .key("inspector-content")
        .w_full()
        .padding(BODY_PADDING)
        .gap(12.0)
        .child(
            el().key("inspector-preview")
                .w_full()
                .h(PREVIEW_HEIGHT)
                .radius(PREVIEW_RADIUS)
                .fill(look.plate)
                .border(1.0, look.rule)
                .children(preview_pixels(s, photo).map(|image| image.radius(PREVIEW_RADIUS))),
        )
        .child(
            col()
                .gap(2.0)
                .px(4.0)
                .child(
                    text(record.file_name())
                        .font(TextStyle::new(14.0, 600.0))
                        .color(look.fg)
                        .truncate(),
                )
                .child(
                    text(format!(
                        "{} × {} · {}",
                        record.width,
                        record.height,
                        format::bytes(record.byte_size)
                    ))
                    .font(MONO_SMALL)
                    .color(look.faint),
                ),
        )
        .children(exposure_readout(look, &record.metadata))
        .child(details(s, photo, look));
    panel.child(
        col()
            .key("inspector-scroll")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(content),
    )
}

/// What went wrong, if anything, then a label and value for each detail the photo records.
/// Absent values are left out, never shown as a placeholder.
fn details(s: &Gallery, photo: &GalleryPhoto, look: &Look) -> El {
    let record = &photo.record;
    let metadata = &record.metadata;
    let preview_error = s
        .preview
        .as_ref()
        .filter(|preview| preview.id == record.id)
        .and_then(|preview| preview.error.clone());
    let mut rows = Vec::new();
    if let Some(film) = metadata.film_simulation {
        rows.push(detail(look, "Film", row().child(film_chip(look, film))));
    }
    if let Some(camera) = metadata.camera() {
        rows.push(detail(look, "Camera", detail_text(camera, look.fg)));
    }
    if let Some(lens) = &metadata.lens {
        rows.push(detail(look, "Lens", detail_text(lens.clone(), look.fg)));
    }
    if let Some(captured_at) = &metadata.captured_at {
        rows.push(detail(
            look,
            "Taken",
            detail_text(captured_at.to_string(), look.fg),
        ));
    }
    if record.source_count > 1 {
        rows.push(detail(
            look,
            "Copies",
            detail_text(
                format!("Found in {} places", format::number(record.source_count)),
                look.fg,
            ),
        ));
    }
    rows.push(detail(
        look,
        "Original",
        detail_text(record.source.display().to_string(), look.muted),
    ));
    col()
        .key("details")
        .w_full()
        .gap(8.0)
        .children(record.source_missing.then(|| {
            col()
                .key("source-missing")
                .role("alert")
                .gap(4.0)
                .padding(12.0)
                .radius(look.radius.lg)
                .fill(look.c.warning.bg)
                .border(1.0, look.c.warning.border)
                .child(
                    row()
                        .gap(6.0)
                        .center_y()
                        .child(icon(&icons::ALERT, 14.0).color(look.warning))
                        .child(text("The original has moved or was deleted").font(look.ty.small_medium).color(look.fg)),
                )
                .child(
                    text("Import it from its new location to reconnect it. Its library previews are kept.")
                        .font(look.ty.small)
                        .color(look.muted)
                        .wrap(),
                )
        }))
        .children(preview_error.map(|message| text(message).font(look.ty.small).color(look.muted).wrap()))
        .child(col().w_full().children(rows))
}

/// The large rendition when decoded, otherwise the grid thumbnail.
pub(super) fn preview_pixels(s: &Gallery, photo: &GalleryPhoto) -> Option<El> {
    let large = s
        .preview
        .as_ref()
        .filter(|preview| preview.id == photo.record.id)
        .and_then(|preview| preview.image.as_deref())
        .and_then(|path| s.previews.pixels(path));
    let thumbnail = match &photo.thumbnail {
        Thumbnail::Ready(path) => s.thumbnails.pixels(path),
        _ => None,
    };
    large.or(thumbnail).map(|pixels| {
        el().absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .pixels_fit(pixels, SurfaceFit::Fit)
    })
}
