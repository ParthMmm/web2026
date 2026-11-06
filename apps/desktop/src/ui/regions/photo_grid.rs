//! The photo grid, its tiles, and what the grid area shows with no photos.

use crate::ui::design::{Design, SCROLL_FADE_BAND, color};
use crate::ui::design::{glass, hover};
use crate::ui::gallery::{Gallery, GalleryPhoto, LibraryState, Thumbnail};
use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    Window, div, img, prelude::FluentBuilder as _, px, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, alert::Alert, button::Button,
    h_flex, scroll::ScrollableElement as _, v_flex,
};

use crate::ui::layout::{GRID_GAP, GRID_GUTTER, Layout};

const TILE_RADIUS: f32 = 4.0;
/// The gap between a tile and its selection ring.
const RING_OFFSET: f32 = 3.0;

impl Gallery {
    fn rows(
        &self,
        design: Design,
        range: std::ops::Range<usize>,
        layout: &Layout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let columns = layout.columns;
        range
            .map(|row| {
                let start = row * columns;
                let end = (start + columns).min(self.photos.len());
                h_flex()
                    .w_full()
                    .h(px(layout.row_height()))
                    .pt(px(GRID_GAP))
                    .px(px(GRID_GUTTER))
                    .gap(px(GRID_GAP))
                    .children(
                        self.photos[start..end]
                            .iter()
                            .map(|photo| self.tile(design, photo, layout, window, cx)),
                    )
            })
            .collect()
    }

    fn tile(
        &self,
        design: Design,
        photo: &GalleryPhoto,
        layout: &Layout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected.as_ref() == Some(&photo.record.id);
        let id = photo.record.id.clone();
        // The element id doubles as the hover key, so no string is built per
        // tile per frame.
        let hover_key = photo.element_id.clone();
        let theme = cx.theme();
        let muted_foreground = theme.muted_foreground;
        let warning = theme.warning;
        let hover_t = hover::hover_t(&hover_key);
        let focused = self.grid_focus.is_focused(window);
        let (rim, halo) = glass::selection_ring(design, selected, focused, hover_t);
        let missing = photo.record.source_missing;
        let frame = div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .rounded(px(TILE_RADIUS))
            .bg(design.ink(0.35));
        let frame = match &photo.thumbnail {
            Thumbnail::Ready(path) => frame.child(
                img(path.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .grayscale(missing)
                    .when(missing, |image| image.opacity(0.4))
                    .image_cache(&self.thumbnails_cache),
            ),
            // The plate alone. An animated skeleton here would keep the window
            // drawing frames for as long as any preview is pending.
            Thumbnail::Pending => frame,
            Thumbnail::Failed => frame.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child(Icon::new(IconName::TriangleAlert).small())
                    .child("Preview unavailable"),
            ),
        };
        // The hover plate rides above the image and fades in. Skip it at rest:
        // a child added for every tile on every frame costs layout and paint
        // for nothing.
        let frame = frame
            .when(hover_t > 0.0, |frame| {
                frame.child(
                    glass::light_plate(design, hover_t)
                        .apply(div().absolute().inset_0().rounded(px(TILE_RADIUS))),
                )
            })
            .when(missing, |frame| {
                frame.child(
                    h_flex()
                        .absolute()
                        .left(px(6.0))
                        .bottom(px(6.0))
                        .h(px(20.0))
                        .px(px(6.0))
                        .gap(px(4.0))
                        .rounded(px(5.0))
                        .bg(color::grey(10).opacity(0.62))
                        .text_size(px(10.5))
                        .text_color(color::grey(236))
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .xsmall()
                                .text_color(warning),
                        )
                        .child("Original missing"),
                )
            });
        div()
            .id(photo.element_id.clone())
            .relative()
            .flex_none()
            .w(layout.tile_width)
            .h(layout.tile_height)
            .on_hover(cx.listener(move |_, hovered: &bool, _, cx| {
                // The enter and the leave both have to be recorded: gpui's
                // with_animation clock replays on remount, so the fade must not
                // go through it. Only a real change asks for a redraw, so a
                // pointer sweep across the grid costs two paints per tile, not
                // one per move event.
                if hover::set_hover(&hover_key, *hovered, cx.reduce_motion()) {
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
                if view.benchmark.is_some() {
                    return;
                }
                window.focus(&view.grid_focus, cx);
                view.select_id(&id, cx);
                if event.click_count() >= 2 {
                    view.open_viewer(window, cx);
                }
            }))
            .child(frame)
            // The ring stands off the photo instead of covering its edge.
            .when(rim.a > 0.0, |tile| {
                tile.child(
                    div()
                        .absolute()
                        .top(px(-RING_OFFSET))
                        .left(px(-RING_OFFSET))
                        .right(px(-RING_OFFSET))
                        .bottom(px(-RING_OFFSET))
                        .rounded(px(TILE_RADIUS + RING_OFFSET))
                        .border_2()
                        .border_color(rim)
                        .shadow(halo),
                )
            })
            .into_any_element()
    }
    pub(in crate::ui) fn grid(
        &self,
        design: Design,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &self.library {
            LibraryState::Opening => return centered_note("Opening library…", cx),
            LibraryState::Unavailable(message) => {
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .p_8()
                    .child(
                        Alert::error("library-unavailable", message.clone())
                            .title("The library couldn't be opened"),
                    )
                    .into_any_element();
            }
            LibraryState::Open(_) => {}
        }
        if self.photos.is_empty() {
            if self.is_importing() {
                return centered_note("Looking for JPEGs…", cx);
            }
            return self.empty_state(cx);
        }
        let entity = cx.entity();
        let columns = layout.columns;
        let geometry = *layout;
        glass::edge_scrim(
            design.panel(),
            SCROLL_FADE_BAND,
            true,
            true,
            uniform_list(
                "photo-grid",
                self.photos.len().div_ceil(columns),
                move |range, window, cx| {
                    entity.update(cx, |view, cx| {
                        view.rows(design, range, &geometry, window, cx)
                    })
                },
            )
            .track_scroll(&self.scroll)
            .size_full(),
        )
        .vertical_scrollbar(&self.scroll)
        .into_any_element()
    }

    fn empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .p_8()
            .child(
                Icon::new(IconName::GalleryVerticalEnd)
                    .large()
                    .text_color(theme.muted_foreground),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("No photos yet"),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        "Import JPEGs from folders or from Photos. Originals stay where they are.",
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("empty-import-files")
                            .label("Import files…")
                            .disabled(self.picking)
                            .on_click(cx.listener(|view, _, _, cx| view.choose_files(cx))),
                    )
                    .child(
                        Button::new("empty-import-photos")
                            .label("Import from Photos…")
                            .disabled(self.picking)
                            .on_click(cx.listener(|view, _, _, cx| view.choose_from_photos(cx))),
                    ),
            )
            .into_any_element()
    }
}

fn centered_note(text: &'static str, cx: &mut Context<Gallery>) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}
