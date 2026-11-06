//! The camera-settings strip: one cell per recorded setting, value over name.

use crate::metadata::PhotoMetadata;
use crate::ui::design::Design;
use gpui::{
    App, FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

const RADIUS: f32 = 7.0;

#[derive(IntoElement)]
pub(in crate::ui) struct ExposureReadout {
    design: Design,
    cells: Vec<(SharedString, &'static str)>,
}

impl ExposureReadout {
    /// `None` when the photo records no exposure settings: an empty strip of
    /// dashes would claim values the camera never wrote.
    pub(in crate::ui) fn new(design: Design, metadata: &PhotoMetadata) -> Option<Self> {
        let cells = settings(metadata);
        (!cells.is_empty()).then_some(Self { design, cells })
    }
}

/// The recorded exposure settings, each with its short name, in the order a
/// camera's display shows them. Settings the camera did not write are left
/// out rather than shown as a dash.
/// The short name of the ISO setting, which a bare number does not explain.
pub(in crate::ui) const ISO: &str = "ISO";

pub(in crate::ui) fn settings(metadata: &PhotoMetadata) -> Vec<(SharedString, &'static str)> {
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
    .map(|(value, name)| (value.into(), name))
    .collect()
}

impl RenderOnce for ExposureReadout {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mono = cx.theme().mono_font_family.clone();
        let design = self.design;
        h_flex()
            .w_full()
            .rounded(px(RADIUS))
            .bg(design
                .ink(0.35)
                .opacity(if design.is_dark() { 1.0 } else { 0.4 }))
            .border_1()
            .border_color(design.border)
            .font_family(mono)
            .children(
                self.cells
                    .into_iter()
                    .enumerate()
                    .map(|(index, (value, name))| {
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .py(px(8.0))
                            .gap(px(2.0))
                            .when(index > 0, |cell| {
                                cell.border_l_1().border_color(design.border)
                            })
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(design.text)
                                    .truncate()
                                    .child(value),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(design.text_muted)
                                    .child(name),
                            )
                    }),
            )
    }
}
