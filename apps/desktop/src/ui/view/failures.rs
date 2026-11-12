//! The sheets that list what didn't make it, at the window's trailing edge: files that could
//! not be imported, and photos whose upload stopped.

use super::super::{gallery::Gallery, look::Look};
use crate::catalog::file_name;
use rune::{Color, Edge, El, ScrollSpec, TextStyle, Transition, col, el, row, text};
use rune_kit::{Button, ButtonVariant, IconButton, IconButtonVariant};

const SHEET_WIDTH: f32 = 440.0;
const SHEET_PADDING: f32 = 16.0;

pub(super) fn failures(s: &Gallery, look: &Look, layer: &mut Vec<El>) {
    let empty = s.failures.is_empty();
    let entries = s.failures.iter().map(|failure| {
        entry(
            look,
            file_name(&failure.path),
            failure.message.clone(),
            Some(failure.path.display().to_string()),
        )
    });
    let list = list(look, "Every file was imported.", entries);
    let actions = vec![
        Button::new("Clear list")
            .variant(ButtonVariant::Ghost)
            .disabled(empty)
            .on_press(|s: &mut Gallery| s.clear_failures())
            .build(look)
            .key("clear-failures"),
        Button::new("Retry all")
            .disabled(empty)
            .on_press(|s: &mut Gallery| s.retry_failures())
            .build(look)
            .key("retry-failures"),
    ];
    sheet(look, "", "Not imported", list, actions, layer);
}

pub(super) fn upload_failures(s: &Gallery, look: &Look, layer: &mut Vec<El>) {
    let retryable = s
        .upload_failures()
        .any(|(_, failure)| failure.retry_helps());
    let entries = s.upload_failures().filter_map(|(id, failure)| {
        let photo = s.photos.iter().find(|photo| &photo.record.id == id)?;
        Some(entry(
            look,
            photo.record.file_name(),
            failure.message(),
            None,
        ))
    });
    let list = list(look, "Every upload finished.", entries);
    let actions = vec![
        Button::new("Retry all")
            .disabled(!retryable || !s.is_connected())
            .on_press(|s: &mut Gallery| s.retry_uploads())
            .build(look)
            .key("retry-uploads"),
    ];
    sheet(look, "upload-", "Not uploaded", list, actions, layer);
}

/// One file or photo: its name, what went wrong, and where it is.
fn entry(look: &Look, name: String, message: String, path: Option<String>) -> El {
    col()
        .w_full()
        .gap(4.0)
        .child(
            text(name)
                .font(TextStyle::new(13.0, 500.0))
                .color(look.fg)
                .truncate(),
        )
        .child(text(message).font(look.ty.ui).color(look.muted).wrap())
        .children(path.map(|path| text(path).font(look.ty.small).color(look.muted).wrap()))
}

fn list(look: &Look, none: &'static str, entries: impl Iterator<Item = El>) -> El {
    let entries: Vec<El> = entries.collect();
    if entries.is_empty() {
        text(none).font(look.ty.ui).color(look.muted)
    } else {
        col().w_full().gap(16.0).children(entries)
    }
}

/// A modal sheet titled `title`, with `list` and `actions`. `prefix` keeps each sheet's keys
/// apart.
fn sheet(
    look: &Look,
    prefix: &str,
    title: &'static str,
    list: El,
    actions: Vec<El>,
    layer: &mut Vec<El>,
) {
    let header = row()
        .key(format!("{prefix}failure-header"))
        .w_full()
        .padding(SHEET_PADDING)
        .center_y()
        .child(
            text(title)
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
                .key(format!("close-{prefix}failures")),
        );
    let actions = row()
        .key(format!("{prefix}failure-actions"))
        .w_full()
        .gap(8.0)
        .padding(SHEET_PADDING)
        .justify_end()
        .border_sides([1.0, 0.0, 0.0, 0.0], look.rule)
        .children(actions);
    layer.push(
        el().key(format!("{prefix}failures-scrim"))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(look.c.scrim)
            .token_motion(look.motion.overlay)
            .a11y_hidden(),
    );
    layer.push(
        col()
            .key(format!("{prefix}failures-sheet"))
            .role("dialog")
            .label(title)
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
                    .key(format!("{prefix}failure-list"))
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
