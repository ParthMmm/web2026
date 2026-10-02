//! The design system: the resolved planes, and the glass, motion and interaction
//! rules on top of them.
//!
//! Ported from Zeron (MIT, Copyright (c) 2026 Wing). The component library owns
//! the semantic palette; this module adds the elevation ladder, the glass planes
//! and plate recipes derived from it, the motion catalog with its shared clock,
//! and hover fades for the states gpui snaps.
//!
//! `docs/photo-gallery/zeron-reference.md` carries the reasoning and the
//! measurements. `gpui-pre 0.3.4` has no in-app backdrop blur and no per-pixel
//! edge fade, so [`glass::frosted`] paints a tint without the blur pass and
//! [`glass::edge_scrim`] gradients into an opaque plane.

pub mod color;
pub mod glass;
pub mod hover;
pub mod motion;

use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::{App, Hsla};
use std::sync::atomic::{AtomicU8, Ordering};

use color::{best_on, flatten, hairline, ink, lighten_to_contrast, mix, wash};

/// Which appearance the resolved planes were built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

/// How a surface composites: a flat plane, or a translucent material over the
/// window's blurred backdrop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceTreatment {
    Opaque,
    #[default]
    Frosted,
}

/// The device-local policy applied to the theme's recommended treatment.
/// Independent of appearance and of the palette, exactly as in Zeron.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfacePreference {
    #[default]
    ThemeDefault,
    Frosted,
    Opaque,
}

impl SurfacePreference {
    pub const ALL: [Self; 3] = [Self::ThemeDefault, Self::Frosted, Self::Opaque];

    pub fn label(self) -> &'static str {
        match self {
            Self::ThemeDefault => "Theme default",
            Self::Frosted => "Frosted",
            Self::Opaque => "Opaque",
        }
    }

    pub fn resolve(self, recommended: SurfaceTreatment) -> SurfaceTreatment {
        match self {
            Self::ThemeDefault => recommended,
            Self::Frosted => SurfaceTreatment::Frosted,
            Self::Opaque => SurfaceTreatment::Opaque,
        }
    }
}

/// The persisted surface preference.
///
/// A process-wide mirror rather than a gpui global, so the paint helpers that
/// build elements deep inside the tree can read it without threading `cx`
/// through every signature. Appearance and material are one setting for every
/// window, so a single value is sound.
static SURFACE: AtomicU8 = AtomicU8::new(0);

/// Install the design globals. Call once at boot, before the first window, so
/// the first frame already has its final material.
pub fn init(cx: &mut App) {
    set_surface_preference(SurfacePreference::default(), cx);
}

/// The active surface preference.
pub fn surface_preference() -> SurfacePreference {
    match SURFACE.load(Ordering::Relaxed) {
        1 => SurfacePreference::Frosted,
        2 => SurfacePreference::Opaque,
        _ => SurfacePreference::ThemeDefault,
    }
}

/// Replace the surface preference. Restart-free: the next paint reads it. Every
/// window is marked dirty, because glass is read imperatively at paint time and
/// no view knows its material changed.
pub fn set_surface_preference(preference: SurfacePreference, cx: &mut App) {
    let encoded = match preference {
        SurfacePreference::ThemeDefault => 0,
        SurfacePreference::Frosted => 1,
        SurfacePreference::Opaque => 2,
    };
    if SURFACE.swap(encoded, Ordering::Relaxed) != encoded {
        cx.refresh_windows();
    }
}

/// The treatment this app recommends for its own palette. Zeron recommends
/// frost because the chrome was designed to sit over a blurred backdrop.
const RECOMMENDED_TREATMENT: SurfaceTreatment = SurfaceTreatment::Frosted;

/// Layout numbers. "Numbers drive layout, colours are paint": these are plain
/// values and never depend on which palette is installed.
///
/// The titlebar arithmetic is worth keeping: a top-only pad moves a
/// flex-centred row by half its value, so `38 / 2 + 5 / 2 = 21.5` lands the
/// traffic lights and the title text on the same optical centre.
pub const TITLEBAR_HEIGHT: f32 = 38.0;
pub const TITLEBAR_TOP_PAD: f32 = 5.0;
pub const HEADER_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
pub const PANEL_RADIUS: f32 = 10.0;
pub const CONTROL_RADIUS: f32 = 6.0;
pub const BUBBLE_RADIUS: f32 = 16.0;
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;

/// Height of the band that fades scrolling content into the plane behind it.
pub const SCROLL_FADE_BAND: f32 = 24.0;

/// The sidebar's expanded width, and its collapsed icon width.
///
/// Held here rather than left to the component's private default, so the grid's
/// column count can subtract the width the sidebar takes.
pub const SIDEBAR_WIDTH: f32 = 255.0;
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 48.0;

