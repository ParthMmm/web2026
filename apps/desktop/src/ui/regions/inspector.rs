//! The panel that describes the selected photo.

use super::framed_image;
use crate::ui::components::{DetailList, ExposureReadout, FilmChip};
use crate::ui::design::{Design, HEADER_HEIGHT};
use crate::ui::layout::INSPECTOR_WIDTH;
use crate::ui::{
    format,
    gallery::{Gallery, GalleryPhoto, Thumbnail},
};
use gpui::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, alert::Alert, h_flex,
    scroll::ScrollableElement as _, skeleton::Skeleton, v_flex,
};

const PREVIEW_HEIGHT: f32 = 184.0;
const PREVIEW_RADIUS: f32 = 8.0;
const BODY_PADDING: f32 = 12.0;

impl Gallery {
    pub(in crate::ui) fn inspector(
        &self,
        design: Design,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = v_flex()
            .id("inspector")
            .flex_none()
            .w(px(INSPECTOR_WIDTH))
            .h_full()
            .bg(design.chrome())
            .border_l_1()
            .border_color(design.border)
            .child(
                h_flex()
                    .flex_none()
                    .h(px(HEADER_HEIGHT))
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(design.border)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(design.text_muted)
                    .child("Info"),
            );
        let Some(photo) = self.selected_photo() else {
            // Only reachable when the library has photos but none is selectable.
            return panel.child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::GalleryVerticalEnd)
                            .large()
                            .text_color(design.text_faint),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(design.text_muted)
                            .child("Select a photo"),
                    ),
            );
        };
        panel.child(
            v_flex()
                .id("inspector-body")
                .flex_1()
                .min_h_0()
                .p(px(BODY_PADDING))
                .gap(px(12.0))
                .child(self.preview_image(photo, px(PREVIEW_HEIGHT), design))
                .child(identity(photo, design, cx))
                .children(ExposureReadout::new(design, &photo.record.metadata))
                .child(self.details(photo, design))
                .overflow_y_scrollbar(),
        )
    }

    fn details(&self, photo: &GalleryPhoto, design: Design) -> impl IntoElement {
        let record = &photo.record;
        let metadata = &record.metadata;
        let preview_error = self
            .preview_for(&record.id)
            .and_then(|preview| preview.error.clone());
        let mut details = DetailList::new(design);
        if let Some(film) = metadata.film_simulation {
            details = details.row("Film", FilmChip::new(design, film));
        }
        if let Some(camera) = metadata.camera() {
            details = details.row("Camera", camera);
        }
        if let Some(lens) = &metadata.lens {
            details = details.row("Lens", lens.clone());
        }
        if let Some(captured_at) = &metadata.captured_at {
            details = details.row("Taken", captured_at.to_string());
        }
        if record.source_count > 1 {
            details = details.row(
                "Copies",
                format!("Found in {} places", format::number(record.source_count)),
            );
        }
        details = details.row(
            "Original",
            div()
                .text_color(design.text_muted)
                .child(record.source.display().to_string()),
        );
        v_flex()
            .gap(px(8.0))
            .when(record.source_missing, |this| {
                this.child(
                    Alert::warning(
                        "source-missing",
                        "Import it from its new location to reconnect it. Its library previews are kept.",
                    )
                    .title("The original has moved or was deleted"),
                )
            })
            .when_some(preview_error, |this, message| {
                this.child(div().text_sm().text_color(design.text_muted).child(message))
            })
            .child(details)
    }

    /// The large rendition when ready, otherwise the grid thumbnail.
    fn preview_image(
        &self,
        photo: &GalleryPhoto,
        height: Pixels,
        design: Design,
    ) -> impl IntoElement {
        let large = self
            .preview_for(&photo.record.id)
            .and_then(|preview| preview.image.clone());
        let frame = div()
            .relative()
            .flex_none()
            .w_full()
            .h(height)
            .overflow_hidden()
            .rounded(px(PREVIEW_RADIUS))
            .bg(design.ink(0.35))
            .border_1()
            .border_color(design.border);
        match (large, &photo.thumbnail) {
            (Some(path), _) => frame.child(framed_image(&path).image_cache(&self.preview_cache)),
            (None, Thumbnail::Ready(path)) => {
                frame.child(framed_image(path).image_cache(&self.thumbnails_cache))
            }
            (None, _) => frame.child(Skeleton::new().size_full()),
        }
    }
}

/// The file name, then its pixel size and weight on one quiet line.
fn identity(photo: &GalleryPhoto, design: Design, cx: &Context<Gallery>) -> impl IntoElement {
    let record = &photo.record;
    v_flex()
        .gap(px(2.0))
        .px(px(4.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(record.file_name()),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(px(11.5))
                .text_color(design.text_faint)
                .child(format!(
                    "{} × {} · {}",
                    record.width,
                    record.height,
                    format::bytes(record.byte_size)
                )),
        )
}
