//! The title strip: what the grid shows, and the window's commands.

use crate::ui::design::{Design, HEADER_HEIGHT, SPACE_LG};
use crate::ui::layout::Layout;
use crate::ui::{
    Import, ImportFromPhotos, format,
    gallery::{Gallery, LibraryState},
};
use gpui::{
    Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _, DropdownButton},
    h_flex,
};

/// The width the title bar keeps clear for the traffic lights.
const TRAFFIC_LIGHTS_WIDTH: f32 = 80.0;

impl Gallery {
    /// One chrome band: the traffic lights, the grid's title over the grid,
    /// and the commands at the trailing edge.
    ///
    /// Two children only: the component lays its children out with
    /// `justify_between` and no gap of its own.
    pub(in crate::ui) fn title_bar(
        &self,
        design: Design,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focus = self.grid_focus.clone();
        let library_open = matches!(self.library, LibraryState::Open(_));
        let performance_label = if self.show_performance {
            "Hide performance"
        } else {
            "Show performance"
        };
        // The title starts where the grid starts, not after the traffic lights.
        let lead = if layout.show_sidebar {
            (layout.sidebar_width - TRAFFIC_LIGHTS_WIDTH).max(0.0) + SPACE_LG
        } else {
            SPACE_LG
        };
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pr(px(10.0))
            .border_color(design.border)
            .child(
                h_flex()
                    .pl(px(lead))
                    .gap(px(8.0))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(design.text)
                            .child("All photos"),
                    )
                    .when(!self.photos.is_empty(), |this| {
                        this.child(
                            div()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(12.0))
                                .text_color(design.text_faint)
                                .child(format::number(self.photos.len())),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap(px(6.0))
                    .child(
                        Button::new("performance")
                            .ghost()
                            .small()
                            .icon(IconName::Cpu)
                            .toggled(self.show_performance)
                            .tooltip(performance_label)
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_performance(cx))),
                    )
                    .child(div().w(px(1.0)).h(px(16.0)).mx(px(4.0)).bg(design.border))
                    .child(
                        DropdownButton::new("import")
                            .primary()
                            .small()
                            .button(
                                Button::new("import-files")
                                    .icon(IconName::Plus)
                                    .label("Import")
                                    .on_click(cx.listener(|view, _, _, cx| view.choose_files(cx))),
                            )
                            .dropdown_menu(move |menu, _, _| {
                                menu.action_context(focus.clone())
                                    .menu("Files or folders…", Box::new(Import))
                                    .menu("From Photos…", Box::new(ImportFromPhotos))
                            })
                            .disabled(self.picking || !library_open),
                    ),
            )
            .when(design.is_glass(), |this| this.bg(design.chrome()))
    }
}
