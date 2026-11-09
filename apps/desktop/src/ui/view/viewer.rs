//! The full-window view of one photo: the photo on the content plane, its name and place in
//! the library above it, the camera settings over it, and a filmstrip of its neighbours below.

use super::super::{
    format,
    gallery::{Gallery, Thumbnail},
    icons,
    look::Look,
};
use super::{TITLE_BAR_HEIGHT, inspector::preview_pixels, parts::exposure_pill};
use crate::grid::Movement;
use rune::{Color, El, Icon, SurfaceFit, TextStyle, Tx, col, el, icon, row, text};
use rune_kit::{IconButton, IconButtonVariant};
use std::ops::Range;

/// The width the header keeps clear for the traffic lights.
const TRAFFIC_LIGHTS_WIDTH: f32 = 80.0;
const STEP_BUTTON_SIZE: f32 = 36.0;
const STAGE_EDGE: f32 = 16.0;
const FILMSTRIP_HEIGHT: f32 = 72.0;
const FILM_FRAME_WIDTH: f32 = 64.0;
const FILM_FRAME_HEIGHT: f32 = 44.0;
const FILM_FRAME_GAP: f32 = 4.0;

pub(super) fn viewer(s: &Gallery, look: &Look) -> Option<El> {
    let index = s.selected_index()?;
    let photo = &s.photos[index];
    let mut place = format!(
        "{} of {}",
        format::number(index + 1),
        format::number(s.photos.len())
    );
    if let Some(taken) = &photo.record.metadata.captured_at {
        place = format!("{place} · {}", taken.long_date());
    }
    let bar = row()
        .key("viewer-bar")
        .w_full()
        .h(TITLE_BAR_HEIGHT)
        .pl(TRAFFIC_LIGHTS_WIDTH)
        .pr(12.0)
        .center_y()
        .justify_end()
        .window_drag()
        .child(
            row()
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .gap(8.0)
                .px(TRAFFIC_LIGHTS_WIDTH + 64.0)
                .child(
                    text(photo.record.file_name())
                        .font(TextStyle::new(13.0, 600.0))
                        .color(look.fg)
                        .truncate(),
                )
                .child(
                    text(place)
                        .font(TextStyle::new(12.0, 400.0).features("'tnum' 1"))
                        .color(look.muted),
                ),
        )
        .child(
            row()
                .gap(4.0)
                .child(
                    IconButton::new(&icons::FOLDER_OPEN)
                        .variant(IconButtonVariant::GhostMuted)
                        .label("Show in Finder")
                        .help("Show in Finder")
                        .on_press(|s: &mut Gallery| s.reveal_selected())
                        .build(look)
                        .key("viewer-reveal"),
                )
                .child(
                    IconButton::new(look.glyph.close)
                        .variant(IconButtonVariant::GhostMuted)
                        .label("Close")
                        .help("Close (Esc)")
                        .on_press(|s: &mut Gallery| s.close_overlay())
                        .build(look)
                        .key("close-viewer"),
                ),
        );
    Some(
        col()
            .key("viewer")
            .role("dialog")
            .label(photo.record.file_name())
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(look.plane)
            .token_motion(look.motion.overlay)
            .modal(false, |s: &mut Gallery| s.close_overlay())
            .child(bar)
            .child(stage(s, look, index))
            .child(filmstrip(s, look, index)),
    )
}

/// The photo, fitted whole, with step buttons at its sides and the camera settings floating
/// over its lower edge.
fn stage(s: &Gallery, look: &Look, index: usize) -> El {
    let photo = &s.photos[index];
    let preview = s
        .preview
        .as_ref()
        .filter(|preview| preview.id == photo.record.id);
    let loading = preview.is_some_and(|preview| preview.image.is_none() && preview.error.is_none())
        || preview
            .and_then(|preview| preview.image.as_deref())
            .is_some_and(|path| s.previews.pixels(path).is_none());
    let note = preview
        .and_then(|preview| preview.error.clone())
        .or_else(|| loading.then(|| "Loading full size…".to_owned()));
    let side = STEP_BUTTON_SIZE + STAGE_EDGE * 2.0;
    el().key("viewer-stage")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            el().key("viewer-image")
                .pin(Some(side), Some(0.0), Some(side), Some(0.0))
                .children(preview_pixels(s, photo)),
        )
        .child(
            step_button(
                look,
                &icons::PREVIOUS,
                "Previous photo",
                index > 0,
                Movement::Left,
            )
            .key("previous-photo"),
        )
        .child(
            step_button(
                look,
                &icons::NEXT,
                "Next photo",
                index + 1 < s.photos.len(),
                Movement::Right,
            )
            .key("next-photo"),
        )
        .child(
            col()
                .pin(Some(0.0), None, Some(0.0), Some(STAGE_EDGE))
                .center()
                .gap(8.0)
                .children(note.map(|note| {
                    text(note)
                        .font(TextStyle::new(12.0, 400.0))
                        .color(look.muted)
                }))
                .children(exposure_pill(look, &photo.record.metadata)),
        )
}

