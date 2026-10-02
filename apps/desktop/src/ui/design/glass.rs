//! Glass: the window material, translucent-surface tints, lit plates, and scroll
//! edge scrims.
//!
//! Ported from Zeron `crates/ui/src/glass.rs`, `frost.rs` and the glass section of
//! `theme.rs` (MIT, Copyright (c) 2026 Wing). See
//! `docs/photo-gallery/zeron-reference.md` Part 2.
//!
//! Window glass is real: the window asks the platform to blur what is behind it
//! and the chrome paints a translucent tint, so the desktop shows through blurred.
//! That only works while some surface is actually translucent over the backdrop —
//! see Part 10, fourth pass.
//!
//! In-app frost is not available at this revision (no backdrop-blur draw call), so
//! [`frosted`] paints the tint, border and shadow and nothing else; adding the blur
//! is a one-line change and the sigma to use is 16.

use gpui_kit::{
    Background, BoxShadow, Div, Hsla, IntoElement, ParentElement as _, Styled,
    WindowBackgroundAppearance, div, linear_color_stop, linear_gradient, point,
    prelude::FluentBuilder as _, px,
};

use super::Design;
use super::color::{contrast_checked_alpha, ensure_contrast, flatten, grey};

/// Window-tint coverage on platforms that composite a blurred backdrop.
pub const GLASS_ALPHA: f32 = 0.80;

/// Light-mode coverage. A light tint controls a blur less than a dark one, so
/// light frost runs at the same alpha to keep the chrome on a known enough
/// background for its labels.
pub const GLASS_ALPHA_LIGHT: f32 = 0.80;

/// Whether this platform can blur the desktop behind the window. Linux stays
/// opaque: compositor blur is not guaranteed there.
pub const fn platform_supports_window_glass() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

