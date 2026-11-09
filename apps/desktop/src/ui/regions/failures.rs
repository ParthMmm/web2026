//! The sheet that lists files that could not be imported.

use crate::catalog;
use crate::ui::gallery::Gallery;
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

const SHEET_WIDTH: f32 = 440.0;

impl Gallery {
    pub(in crate::ui) fn open_failures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                            .child(catalog::file_name(&failure.path)),
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
                                .on_click(finish(clear, Gallery::clear_failures)),
                        )
                        .child(
                            Button::new("retry-failures")
                                .label("Retry all")
                                .disabled(empty)
                                .on_click(finish(retry, Gallery::retry_failures)),
                        ),
                )
        });
    }
}

/// A footer action: run it on the gallery, close the sheet, and return focus
/// to the grid.
fn finish(
    gallery: Entity<Gallery>,
    action: fn(&mut Gallery, &mut Context<Gallery>),
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        let focus = gallery.update(cx, |view, cx| {
            action(view, cx);
            view.grid_focus.clone()
        });
        window.close_sheet(cx);
        window.focus(&focus, cx);
    }
}
