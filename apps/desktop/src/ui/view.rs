use super::design::{
    ActiveDesign as _, CONTROL_RADIUS, Design, HEADER_HEIGHT, PANEL_RADIUS, SCROLL_FADE_BAND,
    SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH, SPACE_LG, SPACE_MD, SPACE_SM, motion,
};
use super::design::{glass, hover};
use super::{
    APP_NAME, CloseViewer, GRID_CONTEXT, Import, ImportFromPhotos, NextPhoto, OpenViewer,
    PreviousPhoto, SelectDown, SelectFirst, SelectLast, SelectLeft, SelectPageDown, SelectPageUp,
    SelectRight, SelectUp, ShowImportFailures, TogglePerformance, VIEWER_CONTEXT, benchmark,
    format,
    gallery::{Gallery, GalleryPhoto, LibraryState, Thumbnail},
};
use crate::{PhotoId, grid::Movement};
use gpui_kit::component::{
    ActiveTheme as _, Collapsible as _, Disableable as _, Icon, IconName, Root, Sizable as _,
    TitleBar, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants as _, DropdownButton},
    description_list::DescriptionList,
    h_flex,
    progress::Progress,
    scroll::ScrollableElement as _,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarMenu, SidebarMenuItem,
        SidebarToggleButton,
    },
    skeleton::Skeleton,
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Window, div, img, prelude::FluentBuilder as _, px, uniform_list,
};
use std::path::Path;

// The grid is a `uniform_list`, which needs exact row heights, so its
// geometry is computed here rather than left to flex layout.
const GRID_GAP: f32 = SPACE_SM;
const MIN_TILE_WIDTH: f32 = 168.0;
const TILE_ASPECT: f32 = 0.75;
const INSPECTOR_WIDTH: f32 = 320.0;
const MIN_WIDTH_FOR_INSPECTOR: f32 = 760.0;
const PROGRESS_WIDTH: f32 = 120.0;
const INSPECTOR_PREVIEW_HEIGHT: f32 = 220.0;
const SHEET_WIDTH: f32 = 440.0;

#[derive(Clone, Copy)]
struct Layout {
    columns: usize,
    tile_width: Pixels,
    tile_height: Pixels,
    show_sidebar: bool,
    show_inspector: bool,
}

impl Gallery {
    fn layout(&self, window: &Window) -> Layout {
        let viewport = window.viewport_size();
        let benchmarking = self.benchmark.is_some();
        // The benchmark's scripted scroll assumes a fixed column count, and its
        // preview indices come from `benchmark.py`'s warm preparation. Hide the
        // sidebar there so the geometry stays exactly what the script measured.
        let show_sidebar = !benchmarking;
        let sidebar = if self.sidebar_collapsed {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            SIDEBAR_WIDTH
        };
        let show_inspector = benchmarking
            || (!self.photos.is_empty() && viewport.width.as_f32() >= MIN_WIDTH_FOR_INSPECTOR);
        let inspector = if show_inspector { INSPECTOR_WIDTH } else { 0.0 };
        let taken = if show_sidebar { sidebar } else { 0.0 } + inspector;
        let usable = (viewport.width.as_f32() - taken - GRID_GAP * 2.0).max(MIN_TILE_WIDTH);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = if benchmarking {
            benchmark::COLUMNS
        } else {
            (((usable + GRID_GAP) / (MIN_TILE_WIDTH + GRID_GAP)).floor() as usize).max(1)
        };
        #[allow(clippy::cast_precision_loss)]
        let tile_width = (usable - GRID_GAP * (columns - 1) as f32) / columns as f32;
        let tile_height = (tile_width * TILE_ASPECT).floor();
        Layout {
            columns,
            tile_width: px(tile_width.floor()),
            tile_height: px(tile_height),
            show_sidebar,
            show_inspector,
        }
    }

    fn rows(
        &self,
        design: Design,
        range: std::ops::Range<usize>,
        layout: &Layout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let columns = layout.columns;
        range
            .map(|row| {
                let start = row * columns;
                let end = (start + columns).min(self.photos.len());
                h_flex()
                    .w_full()
                    .h(layout.tile_height + px(GRID_GAP))
                    .pt(px(GRID_GAP))
                    // The content plane's gutter. Wider than the gap between
                    // tiles, because the window edge is a separate group from
                    // the next tile.
                    .px(px(SPACE_LG))
                    .gap(px(GRID_GAP))
                    .children(
                        self.photos[start..end]
                            .iter()
                            .map(|photo| self.tile(design, photo, layout, window, cx)),
                    )
            })
            .collect()
    }

