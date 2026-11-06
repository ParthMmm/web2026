//! The app's palette and type, installed as the component library's light and
//! dark themes.
//!
//! `theme.json` is the single source of the colours and fonts. The library
//! re-applies the matching variant on every appearance change, so its own
//! widgets (buttons, menus, scrollbars) and [`super::Design`], which is
//! derived from the active theme, always agree.

use gpui::App;
use gpui_component::{Theme, ThemeConfig, ThemeSet};
use std::rc::Rc;

const THEMES: &str = include_str!("theme.json");

/// Make `theme.json` the active light and dark theme. Call after
/// `gpui_component::init` and before the first appearance sync.
pub fn install(cx: &mut App) {
    let (light, dark) = variants();
    let theme = Theme::global_mut(cx);
    theme.light_theme = Rc::new(light);
    theme.dark_theme = Rc::new(dark);
}

/// The light and dark variants. `theme.json` is compiled in, so a malformed
/// file is a build defect, caught by the test below.
fn variants() -> (ThemeConfig, ThemeConfig) {
    let set: ThemeSet = serde_json::from_str(THEMES).expect("theme.json parses");
    let mut light = None;
    let mut dark = None;
    for config in set.themes {
        if config.mode.is_dark() {
            dark = Some(config);
        } else {
            light = Some(config);
        }
    }
    (
        light.expect("theme.json has a light theme"),
        dark.expect("theme.json has a dark theme"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_appearances_use_the_app_type() {
        let (light, dark) = variants();
        for config in [light, dark] {
            assert_eq!(config.font_family.as_deref(), Some("Geist"));
            assert_eq!(config.mono_font_family.as_deref(), Some("Geist Mono"));
        }
    }
}
