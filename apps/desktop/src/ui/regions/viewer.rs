//! The full-window view of one photo.

use crate::grid::Movement;
use crate::ui::design::glass;
use crate::ui::design::{CONTROL_RADIUS, Design, HEADER_HEIGHT, SPACE_MD, SPACE_SM, motion};
use crate::ui::{
    CloseViewer, NextPhoto, PreviousPhoto, VIEWER_CONTEXT, format,
    gallery::{Gallery, Thumbnail},
};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable as _, IconName,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use super::framed_image;

impl Gallery {
    pub(in crate::ui) fn viewer(&self, design: Design, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.selected_index() else {
            return div().into_any_element();
        };
        let photo = &self.photos[index];
        let preview = self.preview_for(&photo.record.id);
        let large = preview.and_then(|preview| preview.image.clone());
        let error = preview.and_then(|preview| preview.error.clone());
        let loading = large.is_none() && error.is_none();
        let image: AnyElement = match (&large, &photo.thumbnail) {
            (Some(path), _) => framed_image(path)
                .image_cache(&self.preview_cache)
                .into_any_element(),
            (None, Thumbnail::Ready(path)) => framed_image(path)
                .image_cache(&self.thumbnails_cache)
                .into_any_element(),
            (None, _) => div().into_any_element(),
        };
        let note = error.or_else(|| loading.then(|| SharedString::from("Loading full size…")));
        let (bar_text, bar_muted) = design.overlay_text();
        // A floating glass bar over the photograph. This is the one surface the
        // design system's `frosted` recipe is for: it carries its own labels and
        // must not rely on the plane behind the photo.
        let bar = glass::frosted(
            design,
            CONTROL_RADIUS,
            h_flex()
                .flex_none()
                .h(px(HEADER_HEIGHT))
                .px(px(SPACE_MD))
                .gap_2()
                .child(
                    Button::new("close-viewer")
                        .ghost()
                        .icon(IconName::Close)
                        .tooltip("Close (Esc)")
                        .text_color(bar_text)
                        .on_click(cx.listener(|view, _, window, cx| view.close_viewer(window, cx))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(bar_text)
                        .child(photo.record.file_name()),
                )
                .child(div().text_sm().text_color(bar_muted).child(format!(
                    "{} of {}",
                    format::number(index + 1),
                    format::number(self.photos.len())
                )))
                .child(
                    Button::new("previous-photo")
                        .ghost()
                        .icon(IconName::ChevronLeft)
                        .tooltip("Previous photo")
                        .disabled(index == 0)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.move_selection(Movement::Left, cx);
                        })),
                )
                .child(
                    Button::new("next-photo")
                        .ghost()
                        .icon(IconName::ChevronRight)
                        .tooltip("Next photo")
                        .disabled(index + 1 == self.photos.len())
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.move_selection(Movement::Right, cx);
                        })),
                ),
        );
        motion::dialog_in(
            "viewer-entrance",
            v_flex()
                .id("viewer")
                .key_context(VIEWER_CONTEXT)
                .track_focus(&self.viewer_focus)
                .on_action(
                    cx.listener(|view, _: &CloseViewer, window, cx| view.close_viewer(window, cx)),
                )
                .on_action(cx.listener(|view, _: &PreviousPhoto, _, cx| {
                    view.move_selection(Movement::Left, cx);
                }))
                .on_action(cx.listener(|view, _: &NextPhoto, _, cx| {
                    view.move_selection(Movement::Right, cx);
                }))
                .absolute()
                .inset_0()
                .occlude()
                .bg(design.scrim)
                .p(px(SPACE_MD))
                .child(bar)
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .mt(px(SPACE_MD))
                        .child(image),
                )
                .when_some(note, |this, note| {
                    this.child(
                        div()
                            .flex_none()
                            .pb(px(SPACE_SM))
                            .text_center()
                            .text_sm()
                            .text_color(design.text_muted)
                            .child(note),
                    )
                }),
        )
        .into_any_element()
    }
}
