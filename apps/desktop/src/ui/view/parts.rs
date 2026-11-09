//! Display pieces that keep no state: the camera-settings readout and pill, the film chip,
//! and the detail list. Each takes plain data and the look, so a region decides what to show
//! and a part decides only how it looks.

use super::super::look::{Look, MONO};
use crate::metadata::{FilmSimulation, PhotoMetadata};
use rune::{Color, El, TextStyle, col, el, row, text};

/// The short name of the ISO setting, which a bare number does not explain.
const ISO: &str = "ISO";
const READOUT_RADIUS: f32 = 7.0;
const PILL_HEIGHT: f32 = 30.0;
const DETAIL_LABEL_WIDTH: f32 = 72.0;
const DETAIL: TextStyle = TextStyle::new(12.0, 400.0);

/// The recorded exposure settings, each with its short name, in the order a camera's display
/// shows them. Settings the camera did not write are left out rather than shown as a dash.
fn settings(metadata: &PhotoMetadata) -> Vec<(String, &'static str)> {
    [
        metadata.aperture().map(|value| (value, "APERTURE")),
        metadata
            .exposure_time
            .map(|time| (time.to_string(), "SHUTTER")),
        metadata.iso.map(|iso| (iso.to_string(), ISO)),
        metadata.focal_length().map(|value| (value, "FOCAL")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The camera-settings strip: one cell per recorded setting, value over name. `None` when the
/// photo records none: a strip of dashes would claim values the camera never wrote.
pub(super) fn exposure_readout(look: &Look, metadata: &PhotoMetadata) -> Option<El> {
    let cells = settings(metadata);
    if cells.is_empty() {
        return None;
    }
    Some(
        row()
            .key("exposure-readout")
            .role("group")
            .label("Exposure")
            .w_full()
            .radius(READOUT_RADIUS)
            .fill(look.c.layer_02)
            .border(1.0, look.rule)
            .children(cells.into_iter().enumerate().map(|(index, (value, name))| {
                let rule = if index == 0 {
                    Color::TRANSPARENT
                } else {
                    look.rule
                };
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .center()
                    .py(8.0)
                    .gap(2.0)
                    .border_sides([0.0, 0.0, 0.0, 1.0], rule)
                    .child(
                        text(value)
                            .font(TextStyle::new(12.5, 500.0).mono().features("'tnum' 1"))
                            .color(look.fg)
                            .truncate(),
                    )
                    .child(
                        text(name)
                            .font(TextStyle::new(9.5, 400.0).mono())
                            .color(look.muted),
                    )
            })),
    )
}

/// The camera settings on one line, floating over a photo, with the film chip at its end.
pub(super) fn exposure_pill(look: &Look, metadata: &PhotoMetadata) -> Option<El> {
    let values = settings(metadata);
    if values.is_empty() {
        return None;
    }
    let mut pill = row()
        .key("exposure-pill")
        .role("group")
        .label("Exposure")
        .h(PILL_HEIGHT)
        .px(12.0)
        .gap(10.0)
        .center_y()
        .radius(PILL_HEIGHT / 2.0)
        .fill(look.float)
        .border(1.0, look.rule)
        .shadow(
            4.0,
            16.0,
            Color::BLACK.alpha(if look.dark { 0.35 } else { 0.12 }),
        );
    for (position, (value, name)) in values.into_iter().enumerate() {
        if position > 0 {
            pill = pill.child(text("·").font(MONO).color(look.muted).a11y_hidden());
        }
        // Alone on a line, a bare ISO number reads as anything; the other values carry their unit.
        let value = if name == ISO {
            format!("ISO {value}")
        } else {
            value
        };
        pill = pill.child(text(value).font(MONO).color(look.fg));
    }
    Some(pill.children(metadata.film_simulation.map(|film| film_chip(look, film))))
}

/// The film simulation the camera applied, as a small outlined label.
pub(super) fn film_chip(look: &Look, film: FilmSimulation) -> El {
    row()
        .key("film-chip")
        .h(20.0)
        .px(7.0)
        .gap(6.0)
        .center_y()
        .radius(4.0)
        .border(1.0, look.rule_strong)
        .child(
            el().size(6.0, 6.0)
                .radius(1.0)
                .fill(look.muted)
                .a11y_hidden(),
        )
        .child(
            text(film.as_str().to_uppercase())
                .font(TextStyle::new(10.0, 500.0).mono())
                .color(look.fg),
        )
}

/// One row of a detail list: the label in a fixed column, then its value.
pub(super) fn detail(look: &Look, label: &'static str, value: El) -> El {
    row()
        .w_full()
        .gap(12.0)
        .py(5.0)
        .child(
            text(label)
                .font(DETAIL)
                .color(look.faint)
                .w(DETAIL_LABEL_WIDTH),
        )
        .child(col().grow(1.0).min_w(0.0).child(value))
}

/// A detail's value as plain words.
pub(super) fn detail_text(value: impl Into<String>, color: Color) -> El {
    text(value.into()).font(DETAIL).color(color).wrap()
}
