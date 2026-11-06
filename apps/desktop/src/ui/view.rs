use super::design::hover;
use super::design::{ActiveDesign as _, Design};
use super::layout::{Layout, LayoutInput};
use super::{
    CloseWindow, GRID_CONTEXT, Import, ImportFromPhotos, Minimize, OpenViewer, RevealInFinder,
    SelectDown, SelectFirst, SelectLast, SelectLeft, SelectPageDown, SelectPageUp, SelectRight,
    SelectUp, ShowImportFailures, ToggleFullScreen, ToggleInspector, TogglePerformance,
    ToggleSidebar, Zoom, ZoomIn, ZoomOut, benchmark, gallery::Gallery,
};
use crate::grid::Movement;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    Root, h_flex,
    menu::{ContextMenuExt as _, PopupMenu},
    v_flex,
};

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.benchmark_frame(window, cx);
        let layout = Layout::compute(LayoutInput {
            viewport_width: window.viewport_size().width.as_f32(),
            benchmarking: self.benchmark.is_some(),
            sidebar_hidden: self.sidebar_hidden,
            inspector_hidden: self.inspector_hidden,
            has_photos: !self.photos.is_empty(),
            min_tile_width: self.min_tile_width,
        });
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let page_rows =
            (window.viewport_size().height.as_f32() / layout.row_height()).floor() as usize;
        self.metrics.columns = layout.columns;
        self.metrics.page_rows = page_rows.saturating_sub(1).max(1);

        let design = cx.design();
        // One frame's worth of hover bookkeeping, before any tile reads its
        // progress. Entries that go a full frame unread are dropped, which is
        // what keeps a tile that unmounted mid-hover from keeping a wash.
        hover::maintain(cx.entity().entity_id(), cx);
        // No root background: the chrome is translucent and must sit on the
        // window's blurred backdrop, not on an opaque plane that hides it.
        v_flex()
            .id("gallery")
            .relative()
            .size_full()
            .text_color(design.text)
            .on_action(cx.listener(|view, _: &Import, _, cx| view.choose_files(cx)))
            .on_action(cx.listener(|view, _: &ImportFromPhotos, _, cx| view.choose_from_photos(cx)))
            .on_action(cx.listener(|view, _: &ShowImportFailures, window, cx| {
                view.open_failures(window, cx);
            }))
            .on_action(
                cx.listener(|view, _: &TogglePerformance, _, cx| view.toggle_performance(cx)),
            )
            .on_action(cx.listener(|view, _: &ToggleSidebar, _, cx| view.toggle_sidebar(cx)))
            .on_action(cx.listener(|view, _: &ToggleInspector, _, cx| view.toggle_inspector(cx)))
            .on_action(cx.listener(|view, _: &ZoomIn, window, cx| view.zoom(1.0, window, cx)))
            .on_action(cx.listener(|view, _: &ZoomOut, window, cx| view.zoom(-1.0, window, cx)))
            .on_action(cx.listener(|view, _: &RevealInFinder, _, cx| view.reveal_selected(cx)))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &Zoom, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .child(self.title_bar(design, &layout, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(layout.show_sidebar, |this| {
                        this.child(self.sidebar(design, cx))
                    })
                    .child(
                        div()
                            .id("photo-grid-region")
                            .key_context(GRID_CONTEXT)
                            .track_focus(&self.grid_focus)
                            .on_action(movement::<SelectLeft>(Movement::Left, cx))
                            .on_action(movement::<SelectRight>(Movement::Right, cx))
                            .on_action(movement::<SelectUp>(Movement::Up, cx))
                            .on_action(movement::<SelectDown>(Movement::Down, cx))
                            .on_action(movement::<SelectPageUp>(Movement::PageUp, cx))
                            .on_action(movement::<SelectPageDown>(Movement::PageDown, cx))
                            .on_action(movement::<SelectFirst>(Movement::First, cx))
                            .on_action(movement::<SelectLast>(Movement::Last, cx))
                            .on_action(cx.listener(|view, _: &OpenViewer, window, cx| {
                                view.open_viewer(window, cx);
                            }))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            // The one opaque plane: the gaps between tiles must
                            // not show the desktop.
                            .bg(design.panel())
                            .child(self.grid_with_menu(design, &layout, cx)),
                    )
                    .when(layout.show_inspector, |this| {
                        this.child(self.inspector(design, cx))
                    }),
            )
            .child(self.status_bar(design, cx))
            .when(self.viewer_open, |this| this.child(self.viewer(design, cx)))
            .when(self.show_performance, |this| {
                this.child(gpui_fps::fps_monitor(window, cx))
            })
            .children(
                self.benchmark
                    .as_ref()
                    .map(benchmark::Benchmark::present_probe),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

fn movement<A: gpui::Action>(
    movement: Movement,
    cx: &mut Context<Gallery>,
) -> impl Fn(&A, &mut Window, &mut gpui::App) + 'static {
    cx.listener(move |view, _: &A, _, cx| view.move_selection(movement, cx))
}

impl Gallery {
    /// The grid, with the photo menu on right click once there are photos.
    fn grid_with_menu(
        &self,
        design: Design,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grid = self.grid(design, layout, cx);
        if self.photos.is_empty() {
            return grid.into_any_element();
        }
        div()
            .size_full()
            .child(grid)
            .context_menu(photo_menu(self.grid_focus.clone(), self.inspector_hidden))
            .into_any_element()
    }
}

/// The menu a right click on a photo opens. The tile has already selected
/// itself, so each item acts on the selection.
fn photo_menu(
    focus: gpui::FocusHandle,
    inspector_hidden: bool,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, _| {
        menu.action_context(focus.clone())
            .menu("Open", Box::new(OpenViewer))
            .menu(
                if inspector_hidden {
                    "Show Info"
                } else {
                    "Hide Info"
                },
                Box::new(ToggleInspector),
            )
            .separator()
            .menu("Show in Finder", Box::new(RevealInFinder))
    }
}
