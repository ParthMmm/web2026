//! The full-window view of one photo: the photo on a quiet stage, its name
//! and place in the library above it, the camera settings over it, and a
//! filmstrip of its neighbours below.

use crate::grid::Movement;
use crate::ui::components::{ExposurePill, FilmChip};
use crate::ui::design::{Design, HEADER_HEIGHT, SPACE_LG, SPACE_MD, glass, motion};
use crate::ui::{
    CloseViewer, NextPhoto, PreviousPhoto, VIEWER_CONTEXT, format,
    gallery::{Gallery, GalleryPhoto, Thumbnail},
};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, WindowControlArea, accesskit, div, img, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use super::framed_image;

/// The width the stage keeps clear for the traffic lights.
const TRAFFIC_LIGHTS_WIDTH: f32 = 80.0;
const STEP_BUTTON_SIZE: f32 = 36.0;
const FILMSTRIP_HEIGHT: f32 = 72.0;
const FILM_FRAME_WIDTH: f32 = 64.0;
const FILM_FRAME_HEIGHT: f32 = 44.0;
const FILM_FRAME_GAP: f32 = 4.0;

impl Gallery {
    pub(in crate::ui) fn viewer(
        &self,
        design: Design,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(index) = self.selected_index() else {
            return div().into_any_element();
        };
        let photo = &self.photos[index];
        let viewer = v_flex()
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
            .bg(design.panel())
            .child(self.viewer_header(design, photo, index, cx))
            .child(self.stage(design, photo, index, cx))
            .child(self.filmstrip(design, index, viewport_width, cx));
        if cx.reduce_motion() {
            viewer.into_any_element()
        } else {
            motion::dialog_in("viewer-entrance", viewer).into_any_element()
        }
    }

    /// The name and position of the photo, centred, with the window's own
    /// controls at the trailing edge. The strip moves the window, as a title
    /// bar does.
    fn viewer_header(
        &self,
        design: Design,
        photo: &GalleryPhoto,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut place = format!(
            "{} of {}",
            format::number(index + 1),
            format::number(self.photos.len())
        );
        if let Some(taken) = &photo.record.metadata.captured_at {
            place = format!("{place} · {}", taken.long_date());
        }
        h_flex()
            .id("viewer-header")
            .window_control_area(WindowControlArea::Drag)
            .relative()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .pl(px(TRAFFIC_LIGHTS_WIDTH))
            .pr(px(SPACE_MD))
            .justify_end()
            .child(
                h_flex()
                    .absolute()
                    .inset_0()
                    .justify_center()
                    .gap(px(8.0))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(design.text)
                            .child(photo.record.file_name()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(design.text_muted)
                            .child(place),
                    ),
            )
            .child(
                h_flex()
                    .gap(px(4.0))
                    .child(
                        Button::new("viewer-reveal")
                            .ghost()
                            .small()
                            .icon(IconName::FolderOpen)
                            .tooltip("Show in Finder")
                            .on_click(cx.listener(|view, _, _, cx| view.reveal_selected(cx))),
                    )
                    .child(
                        Button::new("close-viewer")
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .tooltip("Close (Esc)")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.close_viewer(window, cx)),
                            ),
                    ),
            )
    }

    /// The photo, fitted whole, with step buttons at its sides and the camera
    /// settings floating over its lower edge.
    fn stage(
        &self,
        design: Design,
        photo: &GalleryPhoto,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preview = self.preview_for(&photo.record.id);
        let large = preview.and_then(|preview| preview.image.clone());
        let error = preview.and_then(|preview| preview.error.clone());
        let image: AnyElement = match (&large, &photo.thumbnail) {
            (Some(path), _) => framed_image(path)
                .image_cache(&self.preview_cache)
                .into_any_element(),
            (None, Thumbnail::Ready(path)) => framed_image(path)
                .image_cache(&self.thumbnails_cache)
                .into_any_element(),
            (None, _) => div().into_any_element(),
        };
        let note: Option<SharedString> = error.or_else(|| {
            (large.is_none() && preview.is_some()).then(|| "Loading full size…".into())
        });
        let metadata = &photo.record.metadata;
        let settings = ExposurePill::new(design, metadata).map(|pill| {
            pill.film(
                metadata
                    .film_simulation
                    .map(|film| FilmChip::new(design, film)),
            )
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(STEP_BUTTON_SIZE + SPACE_LG * 2.0))
                    .right(px(STEP_BUTTON_SIZE + SPACE_LG * 2.0))
                    .child(image),
            )
            .child(step_button(
                design,
                "previous-photo",
                "Previous photo (←)",
                IconName::ChevronLeft,
                index > 0,
                cx.listener(|view, _, _, cx| view.move_selection(Movement::Left, cx)),
            ))
            .child(step_button(
                design,
                "next-photo",
                "Next photo (→)",
                IconName::ChevronRight,
                index + 1 < self.photos.len(),
                cx.listener(|view, _, _, cx| view.move_selection(Movement::Right, cx)),
            ))
            .child(
                v_flex()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(SPACE_LG))
                    .items_center()
                    .gap(px(8.0))
                    .when_some(note, |this, note| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(design.text_muted)
                                .child(note),
                        )
                    })
                    .children(settings),
            )
    }

    /// Neighbours of the open photo, centred on it. Clicking one opens it.
    fn filmstrip(
        &self,
        design: Design,
        index: usize,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let range = filmstrip_range(index, self.photos.len(), viewport_width);
        h_flex()
            .flex_none()
            .h(px(FILMSTRIP_HEIGHT))
            .justify_center()
            .gap(px(FILM_FRAME_GAP))
            .border_t_1()
            .border_color(design.border)
            .bg(design.chrome())
            .children(range.map(|position| {
                let photo = &self.photos[position];
                let current = position == index;
                div()
                    .id(SharedString::from(format!("film-{}", photo.record.id)))
                    .role(accesskit::Role::Button)
                    .aria_label(photo.record.file_name())
                    .flex_none()
                    .p(px(2.0))
                    .rounded(px(6.0))
                    .border_2()
                    .border_color(if current {
                        design.text
                    } else {
                        gpui::transparent_black()
                    })
                    .when(!current, |frame| {
                        frame.opacity(0.7).hover(|s| s.opacity(1.0))
                    })
                    .on_click(cx.listener(move |view, _, _, cx| view.select_index(position, cx)))
                    .child(
                        div()
                            .w(px(FILM_FRAME_WIDTH))
                            .h(px(FILM_FRAME_HEIGHT))
                            .rounded(px(3.0))
                            .overflow_hidden()
                            .bg(design.ink(0.35))
                            .when_some(
                                match &photo.thumbnail {
                                    Thumbnail::Ready(path) => Some(path.clone()),
                                    _ => None,
                                },
                                |frame, path| {
                                    frame.child(
                                        img(path)
                                            .size_full()
                                            .object_fit(ObjectFit::Cover)
                                            .image_cache(&self.thumbnails_cache),
                                    )
                                },
                            ),
                    )
            }))
    }
}

