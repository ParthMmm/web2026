//! The app's look: the Library design's palette for each appearance, laid over rune-kit's
//! tokens so kit controls agree with the app's own parts, and the Geist faces. Every colour
//! the view paints comes from here.

use rune::{Color, TextStyle, color::hex};
use rune_kit::{Kit, PaletteSeed};
use std::ops::Deref;

/// Geist and Geist Mono (OFL, `fonts/Geist-OFL.txt`), variable in weight.
pub(super) static FONTS: &[&[u8]] = &[
    include_bytes!("../../fonts/Geist-Variable.ttf"),
    include_bytes!("../../fonts/GeistMono-Variable.ttf"),
];
pub(super) const FAMILIES: (&str, &str) = ("Geist", "Geist Mono");

/// One appearance's colours, as the design gives them (CSS hex, sRGB alpha).
struct Swatch {
    background: &'static str,
    foreground: &'static str,
    muted_background: &'static str,
    muted_foreground: &'static str,
    border: &'static str,
    input_border: &'static str,
    ring: &'static str,
    chrome: &'static str,
    popover: &'static str,
    hover: &'static str,
    selected: &'static str,
    primary: &'static str,
    primary_hover: &'static str,
    primary_pressed: &'static str,
    selection_ring: &'static str,
    skeleton: &'static str,
    warning: &'static str,
    scrim: &'static str,
    /// The ink translucent fills are made of: white on dark, black on light.
    ink: &'static str,
}

const DARK: Swatch = Swatch {
    background: "#060606",
    foreground: "#EDEDED",
    muted_background: "#161616",
    muted_foreground: "#9A9A9A",
    border: "#FFFFFF14",
    input_border: "#FFFFFF24",
    ring: "#FFFFFFB3",
    chrome: "#0D0D0D",
    popover: "#161616",
    hover: "#FFFFFF12",
    selected: "#FFFFFF1C",
    primary: "#E5483A",
    primary_hover: "#EC5A4C",
    primary_pressed: "#D13D30",
    selection_ring: "#F2F2F2",
    skeleton: "#141414",
    warning: "#D9A441",
    scrim: "#00000099",
    ink: "#FFFFFF",
};

const LIGHT: Swatch = Swatch {
    background: "#FFFFFF",
    foreground: "#171717",
    muted_background: "#F2F2F1",
    muted_foreground: "#6B6B6B",
    border: "#0000001A",
    input_border: "#00000024",
    ring: "#000000B3",
    chrome: "#F4F4F3",
    popover: "#FFFFFF",
    hover: "#0000000A",
    selected: "#0000000F",
    primary: "#C8372A",
    primary_hover: "#B83024",
    primary_pressed: "#A52A1F",
    selection_ring: "#1A1A1A",
    skeleton: "#EFEFEE",
    warning: "#B7791F",
    scrim: "#00000059",
    ink: "#000000",
};

/// Bar and title text.
pub(super) const TITLE: TextStyle = TextStyle::new(15.0, 600.0);
/// Counts, sizes and camera settings: Geist Mono with tabular figures.
pub(super) const MONO: TextStyle = TextStyle::new(12.0, 400.0).mono().features("'tnum' 1");
pub(super) const MONO_SMALL: TextStyle = TextStyle::new(11.5, 400.0).mono().features("'tnum' 1");

/// The kit, with the app's roles named once.
#[derive(Clone)]
pub(super) struct Look {
    pub(super) k: Kit,
    /// The content plane: the grid and the viewer sit on it.
    pub(super) plane: Color,
    /// The chrome plane: title bar, sidebar, inspector, status bar and filmstrip.
    pub(super) chrome: Color,
    pub(super) fg: Color,
    pub(super) muted: Color,
    pub(super) faint: Color,
    /// Hairlines between planes.
    pub(super) rule: Color,
    pub(super) rule_strong: Color,
    /// The ring around the selected tile.
    pub(super) selection: Color,
    pub(super) primary: Color,
    pub(super) primary_hover: Color,
    pub(super) primary_pressed: Color,
    pub(super) on_primary: Color,
    /// The plate behind a photo that has no pixels yet.
    pub(super) plate: Color,
    /// A floating surface over a photo: the exposure pill, the step buttons.
    pub(super) float: Color,
    pub(super) warning: Color,
}

impl Look {
    pub(super) fn new(dark: bool) -> Self {
        let s = if dark { &DARK } else { &LIGHT };
        let c = |v: &str| hex(v).unwrap_or_else(|| panic!("{v} is a colour"));
        let plane = c(s.background);
        let fg = c(s.foreground);
        let muted = c(s.muted_foreground);
        let faint = muted.mix(plane, 0.35);
        let primary = c(s.primary);
        let ink = c(s.ink);
        let rule = c(s.border);
        let rule_strong = ink.alpha(if dark { 0.14 } else { 0.19 });
        let chrome = c(s.chrome);
        let warning = c(s.warning);
        let mut k = Kit::from_palette(PaletteSeed {
            bg: plane,
            fg,
            accent: primary,
        });
        let p = &mut k.c;
        p.page = plane;
        p.bg_base = plane;
        p.bg_deep = plane;
        p.layer_01 = chrome;
        p.layer_02 = c(s.muted_background);
        p.surface_raised = c(s.muted_background);
        p.surface_raised_base = c(s.popover);
        p.text_base = fg;
        p.text_strong = fg;
        p.text_muted = muted;
        p.text_weak = muted;
        p.text_faint = faint;
        p.icon_base = fg;
        p.icon_muted = muted;
        p.icon_faint = faint;
        p.border_muted = rule;
        p.border_base = rule;
        p.border_weak = rule;
        p.border_strong = c(s.input_border);
        p.border_focus = c(s.ring);
        p.hover = c(s.hover);
        p.surface_hover = c(s.hover);
        p.pressed = c(s.selected);
        p.bg_accent = primary;
        p.text_accent = primary;
        p.icon_accent = primary;
        p.scrim = c(s.scrim);
        p.warning.icon = warning;
        Self {
            k,
            plane,
            chrome,
            fg,
            muted,
            faint,
            rule,
            rule_strong,
            selection: c(s.selection_ring),
            primary,
            primary_hover: c(s.primary_hover),
            primary_pressed: c(s.primary_pressed),
            on_primary: Color::WHITE,
            plate: c(s.skeleton),
            float: chrome.alpha(0.86),
            warning,
        }
    }
}

impl Deref for Look {
    type Target = Kit;

    fn deref(&self) -> &Kit {
        &self.k
    }
}
