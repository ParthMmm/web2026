//! The camera settings on one line, floating over a photo.

use super::{
    FilmChip,
    exposure_readout::{ISO, settings},
};
use crate::metadata::PhotoMetadata;
use crate::ui::design::{Design, glass};
use gpui::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div, px,
};
use gpui_component::{ActiveTheme as _, h_flex};

const HEIGHT: f32 = 30.0;

#[derive(IntoElement)]
pub(in crate::ui) struct ExposurePill {
    design: Design,
    values: Vec<SharedString>,
    film: Option<FilmChip>,
}

impl ExposurePill {
    /// `None` when the photo records no exposure settings.
    pub(in crate::ui) fn new(design: Design, metadata: &PhotoMetadata) -> Option<Self> {
        let values: Vec<SharedString> = settings(metadata)
            .into_iter()
            // Alone on a line, a bare ISO number reads as anything; the
            // other values carry their unit.
            .map(|(value, name)| {
                if name == ISO {
                    format!("ISO {value}").into()
                } else {
                    value
                }
            })
            .collect();
        (!values.is_empty()).then_some(Self {
            design,
            values,
            film: None,
        })
    }

    pub(in crate::ui) fn film(mut self, film: Option<FilmChip>) -> Self {
        self.film = film;
        self
    }
}

impl RenderOnce for ExposurePill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let design = self.design;
        let (text, muted) = design.overlay_text();
        let mut row = h_flex()
            .h(px(HEIGHT))
            .px(px(12.0))
            .gap(px(10.0))
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.0))
            .text_color(text);
        for (position, value) in self.values.into_iter().enumerate() {
            if position > 0 {
                row = row.child(div().text_color(muted).child("·"));
            }
            row = row.child(value);
        }
        glass::frosted(design, HEIGHT / 2.0, row.children(self.film))
    }
}