/// The resolved planes for one appearance. Everything is `Copy`, so a render
/// function can take it by value.
#[derive(Debug, Clone, Copy)]
pub struct Design {
    pub appearance: Appearance,
    pub surface_treatment: SurfaceTreatment,

    /// Main content plane. The grid and the viewer sit here.
    pub bg: Hsla,
    /// Chrome plane: sidebar, title strip, inspector, status bar. Dark: one step
    /// *up* from `bg`. Light: one step *down*, so chrome recedes from the content
    /// plane in both appearances — the direction a naive inversion gets backwards.
    pub surface: Hsla,
    /// A chip or pill that sits proud of the panel.
    pub surface_raised: Hsla,
    /// A card resting on the content plane.
    pub surface_card: Hsla,
    /// A floating surface: sheet, menu, popover.
    pub surface_overlay: Hsla,
    /// An input plate.
    pub input_bg: Hsla,

    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,

    pub border: Hsla,
    pub border_strong: Hsla,

    /// Maximum-contrast solid fill, and the label that rides on it.
    pub solid: Hsla,
    pub on_solid: Hsla,

    /// Interaction accent: the selection ring, the progress bar, focus.
    pub accent: Hsla,
    /// The accent repaired to clear 3:1 against the content plane, so a ring or
    /// a focus outline stays visible on either appearance.
    pub accent_strong: Hsla,
    pub on_accent: Hsla,
    pub accent_wash: Hsla,

    pub element_hover: Hsla,
    pub element_active: Hsla,

    /// Modal backdrop.
    pub scrim: Hsla,
}

impl Design {
    /// Resolve the planes from the active component-library palette.
    pub fn new(theme: &Theme) -> Self {
        let dark = theme.is_dark();
        let appearance = if dark {
            Appearance::Dark
        } else {
            Appearance::Light
        };
        let surface_treatment = surface_preference().resolve(RECOMMENDED_TREATMENT);

        let bg = theme.background;
        let surface = theme.title_bar;
        // Elevation keeps Zeron's steps (+0.031 card, +0.055 raised, +0.063
        // overlay) rather than its absolute tones. Light lands everything on the
        // content plane and lets the border and shadow separate them.
        let step = |delta: f32| {
            if dark {
                mix(bg, color::grey(255), delta)
            } else {
                bg
            }
        };
        let surface_raised = if dark {
            step(0.055)
        } else {
            mix(bg, color::grey(0), 0.045)
        };
        // The library authors these as a pair: an opaque selected-border colour
        // for the ring, a translucent tint for the wash. `theme.selection` alone
        // is 30% alpha, so as a ring it composites into the plane behind it — the
        // teal ring that no contrast repair could rescue.
        let accent = theme.list_active_border;
        let accent_strong = lighten_to_contrast(accent, bg, 3.0);
        Self {
            appearance,
            surface_treatment,
            bg,
            surface,
            surface_raised,
            surface_card: step(0.031),
            surface_overlay: step(0.063),
            input_bg: ink(dark, 0.03),
            text: theme.foreground,
            text_muted: theme.muted_foreground,
            text_faint: mix(theme.muted_foreground, bg, 0.35),
            border: theme.border,
            border_strong: hairline(dark, 0.14),
            solid: theme.primary,
            on_solid: theme.primary_foreground,
            accent,
            accent_strong,
            on_accent: best_on(accent_strong),
            accent_wash: theme.selection,
            element_hover: wash(dark, 0.11),
            element_active: wash(dark, 0.16),
            scrim: if dark {
                color::grey(0).opacity(color::SCRIM_ALPHA_DARK)
            } else {
                color::grey(0).opacity(0.35)
            },
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self.appearance, Appearance::Dark)
    }

    /// Translucent fill ink at `alpha`, in dark-mode terms.
    pub fn ink(self, alpha: f32) -> Hsla {
        ink(self.is_dark(), alpha)
    }

    /// Translucent hairline ink at `alpha`, in dark-mode terms.
    pub fn hairline(self, alpha: f32) -> Hsla {
        hairline(self.is_dark(), alpha)
    }

    /// Interactive wash at `alpha`, in dark-mode terms.
    pub fn wash(self, alpha: f32) -> Hsla {
        wash(self.is_dark(), alpha)
    }

    /// The backdrop a translucent surface must stay legible against when the
    /// compositor shows the worst case: white behind a dark appearance, black
    /// behind a light one.
    pub fn adverse_backdrop(self) -> Hsla {
        if self.is_dark() {
            color::grey(255)
        } else {
            color::grey(0)
        }
    }

    /// Composite a translucent colour as it will actually be painted on a plane.
    pub fn on_plane(self, color: Hsla, plane: Hsla) -> Hsla {
        flatten(color, plane)
    }
}

