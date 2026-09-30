use super::{
    CloseViewer, GRID_CONTEXT, Import, ImportFromPhotos, NextPhoto, OpenViewer, PreviousPhoto,
    SelectDown, SelectFirst, SelectLast, SelectLeft, SelectPageDown, SelectPageUp, SelectRight,
    SelectUp, ShowImportFailures, TogglePerformance, VIEWER_CONTEXT, benchmark, format,
    gallery::{Gallery, GalleryPhoto, LibraryState, Thumbnail},
};
use crate::{PhotoId, grid::Movement};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Root, Sizable as _, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants as _, DropdownButton},
    description_list::DescriptionList,
    h_flex,
    progress::Progress,
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::{
    AccessibleAction, AnyElement, ClickEvent, Context, Div, FontWeight, InteractiveElement as _,
    IntoElement, ObjectFit, ParentElement as _, Pixels, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window, div, img,
    prelude::FluentBuilder as _, px, transparent_black, uniform_list,
};
use std::path::Path;

// The grid is a `uniform_list`, which needs exact row heights, so its
// geometry is computed here rather than left to flex layout.
const GRID_GAP: f32 = 8.0;
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
    show_inspector: bool,
}

impl Gallery {
    fn layout(&self, window: &Window) -> Layout {
        let viewport = window.viewport_size();
        let benchmarking = self.benchmark.is_some();
        let show_inspector = benchmarking
            || (!self.photos.is_empty() && viewport.width.as_f32() >= MIN_WIDTH_FOR_INSPECTOR);
        let inspector = if show_inspector { INSPECTOR_WIDTH } else { 0.0 };
        let usable = (viewport.width.as_f32() - inspector - GRID_GAP * 2.0).max(MIN_TILE_WIDTH);
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
            show_inspector,
        }
    }

    fn rows(
        &self,
        range: std::ops::Range<usize>,
        layout: &Layout,
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
                    .px(px(GRID_GAP))
                    .gap(px(GRID_GAP))
                    .children(
                        self.photos[start..end]
                            .iter()
                            .map(|photo| self.tile(photo, layout, cx)),
                    )
            })
            .collect()
    }

    fn tile(&self, photo: &GalleryPhoto, layout: &Layout, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected.as_ref() == Some(&photo.record.id);
        let id = photo.record.id.clone();
        let accessible_id = id.clone();
        let select_accessible = cx.listener(move |view, _: &(), window, cx| {
            if view.benchmark.is_none() {
                window.focus(&view.grid_focus, cx);
                view.select_id(&accessible_id, cx);
            }
        });
        let theme = cx.theme();
        let muted_foreground = theme.muted_foreground;
        let warning = theme.warning;
        let background = theme.background;
        let tile = div()
            .id(photo.element_id.clone())
            .accessibility_id(photo.element_id.clone())
            .role(Role::ListBoxOption)
            .aria_label(photo.record.file_name())
            .aria_description(if photo.record.source_missing {
                "Original missing. Import it from its new location to reconnect it."
            } else {
                "Use arrow keys to browse. Press Space or Return to open the photo."
            })
            .aria_selected(selected)
            .when(selected, |tile| tile.aria_active_descendant())
            .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                select_accessible(&(), window, cx)
            })
            .relative()
            .flex_none()
            .w(layout.tile_width)
            .h(layout.tile_height)
            .overflow_hidden()
            .rounded(theme.radius)
            .bg(theme.muted)
            .border_2()
            .border_color(if selected {
                theme.list_active_border
            } else {
                transparent_black()
            })
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
        tile.when(photo.record.source_missing, |tile| {
            tile.child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .p_1()
                    .rounded_full()
                    .bg(background)
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .small()
                            .text_color(warning),
                    ),
            )
        })
        .into_any_element()
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let focus = self.grid_focus.clone();
        let library_open = matches!(self.library, LibraryState::Open(_));
        let failures = self.failures.len();
        let performance_label = if self.show_performance {
            "Hide performance"
        } else {
            "Show performance"
        };
        h_flex()
            .flex_none()
            .px_3()
            .py_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
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
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .when(library_open && !self.photos.is_empty(), |this| {
                        this.child(format::count(self.photos.len(), "photo", "photos"))
                    }),
            )
            .child(div().flex_1())
            .when(failures > 0, |this| {
                this.child(
                    Button::new("import-failures")
                        .ghost()
                        .icon(IconName::TriangleAlert)
                        .label(format!("{} not imported…", format::number(failures)))
                        .on_click(
                            cx.listener(|view, _, window, cx| view.open_failures(window, cx)),
                        ),
                )
            })
            .child(
                Button::new("performance")
                    .ghost()
                    .icon(IconName::Cpu)
                    .toggled(self.show_performance)
                    .tooltip(performance_label)
                    .accessibility_label(performance_label)
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_performance(cx))),
            )
    }

    fn grid(&self, layout: &Layout, cx: &mut Context<Self>) -> AnyElement {
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
        div()
            .relative()
            .size_full()
            .child(
                uniform_list(
                    "photo-grid",
                    self.photos.len().div_ceil(columns),
                    move |range, _, cx| {
                        entity.update(cx, |view, cx| view.rows(range, &geometry, cx))
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

    fn inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let panel = v_flex()
            .id("inspector")
            .flex_none()
            .w(px(INSPECTOR_WIDTH))
            .h_full()
            .p_4()
            .gap_4()
            .border_l_1()
            .border_color(theme.border);
        let Some(photo) = self.selected_photo() else {
            return panel
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Select a photo to see its details")
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
        if let Some(film) = metadata.film_simulation {
            details = details.item("Camera film simulation", film.to_string(), 1);
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
            .child(self.preview_image(photo, px(INSPECTOR_PREVIEW_HEIGHT), cx))
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
        cx: &gpui_kit::App,
    ) -> impl IntoElement {
        let theme = cx.theme();
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
            .rounded(theme.radius)
            .bg(theme.muted);
        match (large, &photo.thumbnail) {
            (Some(path), _) => frame.child(framed_image(&path).image_cache(&self.preview_cache)),
            (None, Thumbnail::Ready(path)) => {
                frame.child(framed_image(path).image_cache(&self.thumbnails_cache))
            }
            (None, _) => frame.child(Skeleton::new().size_full()),
        }
    }

    fn viewer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.selected_index() else {
            return div().into_any_element();
        };
        let photo = &self.photos[index];
        let theme = cx.theme();
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
            .bg(theme.background)
            .child(
                h_flex()
                    .flex_none()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("close-viewer")
                            .ghost()
                            .icon(IconName::Close)
                            .tooltip("Close (Esc)")
                            .accessibility_label("Close")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.close_viewer(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(photo.record.file_name()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} of {}",
                                format::number(index + 1),
                                format::number(self.photos.len())
                            )),
                    )
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
            )
            .child(div().relative().flex_1().min_h_0().m_4().child(image))
            .when_some(note, |this, note| {
                this.child(
                    div()
                        .flex_none()
                        .pb_3()
                        .text_center()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            })
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
        let theme = cx.theme();
        StatusBar::new()
            .text_xs()
            .text_color(theme.muted_foreground)
            .left(div().truncate().child(self.status.clone()))
            .when_some(progress, StatusBar::right)
    }

    pub(super) fn open_failures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let gallery = cx.entity().downgrade();
        let failures = self.failures.clone();
        window.open_sheet(cx, move |sheet, _, cx| {
            let Some(gallery) = gallery.upgrade() else {
                return sheet;
            };
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

        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        v_flex()
            .id("gallery")
            .relative()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .on_action(cx.listener(|view, _: &Import, _, cx| view.choose_files(cx)))
            .on_action(cx.listener(|view, _: &ImportFromPhotos, _, cx| view.choose_from_photos(cx)))
            .on_action(cx.listener(|view, _: &ShowImportFailures, window, cx| {
                view.open_failures(window, cx);
            }))
            .on_action(
                cx.listener(|view, _: &TogglePerformance, _, cx| view.toggle_performance(cx)),
            )
            .child(self.toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("photo-grid-region")
                            .role(Role::ListBox)
                            .aria_label("Photo library")
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
                            .child(self.grid(&layout, cx)),
                    )
                    .when(layout.show_inspector, |this| this.child(self.inspector(cx))),
            )
            .child(self.status_bar(cx))
            .when(self.viewer_open, |this| this.child(self.viewer(cx)))
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

#[cfg(all(test, feature = "visual-check"))]
mod tests {
    use super::*;
    use crate::catalog::ImportFailure;
    use crate::ui::GalleryOptions;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, TestAppContext, size};

    #[gpui_kit::test]
    fn failures_sheet_renders_clears_and_reopens(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let library = tempfile::tempdir().expect("temporary library");
        let mut gallery = None;
        let window = cx.open_window(size(px(1200.), px(820.)), |window, cx| {
            let view = cx.new(|cx| {
                Gallery::new(
                    GalleryOptions {
                        library_root: library.path().to_path_buf(),
                        inputs: Vec::new(),
                        benchmark_seconds: None,
                    },
                    window,
                    cx,
                )
            });
            gallery = Some(view.clone());
            Root::new(view, window, cx)
        });
        let gallery = gallery.expect("gallery created");
        cx.update_window(window.into(), |_, window, cx| {
            gallery.update(cx, |view, cx| {
                view.failures.push(ImportFailure {
                    path: library.path().join("broken.jpg"),
                    message: "Invalid JPEG".into(),
                    failed_at: 0,
                });
                cx.notify();
            });
            window.render_frame(cx);
            window.click("import-failures", cx);
            window.render_frame(cx);
            assert!(window.has_active_sheet(cx));

            window.click("retry-failures", cx);
            window.render_frame(cx);
            assert!(!window.has_active_sheet(cx));
            assert!(gallery.read(cx).grid_focus.is_focused(window));
            window.click("import-failures", cx);
            window.render_frame(cx);
            assert!(window.has_active_sheet(cx));

            window.click("clear-failures", cx);
            window.render_frame(cx);
            assert!(!window.has_active_sheet(cx));
            assert!(gallery.read(cx).failures.is_empty());
            assert!(gallery.read(cx).grid_focus.is_focused(window));

            gallery.update(cx, |view, cx| view.open_failures(window, cx));
            window.render_frame(cx);
            assert!(window.has_active_sheet(cx));
            for button in ["clear-failures", "retry-failures"] {
                window.click(button, cx);
                window.render_frame(cx);
                assert!(window.has_active_sheet(cx));
                assert!(gallery.read(cx).failures.is_empty());
            }
            window.close_sheet(cx);
            window.render_frame(cx);
            assert!(!window.has_active_sheet(cx));
        })
        .expect("render failures sheet");
    }
}