/// How the platform should composite the window behind this paint.
///
/// Must be *re-applied* after a theme or appearance change: gpui's macOS backend
/// tears the visual-effect view out of the hierarchy whenever the value is
/// anything but `Blurred`, so a one-shot set at window creation dies on the first
/// switch.
pub fn window_background(design: Design) -> WindowBackgroundAppearance {
    if design.is_glass() {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

impl Design {
    /// Requested coverage, before the contrast check.
    fn base_glass_alpha(self) -> f32 {
        if !platform_supports_window_glass() {
            return 1.0;
        }
        match self.appearance {
            super::Appearance::Dark => GLASS_ALPHA,
            super::Appearance::Light => GLASS_ALPHA_LIGHT,
        }
    }

    /// Coverage raised until the chrome's own text stays legible against the
    /// worst desktop the compositor could place behind it.
    fn glass_alpha(self) -> f32 {
        contrast_checked_alpha(
            self.surface,
            self.adverse_backdrop(),
            self.base_glass_alpha(),
            &[(self.text, 4.5), (self.text_muted, 3.0)],
        )
    }

    /// The tint painted over the window's blurred backdrop.
    pub fn glass(self) -> Hsla {
        if self.surface_treatment == super::SurfaceTreatment::Opaque {
            return self.surface;
        }
        self.surface.opacity(self.glass_alpha())
    }

    /// Whether this appearance paints translucent chrome over a blurred desktop.
    ///
    /// Gate window-glass recipes on this, not on the platform constant: the
    /// constant is platform-wide, the alpha is per-appearance.
    pub fn is_glass(self) -> bool {
        self.glass().a < 1.0
    }

    /// Whether floating surfaces paint their translucent tint. Scene-level, unlike
    /// [`Self::is_glass`]: it needs no compositor support.
    pub fn is_frost(self) -> bool {
        self.surface_treatment == super::SurfaceTreatment::Frosted
    }

    /// The chrome plane as painted: glass when glass is on, flat otherwise.
    pub fn chrome(self) -> Hsla {
        self.glass()
    }

    /// The content plane as painted. Always opaque, unlike Zeron's `panel_bg`:
    /// this is the surround for the photographs, and the gaps between tiles must
    /// not show the desktop.
    pub fn panel(self) -> Hsla {
        self.bg
    }

    /// A floating surface resting on the content plane.
    pub fn card_bg(self) -> Hsla {
        if !self.is_frost() {
            return self.surface_card;
        }
        let window = flatten(self.glass(), self.adverse_backdrop());
        self.surface_card.opacity(contrast_checked_alpha(
            self.surface_card,
            window,
            0.40,
            &[(self.text, 4.5)],
        ))
    }

    /// A floating surface that carries its own labels: a bar over a photograph,
    /// a menu, a popover.
    pub fn overlay_bg(self) -> Hsla {
        if !self.is_frost() {
            return self.surface_overlay;
        }
        match self.appearance {
            // Light glass keeps the blurred scene visible. A contrast guard
            // against solid black raised this to near-opaque, which is opaque
            // material wearing the word "frost". Strengthen the foreground
            // instead; see [`Self::overlay_text`].
            super::Appearance::Light => self.surface_overlay.opacity(0.45),
            super::Appearance::Dark => self.surface_overlay.opacity(contrast_checked_alpha(
                self.surface_overlay,
                self.adverse_backdrop(),
                0.50,
                &[(self.text, 4.5)],
            )),
        }
    }

    /// An input plate: a search field, a chip that accepts typing.
    pub fn field_bg(self) -> Hsla {
        let tint = flatten(self.input_bg, self.bg);
        if !self.is_frost() {
            return tint;
        }
        if matches!(self.appearance, super::Appearance::Light) {
            return tint.opacity(0.35);
        }
        let window = flatten(self.glass(), self.adverse_backdrop());
        self.input_bg.opacity(contrast_checked_alpha(
            self.input_bg,
            window,
            self.input_bg.a,
            &[(self.text, 4.5)],
        ))
    }

    /// Text colours that stay readable on a translucent floating surface.
    ///
    /// On a light appearance the fill must not be thickened, because that kills
    /// the material, so the foreground is repaired instead.
    pub fn overlay_text(self) -> (Hsla, Hsla) {
        if !self.is_frost() {
            return (self.text, self.text_muted);
        }
        let background = flatten(
            self.overlay_bg(),
            flatten(self.glass(), self.adverse_backdrop()),
        );
        (
            ensure_contrast(self.text, background, 4.5),
            ensure_contrast(self.text_muted, background, 4.5),
        )
    }

    /// Drop shadow for a floating surface: a soft ambient pass and a tighter
    /// contact pass.
    pub fn overlay_shadows(self) -> Vec<BoxShadow> {
        let strength = if self.is_dark() { 0.45 } else { 0.16 };
        vec![
            drop_shadow(grey(0).opacity(strength * 0.5), 8.0, 24.0, -4.0),
            drop_shadow(grey(0).opacity(strength), 1.0, 3.0, 0.0),
        ]
    }
}

fn drop_shadow(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    }
}

fn inset_shadow(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        inset: true,
        ..drop_shadow(color, y, blur, spread)
    }
}

fn white(alpha: f32) -> Hsla {
    grey(255).opacity(alpha)
}

fn black(alpha: f32) -> Hsla {
    grey(0).opacity(alpha)
}

fn vertical(top: Hsla, bottom: Hsla) -> Background {
    linear_gradient(
        180.0,
        linear_color_stop(top, 0.0),
        linear_color_stop(bottom, 1.0),
    )
}

/// One lit glass surface: a lit gradient, a hairline rim, and its shadows.
pub struct Plate {
    background: Background,
    rim: Hsla,
    shadows: Vec<BoxShadow>,
}

impl Plate {
    /// Apply the plate to an element the way a styled node would.
    pub fn apply<E: Styled>(self, element: E) -> E {
        element
            .bg(self.background)
            .border_1()
            .border_color(self.rim)
            .shadow(self.shadows)
    }

    /// The rim alone, for a caller that paints its own fill.
    fn rim(&self) -> Hsla {
        self.rim
    }
}