// The preference lives in a process-wide mirror (see above) so `Design::new`
// also works from pure contexts that have no `App`.

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A `Design` for `appearance` with the given treatment.
    ///
    /// `Theme::from` leaves `mode` at its own Light default, so a dark palette
    /// would otherwise be resolved by the light code path and dark assertions
    /// would pass by coincidence.
    pub(crate) fn design(appearance: Appearance, treatment: SurfaceTreatment) -> Design {
        let colors = match appearance {
            Appearance::Dark => gpui_kit::component::ThemeColor::dark(),
            Appearance::Light => gpui_kit::component::ThemeColor::light(),
        };
        let mut theme = Theme::from(colors.as_ref());
        theme.mode = match appearance {
            Appearance::Dark => gpui_kit::component::ThemeMode::Dark,
            Appearance::Light => gpui_kit::component::ThemeMode::Light,
        };
        let mut design = Design::new(&theme);
        design.surface_treatment = treatment;
        design
    }
}

/// Read the resolved design for the given app.
pub trait ActiveDesign {
    fn design(&self) -> Design;
}

impl ActiveDesign for App {
    fn design(&self) -> Design {
        Design::new(self.theme())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::design::color::painted_contrast;

    #[test]
    fn surface_preference_resolution_follows_the_policy() {
        let recommended = SurfaceTreatment::Frosted;
        assert_eq!(
            SurfacePreference::ThemeDefault.resolve(recommended),
            SurfaceTreatment::Frosted
        );
        assert_eq!(
            SurfacePreference::ThemeDefault.resolve(SurfaceTreatment::Opaque),
            SurfaceTreatment::Opaque
        );
        assert_eq!(
            SurfacePreference::Opaque.resolve(recommended),
            SurfaceTreatment::Opaque
        );
        assert_eq!(
            SurfacePreference::Frosted.resolve(SurfaceTreatment::Opaque),
            SurfaceTreatment::Frosted
        );
    }

    #[test]
    fn the_surface_preference_round_trips() {
        for preference in SurfacePreference::ALL {
            let encoded = match preference {
                SurfacePreference::ThemeDefault => 0,
                SurfacePreference::Frosted => 1,
                SurfacePreference::Opaque => 2,
            };
            SURFACE.store(encoded, Ordering::Relaxed);
            assert_eq!(surface_preference(), preference);
        }
        SURFACE.store(0, Ordering::Relaxed);
    }

    #[test]
    fn a_repaired_accent_keeps_the_hue_the_palette_chose() {
        // The accent is a blue. Repairing its contrast by mixing toward white
        // walked the hue to teal and dropped most of its saturation, which is a
        // different colour; lightness alone must not.
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance);
            assert_eq!(
                design.accent_strong.h, design.accent.h,
                "{appearance:?} hue moved"
            );
            assert_eq!(
                design.accent_strong.s, design.accent.s,
                "{appearance:?} saturation moved"
            );
        }
    }

    #[test]
    fn chrome_recedes_from_the_content_plane_in_both_appearances() {
        // The one direction a naive inversion gets backwards: on dark the chrome
        // plane sits *above* the content plane, on light it sits *below* it.
        let dark = design(Appearance::Dark);
        let light = design(Appearance::Light);
        assert!(dark.surface.l > dark.bg.l, "dark chrome must be lighter");
        assert!(light.surface.l < light.bg.l, "light chrome must be darker");
    }

    #[test]
    fn the_elevation_ladder_climbs_in_one_direction_only() {
        // Collapsing these steps visibly lifts a floating surface off its
        // intended plane.
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance);
            assert!(
                design.surface_overlay.l >= design.surface_card.l,
                "{appearance:?}"
            );
        }
    }

    #[test]
    fn text_tones_step_down_from_the_foreground() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance);
            let foreground = painted_contrast(design.text, design.bg);
            let muted = painted_contrast(design.text_muted, design.bg);
            let faint = painted_contrast(design.text_faint, design.bg);
            assert!(foreground > muted, "{appearance:?} muted out of order");
            assert!(muted >= faint, "{appearance:?} faint out of order");
            assert!(faint > 1.0, "{appearance:?} faint is invisible");
        }
    }

    #[test]
    fn the_ring_is_opaque_and_the_wash_is_not() {
        // `theme.selection` is a translucent wash. As a ring it composites into
        // the plane behind it, which is what produced a teal ring from a blue
        // accent, so the two roles must not be confused.
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance);
            assert_eq!(
                design.accent_strong.a, 1.0,
                "{appearance:?} ring is translucent"
            );
            assert!(design.accent_wash.a < 0.35, "{appearance:?} wash is a fill");
        }
    }

    fn design(appearance: Appearance) -> Design {
        test_support::design(appearance, SurfaceTreatment::Frosted)
    }
}
