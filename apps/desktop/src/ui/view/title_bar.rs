//! The title bar: one chrome band across the window. The sidebar toggle sits past the traffic
//! lights, the grid's title starts where the grid does, and the window's commands sit at the
//! trailing edge.

use super::super::{
    format,
    gallery::{GRID_GUTTER, Gallery, GridLayout, SIDEBAR_WIDTH, TILE_WIDTH_RANGE},
    icons,
    look::{Look, MONO, TITLE},
};
use super::TITLE_BAR_HEIGHT;
use rune::{Color, Drag, El, PopupItem, PopupMenu, Tx, el, icon, row, text};
use rune_kit::{ControlSize, IconButton, IconButtonVariant};

/// Room past the traffic lights before the sidebar toggle.
const PAST_LIGHTS: f32 = 12.0;
/// Room the title leaves for the sidebar toggle when the sidebar is hidden.
const SIDEBAR_TOGGLE_CLEARANCE: f32 = 40.0;
const CONTROL: f32 = 24.0;
const ZOOM_SLIDER_WIDTH: f32 = 88.0;
const ZOOM_KNOB: f32 = 12.0;
const IMPORT_HEIGHT: f32 = 24.0;

pub(super) fn title_bar(s: &Gallery, look: &Look, layout: GridLayout, titlebar_inset: f32) -> El {
    let lights = titlebar_inset + PAST_LIGHTS;
    let sidebar = if layout.show_sidebar {
        SIDEBAR_WIDTH
    } else {
        0.0
    };
    let title_x = (sidebar + GRID_GUTTER).max(lights + SIDEBAR_TOGGLE_CLEARANCE);
    let benchmarking = s.benchmark.is_some();
    let toggle =
        |glyph, key: &'static str, label: &'static str, on: bool, press: fn(&mut Gallery)| {
            IconButton::new(glyph)
                .variant(IconButtonVariant::GhostMuted)
                .expanded(on)
                .label(label)
                .help(label)
                .on_press(press)
                .build(look)
                .key(key)
        };
    let sidebar_label = if s.sidebar_hidden {
        "Show sidebar (⌃⌘S)"
    } else {
        "Hide sidebar (⌃⌘S)"
    };
    let info_label = if s.inspector_hidden {
        "Show info (⌘I)"
    } else {
        "Hide info (⌘I)"
    };
    let performance_label = if s.performance.is_some() {
        "Hide performance (⌥⌘P)"
    } else {
        "Show performance (⌥⌘P)"
    };
    let sidebar_toggle = (!benchmarking).then(|| {
        toggle(
            &icons::PANEL_LEFT,
            "toggle-sidebar",
            sidebar_label,
            !s.sidebar_hidden,
            Gallery::toggle_sidebar,
        )
        .absolute(lights, (TITLE_BAR_HEIGHT - CONTROL) / 2.0)
    });
    let trailing = row()
        .key("title-bar-commands")
        .gap(6.0)
        .center_y()
        .children((!s.photos.is_empty()).then(|| zoom(s, look)))
        .child(toggle(
            &icons::PANEL_RIGHT,
            "toggle-info",
            info_label,
            !s.inspector_hidden,
            Gallery::toggle_inspector,
        ))
        .child(toggle(
            &icons::CPU,
            "performance-toggle",
            performance_label,
            s.performance.is_some(),
            Gallery::toggle_performance,
        ))
        .child(el().w(1.0).h(16.0).mx(4.0).fill(look.rule).a11y_hidden())
        .child(import(s, look));
    row()
        .key("title-bar")
        .role("toolbar")
        .w_full()
        .h(TITLE_BAR_HEIGHT)
        .pl(title_x)
        .pr(10.0)
        .center_y()
        .fill(look.chrome)
        .border_sides([0.0, 0.0, 1.0, 0.0], look.rule)
        .window_drag()
        .children(sidebar_toggle)
        .child(
            row()
                .key("title")
                .gap(8.0)
                .center_y()
                .shrink()
                .min_w(0.0)
                .child(
                    text(s.destination.title())
                        .font(TITLE)
                        .color(look.fg)
                        .truncate(),
                )
                .children((!s.photos.is_empty()).then(|| {
                    text(format::number(s.shown_len()))
                        .key("photo-count")
                        .font(MONO)
                        .color(look.faint)
                })),
        )
        .child(el().grow(1.0).a11y_hidden())
        .child(trailing)
}

