//! The window's regions. Each module renders one part of the gallery window
//! from `Gallery` state; `view.rs` composes them.

mod failures;
mod inspector;
mod photo_grid;
mod sidebar;
mod status_bar;
mod toolbar;
mod viewer;

use gpui::{ObjectFit, Styled as _, StyledImage as _, img};
use std::path::Path;

/// A photo scaled to fit its frame without cropping.
pub(super) fn framed_image(path: &std::sync::Arc<Path>) -> gpui::Img {
    img(path.clone())
        .absolute()
        .size_full()
        .object_fit(ObjectFit::Contain)
}