/// A round step button pinned to one side of the stage, vertically centred.
fn step_button(
    look: &Look,
    glyph: &'static Icon,
    label: &'static str,
    enabled: bool,
    movement: Movement,
) -> El {
    let leading = movement == Movement::Left;
    let (left, right) = if leading {
        (Some(STAGE_EDGE), None)
    } else {
        (None, Some(STAGE_EDGE))
    };
    let mut button = el()
        .size(STEP_BUTTON_SIZE, STEP_BUTTON_SIZE)
        .radius(STEP_BUTTON_SIZE / 2.0)
        .center()
        .fill(look.float)
        .border(1.0, look.rule)
        .shadow(
            4.0,
            16.0,
            Color::BLACK.alpha(if look.dark { 0.35 } else { 0.12 }),
        )
        .role("button")
        .label(label)
        .help(label)
        .child(icon(glyph, 16.0).color(look.fg));
    if enabled {
        button = button
            .hover(look.c.hover)
            .press_scale(0.95)
            .cursor(rune::Cursor::Pointer)
            .on_press_tx(move |s: &mut Gallery, tx: &mut Tx| s.move_selection(movement, tx));
    } else {
        button = button.opacity(0.3);
    }
    col()
        .pin(left, Some(0.0), right, Some(0.0))
        .center()
        .child(button)
}

/// Neighbours of the open photo, centred on it. Pressing one opens it.
fn filmstrip(s: &Gallery, look: &Look, index: usize) -> El {
    let range = filmstrip_range(index, s.photos.len(), s.window_size[0]);
    row()
        .key("filmstrip")
        .w_full()
        .h(FILMSTRIP_HEIGHT)
        .center()
        .gap(FILM_FRAME_GAP)
        .fill(look.chrome)
        .border_sides([1.0, 0.0, 0.0, 0.0], look.rule)
        .children(range.map(|position| {
            let photo = &s.photos[position];
            let current = position == index;
            let pixels = match &photo.thumbnail {
                Thumbnail::Ready(path) => s.thumbnails.pixels(path),
                _ => None,
            };
            el().key(format!("film-{}", photo.record.id))
                .role("button")
                .label(photo.record.file_name())
                .checked(current)
                .padding(2.0)
                .radius(6.0)
                .border(2.0, if current { look.fg } else { Color::TRANSPARENT })
                .opacity(if current { 1.0 } else { 0.7 })
                .cursor(rune::Cursor::Pointer)
                .on_press_tx(move |s: &mut Gallery, tx: &mut Tx| s.select_index(position, tx))
                .child(
                    el().size(FILM_FRAME_WIDTH, FILM_FRAME_HEIGHT)
                        .radius(3.0)
                        .fill(look.plate)
                        .children(pixels.map(|pixels| {
                            el().w_full()
                                .h_full()
                                .radius(3.0)
                                .pixels_fit(pixels, SurfaceFit::Fill)
                        })),
                )
        }))
}

/// The photos the filmstrip shows: as many as fit, centred on `index` and shifted inward at
/// either end of the library.
fn filmstrip_range(index: usize, len: usize, viewport_width: f32) -> Range<usize> {
    let slot = FILM_FRAME_WIDTH + 8.0 + FILM_FRAME_GAP;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fits = (((viewport_width - STAGE_EDGE * 2.0) / slot).floor() as usize).max(1);
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