    fn tile(
        &self,
        design: Design,
        photo: &GalleryPhoto,
        layout: &Layout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected.as_ref() == Some(&photo.record.id);
        let id = photo.record.id.clone();
        // One hover key per tile: the plate fades in over the frame, and no
        // separate element id is needed.
        let hover_key: String = photo.element_id.to_string();
        let theme = cx.theme();
        let muted_foreground = theme.muted_foreground;
        let warning = theme.warning;
        let hover_t = hover::hover_t(&hover_key);
        let focused = self.grid_focus.is_focused(window);
        let (rim, halo) = glass::selection_ring(design, selected, focused, hover_t);
        let tile = div()
            .id(photo.element_id.clone())
            .relative()
            .flex_none()
            .w(layout.tile_width)
            .h(layout.tile_height)
            .overflow_hidden()
            .rounded(px(PANEL_RADIUS))
            .bg(design.surface_card)
            .border_2()
            .border_color(rim)
            .shadow(halo)
            .on_hover(cx.listener(move |_, hovered: &bool, _, cx| {
                // The enter and the leave both have to be recorded: gpui's
                // with_animation clock replays on remount, so the fade must not
                // go through it. Only a real change asks for a redraw, so a
                // pointer sweep across the grid costs two paints per tile, not
                // one per move event.
                if hover::set_hover(&hover_key, *hovered, cx.reduce_motion()) {
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
                if view.benchmark.is_some() {
                    return;
                }
                window.focus(&view.grid_focus, cx);
                view.select_id(&id, cx);
                if event.click_count() >= 2 {
                    view.open_viewer(window, cx);
                }
            }));
        let tile = match &photo.thumbnail {
            Thumbnail::Ready(path) => tile.child(
                img(path.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .image_cache(&self.thumbnails_cache),
            ),
            Thumbnail::Pending => tile.child(Skeleton::new().size_full()),
            Thumbnail::Failed => tile.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child(Icon::new(IconName::TriangleAlert).small())
                    .child("Preview unavailable"),
            ),
        };
        // The hover plate rides above the image and fades in. Skip it at rest:
        // a child added for every tile on every frame costs layout and paint
        // for nothing.
        let tile = if hover_t > 0.0 {
            tile.child(
                glass::light_plate(design, hover_t)
                    .apply(div().absolute().inset_0().rounded(px(PANEL_RADIUS))),
            )
        } else {
            tile
        };
        tile.when(photo.record.source_missing, |tile| {
            tile.child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .p_1()
                    .rounded_full()
                    .bg(design.surface_card)
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .small()
                            .text_color(warning),
                    ),
            )
        })
        .into_any_element()
    }

    /// The unified title strip: the window's object and its import command.
    /// Replaces the separate toolbar, which stacked a second chrome band under
    /// the native titlebar.
    ///
    /// Two children only: the component lays its children out with
    /// `justify_between` and no gap of its own, so six children pushed the title
    /// against the Import button.
    fn title_bar(&self, design: Design, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.grid_focus.clone();
        let library_open = matches!(self.library, LibraryState::Open(_));
        let performance_label = if self.show_performance {
            "Hide performance"
        } else {
            "Show performance"
        };
        TitleBar::new()
            .pr(px(SPACE_LG))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(design.text)
                            .mr(px(SPACE_LG))
                            .child(APP_NAME),
                    )
                    .child(
                        DropdownButton::new("import")
                            .button(
                                Button::new("import-files")
                                    .icon(IconName::Plus)
                                    .label("Import…")
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
            .child(
                Button::new("performance")
                    .ghost()
                    .icon(IconName::Cpu)
                    .toggled(self.show_performance)
                    .tooltip(performance_label)
                    .accessibility_label(performance_label)
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_performance(cx))),
            )
            .when(design.is_glass(), |this| this.bg(design.chrome()))
    }

    /// Persistent navigation beside the work area. One destination per
    /// collection the library keeps, with its count as a suffix.
    fn sidebar(&self, design: Design, cx: &mut Context<Self>) -> impl IntoElement {
        let library_open = matches!(self.library, LibraryState::Open(_));
        let failures = self.failures.len();
        let photos = self.photos.len();
        let total_bytes: u64 = self.photos.iter().map(|photo| photo.record.byte_size).sum();
        let muted = design.text_muted;
        let footer = SidebarFooter::new()
            .collapsed(self.sidebar_collapsed)
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(Icon::new(IconName::HardDrive).small().text_color(muted))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(format::library_size(total_bytes)),
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
                        .suffix(move |_, _| {
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format::number(photos))
                                .into_any_element()
                        })]),
                ),
            )
            .when(failures > 0, |this| {
                this.child(
                    SidebarGroup::new("Attention").child(
                        SidebarMenu::new().children([SidebarMenuItem::new("Not imported")
                            .icon(Icon::new(IconName::TriangleAlert).small())
                            .suffix(move |_, _| {
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format::number(failures))
                                    .into_any_element()
                            })
                            .on_click(
                                cx.listener(|view, _, window, cx| view.open_failures(window, cx)),
                            )]),
                    ),
                )
            })
            .footer(footer)
    }

    fn grid(&self, design: Design, layout: &Layout, cx: &mut Context<Self>) -> AnyElement {
        match &self.library {
            LibraryState::Opening => return centered_note("Opening library…", cx),
            LibraryState::Unavailable(message) => {
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .p_8()
                    .child(
                        Alert::error("library-unavailable", message.clone())
                            .title("The library couldn't be opened"),
                    )
                    .into_any_element();
            }
            LibraryState::Open(_) => {}
        }
        if self.photos.is_empty() {
            if self.is_importing() {
                return centered_note("Looking for JPEGs…", cx);
            }
            return self.empty_state(cx);
        }
        let entity = cx.entity();
        let columns = layout.columns;
        let geometry = *layout;
        glass::edge_scrim(
            design.panel(),
            SCROLL_FADE_BAND,
            true,
            true,
            uniform_list(
                "photo-grid",
                self.photos.len().div_ceil(columns),
                move |range, window, cx| {
                    entity.update(cx, |view, cx| {
                        view.rows(design, range, &geometry, window, cx)
                    })
                },
            )
            .track_scroll(&self.scroll)
            .size_full(),
        )
        .vertical_scrollbar(&self.scroll)
        .into_any_element()
    }

    fn empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .p_8()
            .child(
                Icon::new(IconName::GalleryVerticalEnd)
                    .large()
                    .text_color(theme.muted_foreground),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("No photos yet"),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        "Import JPEGs from folders or from Photos. Originals stay where they are.",
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("empty-import-files")
                            .label("Import files…")
                            .disabled(self.picking)
                            .on_click(cx.listener(|view, _, _, cx| view.choose_files(cx))),
                    )
                    .child(
                        Button::new("empty-import-photos")
                            .label("Import from Photos…")
                            .disabled(self.picking)
                            .on_click(cx.listener(|view, _, _, cx| view.choose_from_photos(cx))),
                    ),
            )
            .into_any_element()
    }

    fn inspector(&self, design: Design, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let panel = v_flex()
            .id("inspector")
            .flex_none()
            .w(px(INSPECTOR_WIDTH))
            .h_full()
            .p_4()
            .gap_4()
            .bg(design.chrome())
            .border_l_1()
            .border_color(design.border);
        let Some(photo) = self.selected_photo() else {
            // A designed empty state, not a bare line centred in a large slab:
            // a card, a glyph, and the next action. This is only reachable when
            // the library has photos but none is selectable.
            return panel
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .p_4()
                        .rounded(px(PANEL_RADIUS))
                        .bg(design.surface_card)
                        .border_1()
                        .border_color(design.border)
                        .child(
                            Icon::new(IconName::GalleryVerticalEnd)
                                .large()
                                .text_color(design.text_faint),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(design.text_muted)
                                .child("Select a photo"),
                        ),
                )
                .overflow_y_scrollbar();
        };
        let record = &photo.record;
        let metadata = &record.metadata;
        let mut details = DescriptionList::horizontal()
            .columns(1)
            .bordered(false)
            .label_width(px(92.))
            .small()
            .item(
                "Dimensions",
                format!("{} × {}", record.width, record.height),
                1,
            )
            .item("File size", format::bytes(record.byte_size), 1);
        if let Some(camera) = metadata.camera() {
            details = details.item("Camera", camera, 1);
        }
        if let Some(lens) = &metadata.lens {
            details = details.item("Lens", lens.clone(), 1);
        }
        if let Some(captured_at) = &metadata.captured_at {
            details = details.item("Taken", captured_at.to_string(), 1);
        }
        if record.source_count > 1 {
            details = details.item(
                "Copies",
                format!("Found in {} places", format::number(record.source_count)),
                1,
            );
        }
        let preview_error = self
            .preview
            .as_ref()
            .filter(|preview| preview.id == record.id)
            .and_then(|preview| preview.error.clone());
        panel
            .child(self.preview_image(photo, px(INSPECTOR_PREVIEW_HEIGHT), design))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(record.file_name()),
                    )
                    .when_some(metadata.exposure_summary(), |this, summary| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(summary),
                        )
                    }),
            )
            .when(record.source_missing, |this| {
                this.child(
                    Alert::warning(
                        "source-missing",
                        "Import it from its new location to reconnect it. Its library previews are kept.",
                    )
                    .title("The original has moved or was deleted"),
                )
            })
            .when_some(preview_error, |this, message| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(message),
                )
            })
            .child(details)
            .child(
                v_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Original")
                    .child(
                        div()
                            .text_color(theme.foreground)
                            .child(record.source.display().to_string()),
                    ),
            )
            .overflow_y_scrollbar()
    }

    /// The large rendition when ready, otherwise the grid thumbnail.
    fn preview_image(
        &self,
        photo: &GalleryPhoto,
        height: Pixels,
        design: Design,
    ) -> impl IntoElement {
        let large = self
            .preview
            .as_ref()
            .filter(|preview| preview.id == photo.record.id)
            .and_then(|preview| preview.image.clone());
        let frame = div()
            .relative()
            .flex_none()
            .w_full()
            .h(height)
            .overflow_hidden()
            .rounded(px(PANEL_RADIUS))
            .bg(design.surface_card);
        match (large, &photo.thumbnail) {
            (Some(path), _) => frame.child(framed_image(&path).image_cache(&self.preview_cache)),
            (None, Thumbnail::Ready(path)) => {
                frame.child(framed_image(path).image_cache(&self.thumbnails_cache))
            }
            (None, _) => frame.child(Skeleton::new().size_full()),
        }
    }

    fn viewer(&self, design: Design, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.selected_index() else {
            return div().into_any_element();
        };
        let photo = &self.photos[index];
        let preview = self
            .preview
            .as_ref()
            .filter(|preview| preview.id == photo.record.id);
        let large = preview.and_then(|preview| preview.image.clone());
        let error = preview.and_then(|preview| preview.error.clone());
        let loading = large.is_none() && error.is_none();
        let image: AnyElement = match (&large, &photo.thumbnail) {
            (Some(path), _) => framed_image(path)
                .image_cache(&self.preview_cache)
                .into_any_element(),
            (None, Thumbnail::Ready(path)) => framed_image(path)
                .image_cache(&self.thumbnails_cache)
                .into_any_element(),
            (None, _) => div().into_any_element(),
        };
        let note = error.or_else(|| loading.then(|| SharedString::from("Loading full size…")));
        let (bar_text, bar_muted) = design.overlay_text();
        // A floating glass bar over the photograph. This is the one surface the
        // design system's `frosted` recipe is for: it carries its own labels and
        // must not rely on the plane behind the photo.
        let bar = glass::frosted(
            design,
            CONTROL_RADIUS,
            h_flex()
                .flex_none()
                .h(px(HEADER_HEIGHT))
                .px(px(SPACE_MD))
                .gap_2()
                .child(
                    Button::new("close-viewer")
                        .ghost()
                        .icon(IconName::Close)
                        .tooltip("Close (Esc)")
                        .accessibility_label("Close")
                        .text_color(bar_text)
                        .on_click(cx.listener(|view, _, window, cx| view.close_viewer(window, cx))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(bar_text)
                        .child(photo.record.file_name()),
                )
                .child(div().text_sm().text_color(bar_muted).child(format!(
                    "{} of {}",
                    format::number(index + 1),
                    format::number(self.photos.len())
                )))
                .child(
                    Button::new("previous-photo")
                        .ghost()
                        .icon(IconName::ChevronLeft)
                        .tooltip("Previous photo")
                        .accessibility_label("Previous photo")
                        .disabled(index == 0)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.move_selection(Movement::Left, cx);
                        })),
                )
                .child(
                    Button::new("next-photo")
                        .ghost()
                        .icon(IconName::ChevronRight)
                        .tooltip("Next photo")
                        .accessibility_label("Next photo")
                        .disabled(index + 1 == self.photos.len())
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.move_selection(Movement::Right, cx);
                        })),
                ),
        );
        motion::dialog_in(
            "viewer-entrance",
            v_flex()
                .id("viewer")
                .key_context(VIEWER_CONTEXT)
                .track_focus(&self.viewer_focus)
                .on_action(
                    cx.listener(|view, _: &CloseViewer, window, cx| view.close_viewer(window, cx)),
                )
                .on_action(cx.listener(|view, _: &PreviousPhoto, _, cx| {
                    view.move_selection(Movement::Left, cx);
                }))
                .on_action(cx.listener(|view, _: &NextPhoto, _, cx| {
                    view.move_selection(Movement::Right, cx);
                }))
                .absolute()
                .inset_0()
                .occlude()
                .bg(design.scrim)
                .p(px(SPACE_MD))
                .child(bar)
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .mt(px(SPACE_MD))
                        .child(image),
                )
                .when_some(note, |this, note| {
                    this.child(
                        div()
                            .flex_none()
                            .pb(px(SPACE_SM))
                            .text_center()
                            .text_sm()
                            .text_color(design.text_muted)
                            .child(note),
                    )
                }),
        )
        .into_any_element()
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = self.progress.map(|(processed, total)| {
            let scanning = total == 0;
            #[allow(clippy::cast_precision_loss)]
            let percent = if scanning {
                0.0
            } else {
                processed as f32 * 100.0 / total as f32
            };
            let label = if scanning {
                "Looking for JPEGs…".to_owned()
            } else {
                format!("{} of {}", format::number(processed), format::number(total))
            };
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Progress::new("import-progress")
                        .loading(scanning)
                        .value(percent)
                        .accessibility_label("Import progress")
                        .w(px(PROGRESS_WIDTH)),
                )
                .child(label)
        });
        let design = cx.design();
        StatusBar::new()
            .text_xs()
            .text_color(design.text_muted)
            .bg(design.chrome())
            .left(div().truncate().child(self.status.clone()))
            .when_some(progress, StatusBar::right)
    }

    pub(super) fn open_failures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let gallery = cx.entity().downgrade();
        window.open_sheet(cx, move |sheet, _, cx| {
            let Some(gallery) = gallery.upgrade() else {
                return sheet;
            };
            let failures = gallery.read(cx).failures.clone();
            let theme = cx.theme();
            let empty = failures.is_empty();
            let list = v_flex().gap_4().children(failures.iter().map(|failure| {
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(file_name(&failure.path)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(failure.message.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(failure.path.display().to_string()),
                    )
            }));
            let retry = gallery.clone();
            let clear = gallery;
            sheet
                .title("Not imported")
                .size(px(SHEET_WIDTH))
                .child(if empty {
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Every file was imported.")
                } else {
                    list
                })
                .footer(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("clear-failures")
                                .ghost()
                                .label("Clear list")
                                .disabled(empty)
                                .on_click(move |_, window, cx| {
                                    let focus = clear.update(cx, |view, cx| {
                                        view.clear_failures(cx);
                                        view.grid_focus.clone()
                                    });
                                    window.close_sheet(cx);
                                    window.focus(&focus, cx);
                                }),
                        )
                        .child(
                            Button::new("retry-failures")
                                .label("Retry all")
                                .disabled(empty)
                                .on_click(move |_, window, cx| {
                                    let focus = retry.update(cx, |view, cx| {
                                        view.retry_failures(cx);
                                        view.grid_focus.clone()
                                    });
                                    window.close_sheet(cx);
                                    window.focus(&focus, cx);
                                }),
                        ),
                )
        });
    }

    fn select_id(&mut self, id: &PhotoId, cx: &mut Context<Self>) {
        if let Some(index) = self.photos.iter().position(|photo| &photo.record.id == id) {
            self.select_index(index, cx);
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.benchmark_frame(window, cx);
        let layout = self.layout(window);
        let row_height = layout.tile_height.as_f32() + GRID_GAP;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let page_rows = (window.viewport_size().height.as_f32() / row_height).floor() as usize;
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
            .child(self.title_bar(design, cx))
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
                            .child(self.grid(design, &layout, cx)),
                    )
                    .when(layout.show_inspector, |this| {
                        this.child(self.inspector(design, cx))
                    }),
            )
            .child(self.status_bar(cx))
            .when(self.viewer_open, |this| this.child(self.viewer(design, cx)))
            .when(self.show_performance, |this| {
                this.child(gpui_fps::fps_monitor(window, cx))
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

fn movement<A: gpui_kit::Action>(
    movement: Movement,
    cx: &mut Context<Gallery>,
) -> impl Fn(&A, &mut Window, &mut gpui_kit::App) + 'static {
    cx.listener(move |view, _: &A, _, cx| view.move_selection(movement, cx))
}

fn framed_image(path: &std::sync::Arc<Path>) -> gpui_kit::Img {
    img(path.clone())
        .absolute()
        .size_full()
        .object_fit(ObjectFit::Contain)
}

fn centered_note(text: &'static str, cx: &mut Context<Gallery>) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
