//! The sheet that lists files that could not be imported, at the window's trailing edge.

use super::super::{gallery::Gallery, look::Look};
use crate::catalog::file_name;
use rune::{Color, Edge, El, ScrollSpec, TextStyle, Transition, col, el, row, text};
use rune_kit::{Button, ButtonVariant, IconButton, IconButtonVariant};

const SHEET_WIDTH: f32 = 440.0;
const SHEET_PADDING: f32 = 16.0;

pub(super) fn failures(s: &Gallery, look: &Look, layer: &mut Vec<El>) {
    let empty = s.failures.is_empty();
    let list: El = if empty {
        text("Every file was imported.")
            .font(look.ty.ui)
            .color(look.muted)
    } else {
        col()
            .w_full()
            .gap(16.0)
            .children(s.failures.iter().map(|failure| {
                col()
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(file_name(&failure.path))
                            .font(TextStyle::new(13.0, 500.0))
                            .color(look.fg)
                            .truncate(),
                    )
                    .child(
                        text(failure.message.clone())
                            .font(look.ty.ui)
                            .color(look.muted)
                            .wrap(),
                    )
                    .child(
                        text(failure.path.display().to_string())
                            .font(look.ty.small)
                            .color(look.muted)
                            .wrap(),
                    )
            }))
    };
    let header = row()
        .key("failure-header")
        .w_full()
        .padding(SHEET_PADDING)
        .center_y()
        .child(
            text("Not imported")
                .font(TextStyle::new(15.0, 600.0))
                .color(look.fg)
                .grow(1.0),
        )
        .child(
            IconButton::new(look.glyph.close)
                .variant(IconButtonVariant::GhostMuted)
                .label("Close")
                .help("Close (Esc)")
                .on_press(|s: &mut Gallery| s.close_overlay())
                .build(look)
                .key("close-failures"),
        );
    let actions = row()
        .key("failure-actions")
        .w_full()
        .gap(8.0)
        .padding(SHEET_PADDING)
        .justify_end()
        .border_sides([1.0, 0.0, 0.0, 0.0], look.rule)
        .child(
            Button::new("Clear list")
                .variant(ButtonVariant::Ghost)
                .disabled(empty)
                .on_press(|s: &mut Gallery| s.clear_failures())
                .build(look)
                .key("clear-failures"),
        )
        .child(
            Button::new("Retry all")
                .disabled(empty)
                .on_press(|s: &mut Gallery| s.retry_failures())
                .build(look)
                .key("retry-failures"),
        );
    layer.push(
        el().key("failures-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(look.c.scrim)
            .token_motion(look.motion.overlay)
            .a11y_hidden(),
    );
    layer.push(
        col()
            .key("failures-sheet")
            .role("dialog")
            .label("Not imported")
            .pin(None, Some(0.0), Some(0.0), Some(0.0))
            .w(SHEET_WIDTH)
            .fill(look.c.surface_raised_base)
            .border_sides([0.0, 0.0, 0.0, 1.0], look.rule)
            .shadow(
                8.0,
                24.0,
                Color::BLACK.alpha(if look.dark { 0.45 } else { 0.16 }),
            )
            .transition(Transition::move_edge(Edge::Trailing))
            .token_motion(look.motion.overlay)
            .modal(true, |s: &mut Gallery| s.close_overlay())
            .child(header)
            .child(
                col()
                    .key("failure-list")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(
                        col()
                            .w_full()
                            .px(SHEET_PADDING)
                            .pb(SHEET_PADDING)
                            .child(list),
                    ),
            )
            .child(actions),
    );
}
