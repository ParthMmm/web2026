//! Label and value rows, labels in a fixed column.

use crate::ui::design::Design;
use gpui::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};

const LABEL_WIDTH: f32 = 72.0;

#[derive(IntoElement)]
pub(in crate::ui) struct DetailList {
    design: Design,
    rows: Vec<(SharedString, AnyElement)>,
}

impl DetailList {
    pub(in crate::ui) fn new(design: Design) -> Self {
        Self {
            design,
            rows: Vec::new(),
        }
    }

    /// Add a row. Absent values are skipped by the caller, never shown as a
    /// placeholder.
    pub(in crate::ui) fn row(
        mut self,
        label: impl Into<SharedString>,
        value: impl IntoElement,
    ) -> Self {
        self.rows.push((label.into(), value.into_any_element()));
        self
    }
}

impl RenderOnce for DetailList {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let design = self.design;
        v_flex()
            .w_full()
            .text_size(px(12.0))
            .children(self.rows.into_iter().map(|(label, value)| {
                h_flex()
                    .items_start()
                    .gap(px(12.0))
                    .py(px(5.0))
                    .child(
                        div()
                            .flex_none()
                            .w(px(LABEL_WIDTH))
                            .text_color(design.text_faint)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(design.text)
                            .child(value),
                    )
            }))
    }
}