/// Neutral plate: a track, a chip, an unselected control, the frame around a
/// photograph.
///
/// `t` fades the whole treatment in, so one recipe serves both the resting and
/// the hovered state of a control. Dark plates lift translucent white off the
/// surface; light plates sink a translucent tint into it and light the inside
/// of the bottom edge, because a near-white plate on a near-white surface
/// otherwise washes out.
pub fn light_plate(design: Design, t: f32) -> Plate {
    let t = t.clamp(0.0, 1.0);
    if design.is_dark() {
        return Plate {
            background: vertical(white(0.08 * t), white(0.05 * t)),
            rim: white(0.09 * t),
            shadows: vec![
                inset_shadow(white(0.07 * t), 1.0, 0.0, 0.0),
                drop_shadow(black(0.16 * t), 1.0, 2.0, 0.0),
            ],
        };
    }
    Plate {
        background: vertical(black(0.075 * t), black(0.04 * t)),
        rim: black(0.08 * t),
        // Inset only. gpui paints drop shadows under the whole box, so an outer
        // lip would show through the translucent fill and whiten it.
        shadows: vec![
            inset_shadow(black(0.08 * t), 1.0, 2.0, 0.0),
            inset_shadow(white(0.55 * t), -1.0, 0.0, 0.0),
        ],
    }
}

/// The selection treatment for a tile: a rim plus a coloured halo, lit the same
/// way a plate is.
///
/// Zeron reaches for an accent *plate* here. This app's filled controls are
/// library buttons, so the accent has no plate to sit on and the ring is what
/// the design actually needs.
/// The selection treatment for a tile: a rim plus a coloured halo, lit the same
/// way a plate is.
///
/// `focused` says whether the region that owns the tiles has keyboard focus. On
/// a desktop file browser the selected item *is* the focus indicator while the
/// view is focused, and it has to read differently when it is not — otherwise a
/// selection made in an unfocused window is indistinguishable from a focused
/// one, and keyboard focus has no cue of its own. Unfocused, the rim drops to a
/// translucent accent and the halo goes.
///
/// Zeron reaches for an accent *plate* here. This app's filled controls are
/// library buttons, so the accent has no plate to sit on and the ring is what
/// the design actually needs.
pub fn selection_ring(
    design: Design,
    selected: bool,
    focused: bool,
    glow: f32,
) -> (Hsla, Vec<BoxShadow>) {
    if !selected {
        // Quiet. An unselected tile is a photograph on a plane, not a framed
        // object: giving every tile a visible rim is what made the grid read as
        // a field of outlined boxes.
        return (gpui_kit::transparent_black(), Vec::new());
    }
    if !focused {
        return (design.accent_strong.opacity(0.45), Vec::new());
    }
    let glow = glow.clamp(0.0, 1.0);
    let accent = design.accent_strong;
    (
        accent,
        vec![
            inset_shadow(white(0.10), 0.0, 0.0, 1.0),
            drop_shadow(
                accent.opacity(0.22 + 0.28 * glow),
                0.0,
                6.0 + 14.0 * glow,
                0.0,
            ),
        ],
    )
}

/// A floating card: a bar over a photograph, a menu, a popover.
///
/// **Without a backdrop blur this is a tinted card, not frosted glass.** The
/// tint, border, radius and shadow are the resolved recipe; only the blur pass
/// is missing. Wrap the child in a paint scope here and call
/// `window.paint_backdrop_blur(bounds, corners, px(16.0))` once the primitive
/// exists.
pub fn frosted(design: Design, corner_radius: f32, child: impl IntoElement) -> Div {
    div()
        .rounded(px(corner_radius))
        .bg(design.overlay_bg())
        .border_1()
        .border_color(design.border)
        .shadow(design.overlay_shadows())
        .child(child)
}