/// A round step button pinned to one side of the stage, vertically centred.
fn step_button(
    design: Design,
    id: &'static str,
    label: &'static str,
    icon: IconName,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let leading = matches!(icon, IconName::ChevronLeft);
    let (text, _) = design.overlay_text();
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .when(leading, |side| side.left(px(SPACE_LG)))
        .when(!leading, |side| side.right(px(SPACE_LG)))
        .flex()
        .items_center()
        .child(
            glass::frosted(
                design,
                STEP_BUTTON_SIZE / 2.0,
                div()
                    .id(id)
                    .role(accesskit::Role::Button)
                    .aria_label(label)
                    .size(px(STEP_BUTTON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(text)
                    .child(Icon::new(icon).small())
                    .when(enabled, |button| button.on_click(on_click)),
            )
            .when(!enabled, |button| button.opacity(0.3)),
        )
}

/// The photos the filmstrip shows: as many as fit, centred on `index` and
/// shifted inward at either end of the library.
fn filmstrip_range(index: usize, len: usize, viewport_width: f32) -> std::ops::Range<usize> {
    let slot = FILM_FRAME_WIDTH + 8.0 + FILM_FRAME_GAP;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fits = (((viewport_width - SPACE_LG * 2.0) / slot).floor() as usize).max(1);
    let count = fits.min(len);
    let start = index
        .saturating_sub(count / 2)
        .min(len.saturating_sub(count));
    start..start + count
}

#[cfg(test)]
mod tests {
    use super::filmstrip_range;

    #[test]
    fn the_filmstrip_centres_on_the_open_photo() {
        let range = filmstrip_range(50, 100, 1200.0);
        let middle = range.start + range.len() / 2;
        assert!(middle.abs_diff(50) <= 1, "{range:?}");
    }

    #[test]
    fn the_filmstrip_stays_inside_the_library_at_either_end() {
        assert_eq!(filmstrip_range(0, 100, 1200.0).start, 0);
        assert_eq!(filmstrip_range(99, 100, 1200.0).end, 100);
        assert_eq!(filmstrip_range(2, 5, 1200.0), 0..5);
    }
}
