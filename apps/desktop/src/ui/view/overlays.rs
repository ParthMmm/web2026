//! What covers the library: the full-window viewer and the sheet of files that didn't import.

use super::super::{format, gallery::Gallery, icons};
use super::{TABULAR, inspector::preview_pixels};
use crate::{catalog::file_name, grid::Movement};
use rune::{Color, El, ScrollSpec, Tx, col, el, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Dialog, IconButton, IconButtonVariant, Kit};

const SHEET_WIDTH: f32 = 440.0;
/// A list row's inset; the dialog's 16 pt edge less this keeps the text on
/// the title's line.
const LIST_INSET: f32 = 14.0;

/// The full-window viewer. Always dark, whatever the system appearance: the
/// photograph is the content and the chrome recedes from it.
pub(super) fn viewer(s: &Gallery) -> Option<El> {
    let k = Kit::neutral(true);
    let index = s.selected_index()?;
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
    let step = |glyph, label: &'static str, disabled: bool, movement: Movement| {
        IconButton::new(glyph)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Large)
            .label(label)
            .help(label)
            .disabled(disabled)
            .on_press_tx(move |s: &mut Gallery, tx: &mut Tx| s.move_selection(movement, tx))
            .build(&k)
    };
    let bar = row()
        .key("viewer-bar")
        .w_full()
        .h(44.0)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .child(
            IconButton::new(k.glyph.close)
                .variant(IconButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .label("Close")
                .help("Close (Esc)")
                .on_press(|s: &mut Gallery| s.close_overlay())
                .build(&k)
                .key("close-viewer"),
        )
        .child(
            text(photo.record.file_name())
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .truncate()
                .grow(1.0),
        )
        .child(
            text(format!(
                "{} of {}",
                format::number(index + 1),
                format::number(s.photos.len())
            ))
            .font(k.ty.small.features(TABULAR))
            .color(k.c.text_muted),
        )
        .child(
            step(
                &icons::PREVIOUS,
                "Previous photo",
                index == 0,
                Movement::Left,
            )
            .key("previous-photo"),
        )
        .child(
            step(
                &icons::NEXT,
                "Next photo",
                index + 1 == s.photos.len(),
                Movement::Right,
            )
            .key("next-photo"),
        );
    let image = el()
        .key("viewer-image")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .mt(12.0)
        .children(preview_pixels(s, photo));
    Some(
        col()
            .key("viewer")
            .role("dialog")
            .label(photo.record.file_name())
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(12.0)
            .fill(Color::BLACK.alpha(0.94))
            .token_motion(k.motion.overlay)
            .modal(false, |s: &mut Gallery| s.close_overlay())
            .child(bar)
            .child(image)
            .children(note.map(|note| text(note).font(k.ty.small).color(k.c.text_muted).pt(8.0))),
    )
}

pub(super) fn failures(s: &Gallery, k: &Kit, layer: &mut Vec<El>) {
    let empty = s.failures.is_empty();
    let last = s.failures.len().saturating_sub(1);
    let list: El = if empty {
        text("Every file was imported.")
            .font(k.ty.ui)
            .color(k.c.text_muted)
            .px(LIST_INSET)
    } else {
        col()
            .w_full()
            .children(s.failures.iter().enumerate().map(|(index, failure)| {
                let rule = if index == last {
                    Color::TRANSPARENT
                } else {
                    k.c.border_weak
                };
                col()
                    .w_full()
                    .px(LIST_INSET)
                    .py(12.0)
                    .gap(2.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], rule)
                    .child(
                        text(file_name(&failure.path))
                            .font(k.ty.ui_medium)
                            .color(k.c.text_base)
                            .truncate(),
                    )
                    .child(
                        text(failure.message.clone())
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .wrap(),
                    )
                    .child(
                        text(failure.path.display().to_string())
                            .font(k.ty.small)
                            .color(k.c.text_faint)
                            .wrap(),
                    )
            }))
    };
    let actions = row()
        .key("failure-actions")
        .w_full()
        .gap(8.0)
        .padding(16.0)
        .justify_end()
        .child(
            Button::new("Clear list")
                .variant(ButtonVariant::Ghost)
                .disabled(empty)
                .on_press(|s: &mut Gallery| s.clear_failures())
                .build(k)
                .key("clear-failures"),
        )
        .child(
            Button::new("Retry all")
                .variant(ButtonVariant::Contrast)
                .disabled(empty)
                .on_press(|s: &mut Gallery| s.retry_failures())
                .build(k)
                .key("retry-failures"),
        );
    Dialog::new("failures", "Not imported")
        .open(true)
        .width(SHEET_WIDTH)
        .on_close(|s: &mut Gallery| s.close_overlay())
        .child(
            col()
                .key("failure-list")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(col().w_full().px(16.0 - LIST_INSET).child(list)),
        )
        .child(actions)
        .build(k, layer);
}