/// Gradient scrims that fade scrolling content into `plane` at its top and
/// bottom edges.
///
/// An overlay is exact where `plane` is opaque, which covers the content plane
/// and every floating card. Over glass it is an approximation: the scrim tints
/// the blurred backdrop as well as fading the content, where Zeron's per-pixel
/// edge fade would fade only the content. Pass the plane the scrolled content
/// actually sits on.
pub fn edge_scrim(plane: Hsla, band: f32, top: bool, bottom: bool, child: impl IntoElement) -> Div {
    let top_band = || {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(band))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(plane, 0.0),
                linear_color_stop(plane.opacity(0.0), 1.0),
            ))
    };
    let bottom_band = || {
        div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(band))
            .bg(linear_gradient(
                0.0,
                linear_color_stop(plane, 0.0),
                linear_color_stop(plane.opacity(0.0), 1.0),
            ))
    };
    div()
        .relative()
        .size_full()
        .child(child)
        .when(top, |this| this.child(top_band()))
        .when(bottom, |this| this.child(bottom_band()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::design::Appearance;
    use crate::ui::design::SurfaceTreatment;
    use crate::ui::design::color::painted_contrast;
    use crate::ui::design::test_support::design;

    #[test]
    fn opaque_treatment_never_frosts_anything() {
        let design = design(Appearance::Dark, SurfaceTreatment::Opaque);
        assert_eq!(design.glass().a, 1.0);
        assert!(!design.is_glass());
        assert!(!design.is_frost());
        assert_eq!(design.overlay_bg().a, 1.0);
        assert_eq!(design.card_bg().a, 1.0);
    }

    #[test]
    fn dark_frost_makes_the_chrome_translucent() {
        let frost = design(Appearance::Dark, SurfaceTreatment::Frosted);
        if platform_supports_window_glass() {
            assert!(frost.glass().a < 1.0, "chrome should be translucent");
            assert!(frost.is_glass());
        } else {
            assert_eq!(frost.glass().a, 1.0);
        }
    }

    #[test]
    fn every_appearance_keeps_its_text_legible_over_glass() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance, SurfaceTreatment::Frosted);
            let worst_case = flatten(design.glass(), design.adverse_backdrop());
            assert!(
                painted_contrast(design.text, worst_case) >= 4.5,
                "{appearance:?} primary text fails on its own glass"
            );
            assert!(
                painted_contrast(design.text_muted, worst_case) >= 3.0,
                "{appearance:?} muted text fails on its own glass"
            );
        }
    }

    #[test]
    fn light_frost_keeps_the_scene_visible() {
        let design = design(Appearance::Light, SurfaceTreatment::Frosted);
        assert!(
            design.overlay_bg().a <= 0.5,
            "a light overlay must not become opaque material"
        );
    }

    #[test]
    fn the_accent_clears_the_ring_requirement_on_the_content_plane() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let design = design(appearance, SurfaceTreatment::Frosted);
            assert!(
                painted_contrast(design.accent_strong, design.bg) >= 3.0,
                "{appearance:?} accent fails the 3:1 ring requirement"
            );
        }
    }

    #[test]
    fn neutral_plates_never_drop_an_outer_shadow_on_light() {
        let light = light_plate(design(Appearance::Light, SurfaceTreatment::Frosted), 1.0);
        assert!(
            light.shadows.iter().all(|shadow| shadow.inset),
            "an outer lip whitens a translucent light plate"
        );
    }

    #[test]
    fn a_faded_plate_has_no_visible_edges() {
        let plate = light_plate(design(Appearance::Dark, SurfaceTreatment::Frosted), 0.0);
        assert!(plate.rim().a.abs() < 1e-6);
        assert!(
            plate
                .shadows
                .iter()
                .all(|shadow| shadow.color.a.abs() < 1e-6)
        );
    }

    #[test]
    fn an_unselected_tile_is_quiet() {
        let design = design(Appearance::Dark, SurfaceTreatment::Frosted);
        let (rim, shadows) = selection_ring(design, false, true, 0.0);
        assert_eq!(
            rim.a, 0.0,
            "an unselected tile must not carry a visible rim"
        );
        assert!(shadows.is_empty());
    }

    #[test]
    fn an_unfocused_selection_still_reads_but_does_not_glow() {
        let design = design(Appearance::Dark, SurfaceTreatment::Frosted);
        let (rim, shadows) = selection_ring(design, true, false, 1.0);
        assert!(rim.a < 1.0, "an unfocused selection must read as dimmed");
        assert!(
            shadows.is_empty(),
            "an unfocused selection must not carry a halo"
        );
    }

    #[test]
    fn a_selected_tile_glows_brighter_with_hover() {
        let design = design(Appearance::Dark, SurfaceTreatment::Frosted);
        let (_, resting) = selection_ring(design, true, true, 0.0);
        let (_, hovered) = selection_ring(design, true, true, 1.0);
        assert!(
            hovered.last().expect("halo").color.a > resting.last().expect("halo").color.a,
            "hover should spread the halo"
        );
    }
}
