//! Navigation beside the grid, with a count on each destination.

use crate::ui::design::{Design, SIDEBAR_WIDTH};
use crate::ui::{
    format,
    gallery::{Gallery, LibraryState},
};
use gpui::{
    AnyElement, App, Context, Hsla, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Collapsible as _, Icon, IconName, Sizable as _, h_flex,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarMenu, SidebarMenuItem,
        SidebarToggleButton,
    },
    v_flex,
};

impl Gallery {
    /// Persistent navigation beside the work area. One destination per
    /// collection the library keeps, with its count as a suffix.
    pub(in crate::ui) fn sidebar(
        &self,
        design: Design,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let library_open = matches!(self.library, LibraryState::Open(_));
        let failures = self.failures.len();
        let photos = self.photos.len();
        let total_bytes: u64 = self.photos.iter().map(|photo| photo.record.byte_size).sum();
        let muted = design.text_muted;
        let faint = design.text_faint;
        let warning = cx.theme().warning;
        let footer = SidebarFooter::new()
            .collapsed(self.sidebar_collapsed)
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(Icon::new(IconName::HardDrive).small().text_color(faint))
                    .child(
                        v_flex()
                            .min_w_0()
                            .text_xs()
                            .child(
                                div()
                                    .text_color(muted)
                                    .truncate()
                                    .child(format::library_size(total_bytes)),
                            )
                            .child(
                                div()
                                    .text_color(faint)
                                    .truncate()
                                    .child("Left where they are, not copied"),
                            ),
                    ),
            );
        Sidebar::new("sidebar")
            .w(px(SIDEBAR_WIDTH))
            // Replaces the component's opaque `sidebar` token so this panel sits
            // on the window's blurred backdrop, like the inspector.
            .bg(design.chrome())
            .text_color(design.text)
            .collapsible(SidebarCollapsible::Icon)
            .collapsed(self.sidebar_collapsed)
            .header(
                SidebarToggleButton::new()
                    .collapsed(self.sidebar_collapsed)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_collapsed = !view.sidebar_collapsed;
                        cx.notify();
                    })),
            )
            .child(
                SidebarGroup::new("Library").child(
                    SidebarMenu::new().children([SidebarMenuItem::new("All photos")
                        .icon(Icon::new(IconName::GalleryVerticalEnd).small())
                        .active(true)
                        .disable(!library_open)
                        .suffix(count(photos, faint))]),
                ),
            )
            .when(failures > 0, |this| {
                this.child(
                    SidebarGroup::new("Needs attention").child(
                        SidebarMenu::new().children([SidebarMenuItem::new("Not imported")
                            .icon(
                                Icon::new(IconName::TriangleAlert)
                                    .small()
                                    .text_color(warning),
                            )
                            .suffix(count(failures, faint))
                            .on_click(
                                cx.listener(|view, _, window, cx| view.open_failures(window, cx)),
                            )]),
                    ),
                )
            })
            .footer(footer)
    }
}

/// A menu item's count suffix.
fn count(n: usize, color: Hsla) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| {
        div()
            .text_xs()
            .text_color(color)
            .child(format::number(n))
            .into_any_element()
    }
}
