//! The bottom strip: what is in view and selected, transient status, and
//! import progress.

use crate::ui::design::{Design, STATUS_BAR_HEIGHT};
use crate::ui::{format, gallery::Gallery};
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, accesskit, div, px,
};
use gpui_component::{ActiveTheme as _, h_flex, progress::Progress};

const PROGRESS_WIDTH: f32 = 120.0;

impl Gallery {
    pub(in crate::ui) fn status_bar(
        &self,
        design: Design,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mono = cx.theme().mono_font_family.clone();
        let progress = self.progress.map(|(processed, total)| {
            let scanning = total == 0;
            #[allow(clippy::cast_precision_loss)]
            let percent = if scanning {
                0.0
            } else {
                processed as f32 * 100.0 / total as f32
            };
            let count = (!scanning).then(|| {
                div().font_family(mono.clone()).child(format!(
                    "{} of {}",
                    format::number(processed),
                    format::number(total)
                ))
            });
            h_flex()
                .id("import-progress-group")
                .role(accesskit::Role::Group)
                .aria_label("Import progress")
                .gap(px(10.0))
                .items_center()
                .child(if scanning {
                    "Looking for JPEGs…"
                } else {
                    "Importing"
                })
                .children(count)
                .child(
                    Progress::new("import-progress")
                        .loading(scanning)
                        .value(percent)
                        .w(px(PROGRESS_WIDTH)),
                )
        });
        // A transient message wins; at rest the strip says what is in view.
        let left = if self.status.is_empty() {
            div()
                .font_family(mono)
                .child(self.selection_summary())
                .into_any_element()
        } else {
            div()
                .truncate()
                .child(self.status.clone())
                .into_any_element()
        };
        h_flex()
            .flex_none()
            .h(px(STATUS_BAR_HEIGHT))
            .px(px(14.0))
            .gap(px(10.0))
            .text_size(px(11.5))
            .text_color(design.text_muted)
            .bg(design.chrome())
            .border_t_1()
            .border_color(design.border)
            .child(div().flex_1().min_w_0().child(left))
            .children(progress)
    }

    fn selection_summary(&self) -> String {
        let photos = format::count(self.photos.len(), "photo", "photos");
        if self.selected.is_some() {
            format!("{photos} · 1 selected")
        } else {
            photos
        }
    }
}
