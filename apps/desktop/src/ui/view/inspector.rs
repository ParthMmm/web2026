//! The inspector beside the grid: the selected photo large, its name, exposure and details,
//! and where its original lives.

use super::super::{
    format,
    gallery::{Gallery, GalleryPhoto, INSPECTOR_WIDTH, Thumbnail},
    icons,
};
use rune::{El, ScrollSpec, SurfaceFit, col, el, icon, row, text};
use rune_kit::Kit;

const PREVIEW_HEIGHT: f32 = 220.0;
const DETAIL_LABEL_WIDTH: f32 = 92.0;

pub(super) fn inspector(s: &Gallery, k: &Kit) -> El {
    let panel = col()
        .key("inspector")
        .w(INSPECTOR_WIDTH)
        .h_full()
        .min_h(0.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted);
    let Some(photo) = s.selected_photo() else {
        return panel.center().child(
            col()
                .center()
                .gap(8.0)
                .padding(16.0)
                .child(icon(&icons::PHOTOS, 32.0).color(k.c.icon_faint))
                .child(text("Select a photo").font(k.ty.ui).color(k.c.text_muted)),
        );
    };
    let record = &photo.record;
    let metadata = &record.metadata;
    let mut details = vec![
        (
            "Dimensions",
            format!("{} × {}", record.width, record.height),
        ),
        ("File size", format::bytes(record.byte_size)),
    ];
    if let Some(camera) = metadata.camera() {
        details.push(("Camera", camera));
    }
    if let Some(lens) = &metadata.lens {
        details.push(("Lens", lens.clone()));
    }
    if let Some(film) = metadata.film_simulation {
        details.push(("Film simulation", film.to_string()));
    }
    if let Some(captured_at) = &metadata.captured_at {
        details.push(("Taken", captured_at.to_string()));
    }
    if record.source_count > 1 {
        details.push((
            "Copies",
            format!("Found in {} places", format::number(record.source_count)),
        ));
    }
    let preview_error = s
        .preview
        .as_ref()
        .filter(|preview| preview.id == record.id)
        .and_then(|preview| preview.error.clone());
    let content = col()
        .key("inspector-content")
        .w_full()
        .padding(16.0)
        .gap(16.0)
        .child(
            el().key("inspector-preview")
                .w_full()
                .h(PREVIEW_HEIGHT)
                .radius(k.radius.xl)
                .fill(k.c.layer_02)
                .children(preview_pixels(s, photo).map(|image| image.radius(k.radius.xl))),
        )
        .child(
            col()
                .gap(4.0)
                .child(
                    text(record.file_name())
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .truncate(),
                )
                .children(metadata.exposure_summary().map(|summary| {
                    text(summary).font(k.ty.small).color(k.c.text_muted)
                })),
        )
        .children(record.source_missing.then(|| {
            col()
                .key("source-missing")
                .role("alert")
                .gap(4.0)
                .padding(12.0)
                .radius(k.radius.lg)
                .fill(k.c.surface_raised)
                .border(1.0, k.c.warning.border)
                .child(
                    text("The original has moved or was deleted")
                        .font(k.ty.small_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text("Import it from its new location to reconnect it. Its library previews are kept.")
                        .font(k.ty.small)
                        .color(k.c.text_muted)
                        .wrap(),
                )
        }))
        .children(
            preview_error
                .map(|message| text(message).font(k.ty.small).color(k.c.text_muted).wrap()),
        )
        .child(col().key("details").gap(6.0).children(details.into_iter().map(
            |(label, value)| {
                row()
                    .gap(8.0)
                    .child(
                        text(label)
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .w(DETAIL_LABEL_WIDTH),
                    )
                    .child(text(value).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0))
            },
        )))
        .child(
            col()
                .gap(4.0)
                .child(text("Original").font(k.ty.small).color(k.c.text_muted))
                .child(
                    text(record.source.display().to_string())
                        .font(k.ty.small)
                        .color(k.c.text_base)
                        .wrap(),
                ),
        );
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
