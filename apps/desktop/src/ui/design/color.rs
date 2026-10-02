//! Colour math for the design system.
//!
//! Ported from Zeron `crates/ui/src/theme.rs` (MIT, Copyright (c) 2026 Wing);
//! `docs/photo-gallery/zeron-reference.md` §1.2 and §1.4 carry the reasoning.
//!
//! Contrast is always measured on the composited result ([`flatten`]): a
//! translucent tint over an unknown backdrop is not what the eye receives.

use gpui_kit::{Hsla, hsla};

/// Alpha multiplier for **fills** (hover washes, chip and pill backgrounds).
///
/// Alphas are quoted in dark-mode terms and the light value is derived. A blanket
/// light multiplier was tried upstream and removed: it turned a 3% ink into 1.5%
/// black on white, which is nothing.
pub const INK_FILL_SCALE: f32 = 1.0;

/// Alpha multiplier for **hairlines** (borders, dividers, rings).
pub const INK_HAIRLINE_SCALE: f32 = 1.35;

/// Alpha of the standard modal backdrop in dark mode. Light appearances pass a
/// smaller value, because a black scrim reads heavier on a bright field.
pub const SCRIM_ALPHA_DARK: f32 = 0.60;

/// An achromatic tone straight from an 8-bit grey value.
pub fn grey(value: u8) -> Hsla {
    hsla(0.0, 0.0, f32::from(value) / 255.0, 1.0)
}

/// Translucent **fill** ink: soft white on dark, soft black on light.
pub fn ink(dark: bool, alpha: f32) -> Hsla {
    if dark {
        hsla(0.0, 0.0, 1.0, alpha)
    } else {
        hsla(0.0, 0.0, 0.0, alpha * INK_FILL_SCALE)
    }
}

/// Translucent **hairline** ink, scaled so a 1px edge survives a bright field.
pub fn hairline(dark: bool, alpha: f32) -> Hsla {
    if dark {
        hsla(0.0, 0.0, 1.0, alpha)
    } else {
        hsla(0.0, 0.0, 0.0, (alpha * INK_HAIRLINE_SCALE).min(0.5))
    }
}

/// Interactive-state wash. Softer than [`ink`] so hover plates read as tinted
/// glass rather than paint.
pub fn wash(dark: bool, alpha: f32) -> Hsla {
    if dark {
        hsla(0.0, 0.0, 0.92, alpha)
    } else {
        hsla(0.0, 0.0, 0.10, alpha * INK_FILL_SCALE)
    }
}

/// Linear per-component mix. Both endpoints sit close enough on the wheel that
/// shortest-arc hue handling is not needed for this palette.
pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    hsla(
        lerp(a.h, b.h),
        lerp(a.s, b.s),
        lerp(a.l, b.l),
        lerp(a.a, b.a),
    )
}

/// Composite a possibly translucent `fg` over an opaque `bg`.
///
/// Returns the colour the eye receives, which is what the contrast checks here
/// measure.
pub fn flatten(fg: Hsla, bg: Hsla) -> Hsla {
    let a = fg.a.clamp(0.0, 1.0);
    let f = fg.to_rgb();
    let b = bg.to_rgb();
    let (h, s, l) = rgb_to_hsl(
        f.r * a + b.r * (1.0 - a),
        f.g * a + b.g * (1.0 - a),
        f.b * a + b.b * (1.0 - a),
    );
    hsla(h, s, l, 1.0)
}

/// sRGB components in `0..1` to HSL in `0..1`, gpui's `Hsla` convention.
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    (h, s, l)
}

