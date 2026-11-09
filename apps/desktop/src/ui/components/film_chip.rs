//! The film simulation the camera applied, as a small outlined label.

use crate::metadata::FilmSimulation;
use crate::ui::design::Design;
use gpui::{
    App, FontWeight, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div, px,
};
use gpui_component::{ActiveTheme as _, h_flex};

#[derive(IntoElement)]
pub(in crate::ui) struct FilmChip {
    design: Design,
    film: FilmSimulation,
}

impl FilmChip {
    pub(in crate::ui) fn new(design: Design, film: FilmSimulation) -> Self {
        Self { design, film }
    }
}

impl RenderOnce for FilmChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let design = self.design;
        // The outer row keeps the chip at its content width inside a
        // full-width value column.
        h_flex().child(
            h_flex()
                .flex_none()
                .h(px(20.0))
                .px(px(7.0))
                .gap(px(6.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(design.border_strong)
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(px(10.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(design.text)
                .child(div().size(px(6.0)).rounded(px(1.0)).bg(design.text_muted))
                .child(self.film.as_str().to_uppercase()),
        )
    }
}