/// The photo size: a slider between a smaller and a larger step.
fn zoom(s: &Gallery, look: &Look) -> El {
    let (low, high) = (*TILE_WIDTH_RANGE.start(), *TILE_WIDTH_RANGE.end());
    let at = ((s.min_tile_width - low) / (high - low)).clamp(0.0, 1.0);
    let step = |glyph, key: &'static str, label: &'static str, steps: i32| {
        IconButton::new(glyph)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Small)
            .label(label)
            .help(label)
            .on_press(move |s: &mut Gallery| s.zoom(steps))
            .build(look)
            .key(key)
    };
    let knob_x = at * (ZOOM_SLIDER_WIDTH - ZOOM_KNOB);
    let slider = el()
        .key("zoom-slider")
        .role("slider")
        .label("Photo size")
        .help("Photo size (⌘+ and ⌘−)")
        .w(ZOOM_SLIDER_WIDTH)
        .h(CONTROL)
        .cursor(rune::Cursor::Pointer)
        .on_drag(move |s: &mut Gallery, drag: &mut Drag, _: &mut Tx| {
            let t = ((drag.x - ZOOM_KNOB / 2.0) / (drag.w - ZOOM_KNOB)).clamp(0.0, 1.0);
            s.set_zoom(low + t * (high - low));
        })
        .on_step(|s: &mut Gallery, steps, _: &mut Tx| s.zoom(steps))
        .on_step_limit(move |s: &mut Gallery, max, _: &mut Tx| {
            s.set_zoom(if max { high } else { low })
        })
        .child(
            el().absolute(0.0, (CONTROL - 4.0) / 2.0)
                .w(ZOOM_SLIDER_WIDTH)
                .h(4.0)
                .radius(2.0)
                .fill(look.rule_strong),
        )
        .child(
            el().absolute(0.0, (CONTROL - 4.0) / 2.0)
                .w(knob_x + ZOOM_KNOB / 2.0)
                .h(4.0)
                .radius(2.0)
                .fill(look.muted),
        )
        .child(
            el().key("zoom-knob")
                .absolute(knob_x, (CONTROL - ZOOM_KNOB) / 2.0)
                .size(ZOOM_KNOB, ZOOM_KNOB)
                .radius(ZOOM_KNOB / 2.0)
                .fill(look.selection)
                .shadow(1.0, 3.0, Color::BLACK.alpha(0.3)),
        );
    row()
        .key("zoom")
        .gap(2.0)
        .mr(6.0)
        .center_y()
        .child(step(&icons::MINUS, "zoom-out", "Smaller photos (⌘−)", -1))
        .child(slider)
        .child(step(&icons::PLUS, "zoom-in", "Larger photos (⌘+)", 1))
}

/// Import, as one red split button: the button imports files or folders, and its chevron
/// offers the other sources.
fn import(s: &Gallery, look: &Look) -> El {
    let enabled = !s.picking && s.library.is_open();
    let segment = |key: &'static str| {
        let segment = row()
            .key(key)
            .h_full()
            .center_y()
            .role("button")
            .cursor(rune::Cursor::Pointer);
        if enabled {
            segment
                .hover(look.primary_hover)
                .press(look.primary_pressed)
        } else {
            segment
        }
    };
    let mut main = segment("import-files")
        .label("Import")
        .help("Import files or folders (⌘O)")
        .px(8.0)
        .gap(4.0)
        .child(icon(&icons::PLUS, 14.0).color(look.on_primary))
        .child(
            text("Import")
                .font(look.ty.ui_medium)
                .color(look.on_primary),
        );
    let mut more = segment("import-more")
        .label("More import sources")
        .w(22.0)
        .center()
        .border_sides([0.0, 0.0, 0.0, 1.0], look.on_primary.alpha(0.25))
        .child(icon(look.glyph.chevron_down, 12.0).color(look.on_primary));
    if enabled {
        main = main.on_press(|s: &mut Gallery| s.choose_files());
        more = more.on_press_tx(|_: &mut Gallery, tx: &mut Tx| {
            let menu = PopupMenu::new()
                .item(PopupItem::new("files", "Files or folders…"))
                .item(PopupItem::new("photos", "From Photos…"));
            tx.popup_menu(
                menu,
                "import-more",
                |s: &mut Gallery, picked, _| match picked {
                    Some("files") => s.choose_files(),
                    Some("photos") => s.choose_from_photos(),
                    _ => {}
                },
            );
        });
    }
    row()
        .key("import")
        .h(IMPORT_HEIGHT)
        .radius(look.radius.md)
        .fill(look.primary)
        .clip_rounded(look.radius.md)
        .opacity(if enabled { 1.0 } else { 0.5 })
        .child(main)
        .child(more)
}