/// HSL in `0..1` to sRGB components in `0..1`.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= f32::EPSILON {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// WCAG 2.1 relative luminance of an opaque colour.
pub fn relative_luminance(color: Hsla) -> f32 {
    let linear = |c: f32| {
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = hsl_to_rgb(color.h, color.s, color.l);
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// WCAG 2.1 contrast ratio between two opaque colours, in `1.0..=21.0`.
pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Contrast ratio of `foreground` as it is actually painted on `background`.
pub fn painted_contrast(foreground: Hsla, background: Hsla) -> f32 {
    contrast_ratio(flatten(foreground, background), background)
}

/// Move `color` toward black or white until it reaches `minimum` contrast
/// against `background`. The nearer endpoint is preferred, so the repair keeps
/// as much of the original colour as it can.
pub fn ensure_contrast(color: Hsla, background: Hsla, minimum: f32) -> Hsla {
    if painted_contrast(color, background) >= minimum {
        return color;
    }
    let target = if contrast_ratio(grey(0), background) > contrast_ratio(grey(255), background) {
        grey(0)
    } else {
        grey(255)
    };
    for step in 1..=20 {
        #[allow(clippy::cast_precision_loss)]
        let candidate = mix(color, target, step as f32 / 20.0);
        if painted_contrast(candidate, background) >= minimum {
            return candidate;
        }
    }
    target
}

/// The endpoint with the higher contrast against `color`, so a label painted
/// on top of `color` clears its ratio as easily as the palette allows.
pub fn best_on(color: Hsla) -> Hsla {
    if contrast_ratio(grey(255), color) >= contrast_ratio(grey(0), color) {
        grey(255)
    } else {
        grey(0)
    }
}

/// Raise `color`'s lightness — and nothing else — until it reaches `minimum`
/// contrast against `background`, keeping the hue and saturation the palette
/// chose. When no lightness passes, return the candidate with the highest one.
///
/// Mixing toward white is not a repair: it walks the hue across the wheel and
/// drains the chroma. Measured on this palette, a blue accent at 223° came out at
/// 169° with a third of its saturation.
pub fn lighten_to_contrast(color: Hsla, background: Hsla, minimum: f32) -> Hsla {
    if painted_contrast(color, background) >= minimum {
        return color;
    }
    let steps = 40;
    let mut best = color;
    let mut best_shift = f32::INFINITY;
    let mut fallback = color;
    let mut fallback_contrast = painted_contrast(color, background);
    for step in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let lightness = step as f32 / steps as f32;
        let candidate = Hsla {
            l: lightness,
            ..color
        };
        let contrast = painted_contrast(candidate, background);
        if contrast >= minimum {
            let shift = (lightness - color.l).abs();
            if shift < best_shift {
                best_shift = shift;
                best = candidate;
            }
        } else if contrast > fallback_contrast {
            fallback_contrast = contrast;
            fallback = candidate;
        }
    }
    if best_shift.is_finite() {
        best
    } else {
        // Nothing passed. Hand back the closest candidate rather than a colour
        // that fails silently.
        fallback
    }
}

/// Raise a translucent tint's coverage until every `(colour, minimum)` pair in
/// `checks` clears its ratio against the composited result, from `base` to fully
/// opaque. A delicate palette gets a denser material; it never gets unreadable
/// text.
pub fn contrast_checked_alpha(
    tint: Hsla,
    backdrop: Hsla,
    base: f32,
    checks: &[(Hsla, f32)],
) -> f32 {
    let base = base.clamp(0.0, 1.0);
    for step in 0..=20 {
        #[allow(clippy::cast_precision_loss)]
        let alpha = base + (1.0 - base) * step as f32 / 20.0;
        let composite = flatten(tint.opacity(alpha), backdrop);
        if checks
            .iter()
            .all(|(color, minimum)| painted_contrast(*color, composite) >= *minimum)
        {
            return alpha;
        }
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_returns_the_background_for_a_fully_transparent_foreground() {
        let backdrop = grey(20);
        assert!((flatten(ink(true, 0.0), backdrop).l - backdrop.l).abs() < 1e-6);
    }

    #[test]
    fn flatten_returns_the_foreground_for_a_fully_opaque_one() {
        let foreground = grey(200);
        assert!((flatten(foreground, grey(20)).l - foreground.l).abs() < 1e-6);
    }

    #[test]
    fn contrast_ratio_matches_the_known_black_on_white_extremes() {
        assert!((contrast_ratio(grey(0), grey(255)) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(grey(255), grey(255)) - 1.0).abs() < 1e-6);
        assert!((contrast_ratio(grey(0), grey(0)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ensure_contrast_leaves_a_compliant_colour_alone() {
        let colour = grey(240);
        assert_eq!(ensure_contrast(colour, grey(0), 4.5).l, colour.l);
    }

    #[test]
    fn ensure_contrast_repairs_a_colour_that_fails() {
        let repaired = ensure_contrast(grey(120), grey(0), 4.5);
        assert!(painted_contrast(repaired, grey(0)) >= 4.5);
    }

    #[test]
    fn contrast_checked_alpha_keeps_the_requested_coverage_when_it_already_passes() {
        // Black text over a light tint on a light backdrop needs no extra ink.
        let alpha = contrast_checked_alpha(grey(255), grey(255), 0.45, &[(grey(0), 4.5)]);
        assert!((alpha - 0.45).abs() < 1e-6);
    }

    #[test]
    fn contrast_checked_alpha_densifies_the_tint_until_text_passes() {
        // A white tint over a black backdrop: raising its coverage makes the
        // composite brighter, so black text eventually clears 6:1.
        let checks = [(grey(0), 6.0)];
        let alpha = contrast_checked_alpha(grey(255), grey(0), 0.45, &checks);
        assert!(alpha > 0.45, "coverage had to rise");
        let composite = flatten(grey(255).opacity(alpha), grey(0));
        assert!(painted_contrast(grey(0), composite) >= 6.0);
    }

    #[test]
    fn contrast_checked_alpha_stops_at_full_coverage_when_the_check_is_impossible() {
        // White text over a white tint on a white backdrop can never clear a
        // ratio above 1. The contract is "return the first passing alpha, else
        // the densest one": never silently ship unreadable ink, never lock the
        // material away either.
        let alpha = contrast_checked_alpha(grey(255), grey(255), 0.45, &[(grey(255), 1.5)]);
        assert!(alpha > 0.45);
        assert!((alpha - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hairline_ink_scales_up_only_on_light_appearances() {
        assert!((hairline(true, 0.08).a - 0.08).abs() < 1e-6);
        assert!((hairline(false, 0.08).a - 0.08 * INK_HAIRLINE_SCALE).abs() < 1e-6);
    }

    #[test]
    fn hairline_ink_stops_at_half_coverage_on_light_appearances() {
        assert!((hairline(false, 0.9).a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fill_ink_keeps_the_same_alpha_in_both_appearances() {
        assert!((ink(true, 0.03).a - ink(false, 0.03).a).abs() < 1e-6);
    }

    #[test]
    fn mix_endpoints_are_exact() {
        let (a, b) = (grey(0), grey(100));
        assert!((mix(a, b, 0.0).l - a.l).abs() < 1e-6);
        assert!((mix(a, b, 1.0).l - b.l).abs() < 1e-6);
        assert!((mix(a, b, 0.5).l - (a.l + b.l) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn best_on_picks_the_readable_endpoint() {
        assert!((best_on(grey(0)).l - 1.0).abs() < 1e-6);
        assert!((best_on(grey(255)).l - 0.0).abs() < 1e-6);
    }

    #[test]
    fn rgb_and_hsl_round_trip() {
        for value in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let (h, s, l) = rgb_to_hsl(value, value, value);
            let [r, g, b] = hsl_to_rgb(h, s, l);
            assert!((r - value).abs() < 1e-5, "{value} -> {r}");
            assert!((g - value).abs() < 1e-5, "{value} -> {g}");
            assert!((b - value).abs() < 1e-5, "{value} -> {b}");
        }
    }
}
