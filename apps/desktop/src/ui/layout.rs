//! Window geometry: which side panels fit, and how the photo grid divides the
//! width that is left.
//!
//! The grid is a `uniform_list`, which needs exact row heights, so the
//! geometry is computed here rather than left to flex layout.

use super::benchmark;
use super::design::{SIDEBAR_WIDTH, SPACE_LG};
use gpui::{Pixels, px};

pub(super) const GRID_GAP: f32 = 6.0;
/// Padding on each side of the grid. Wider than the gap between tiles,
/// because the window edge is a separate group from the next tile.
pub(super) const GRID_GUTTER: f32 = SPACE_LG;
/// The smallest tile the grid lays out, at the default zoom.
pub(super) const DEFAULT_TILE_WIDTH: f32 = 168.0;
/// The zoom range, as the smallest tile width a column may take.
pub(super) const TILE_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 112.0..=360.0;
/// How far one Zoom In or Zoom Out step moves the smallest tile width.
pub(super) const TILE_WIDTH_STEP: f32 = 32.0;
/// Square tiles: every photo gets the same weight in the grid, whatever its
/// orientation.
const TILE_ASPECT: f32 = 1.0;
pub(super) const INSPECTOR_WIDTH: f32 = 300.0;
const MIN_WIDTH_FOR_INSPECTOR: f32 = 760.0;

/// What the geometry depends on.
#[derive(Clone, Copy)]
pub(super) struct LayoutInput {
    pub(super) viewport_width: f32,
    /// The benchmark's scripted scroll assumes a fixed column count, and its
    /// preview indices come from `benchmark.py`'s warm preparation, so the
    /// sidebar is hidden there to keep the geometry the script measured.
    pub(super) benchmarking: bool,
    /// The person hid the sidebar from the View menu.
    pub(super) sidebar_hidden: bool,
    /// The person hid the inspector from the View menu.
    pub(super) inspector_hidden: bool,
    pub(super) has_photos: bool,
    /// The smallest width a tile may take: the grid's zoom.
    pub(super) min_tile_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) columns: usize,
    pub(super) tile_width: Pixels,
    pub(super) tile_height: Pixels,
    pub(super) show_sidebar: bool,
    /// The width the sidebar takes, or zero when it is hidden.
    pub(super) sidebar_width: f32,
    pub(super) show_inspector: bool,
}

impl Layout {
    pub(super) fn compute(input: LayoutInput) -> Self {
        let show_sidebar = !input.benchmarking && !input.sidebar_hidden;
        let sidebar = SIDEBAR_WIDTH;
        let show_inspector = input.benchmarking
            || (!input.inspector_hidden
                && input.has_photos
                && input.viewport_width >= MIN_WIDTH_FOR_INSPECTOR);
        let min_tile = input.min_tile_width;
        let inspector = if show_inspector { INSPECTOR_WIDTH } else { 0.0 };
        let taken = if show_sidebar { sidebar } else { 0.0 } + inspector;
        let usable = (input.viewport_width - taken - GRID_GUTTER * 2.0).max(min_tile);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = if input.benchmarking {
            benchmark::COLUMNS
        } else {
            (((usable + GRID_GAP) / (min_tile + GRID_GAP)).floor() as usize).max(1)
        };
        #[allow(clippy::cast_precision_loss)]
        let tile_width = (usable - GRID_GAP * (columns - 1) as f32) / columns as f32;
        let tile_height = (tile_width * TILE_ASPECT).floor();
        Self {
            columns,
            tile_width: px(tile_width.floor()),
            tile_height: px(tile_height),
            show_sidebar,
            sidebar_width: if show_sidebar { sidebar } else { 0.0 },
            show_inspector,
        }
    }

    /// The height of one grid row, including the gap above it. The grid's
    /// `uniform_list` and the page-up/down math both use this value.
    pub(super) fn row_height(&self) -> f32 {
        self.tile_height.as_f32() + GRID_GAP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(viewport_width: f32) -> LayoutInput {
        LayoutInput {
            viewport_width,
            benchmarking: false,
            sidebar_hidden: false,
            inspector_hidden: false,
            has_photos: true,
            min_tile_width: DEFAULT_TILE_WIDTH,
        }
    }

    #[test]
    fn a_narrow_window_still_has_one_column() {
        let layout = Layout::compute(input(200.0));
        assert_eq!(layout.columns, 1);
        assert!(layout.tile_width.as_f32() >= DEFAULT_TILE_WIDTH);
    }

    #[test]
    fn the_inspector_appears_only_when_it_fits_and_has_something_to_show() {
        assert!(!Layout::compute(input(MIN_WIDTH_FOR_INSPECTOR - 1.0)).show_inspector);
        assert!(Layout::compute(input(MIN_WIDTH_FOR_INSPECTOR)).show_inspector);
        let empty = LayoutInput {
            has_photos: false,
            ..input(1400.0)
        };
        assert!(!Layout::compute(empty).show_inspector);
    }

    #[test]
    fn tiles_never_overflow_the_width_left_for_the_grid() {
        for width in [560.0, 900.0, 1200.0, 1728.0, 2560.0] {
            let layout = Layout::compute(input(width));
            let inspector = if layout.show_inspector {
                INSPECTOR_WIDTH
            } else {
                0.0
            };
            let grid = width - SIDEBAR_WIDTH - inspector - GRID_GUTTER * 2.0;
            #[allow(clippy::cast_precision_loss)]
            let used = layout.tile_width.as_f32() * layout.columns as f32
                + GRID_GAP * (layout.columns - 1) as f32;
            assert!(
                used <= grid.max(DEFAULT_TILE_WIDTH),
                "{width}: {used} > {grid}"
            );
            assert!(layout.tile_width.as_f32() >= DEFAULT_TILE_WIDTH, "{width}");
        }
    }

    #[test]
    fn hiding_the_sidebar_gives_the_grid_more_room() {
        let open = Layout::compute(input(1200.0));
        let hidden = Layout::compute(LayoutInput {
            sidebar_hidden: true,
            ..input(1200.0)
        });
        assert!(!hidden.show_sidebar);
        assert_eq!(hidden.sidebar_width, 0.0);
        assert!(
            hidden.columns > open.columns || hidden.tile_width >= open.tile_width,
            "{hidden:?} vs {open:?}"
        );
    }

    #[test]
    fn hiding_the_inspector_gives_its_width_to_the_grid() {
        let shown = Layout::compute(input(1400.0));
        let hidden = Layout::compute(LayoutInput {
            inspector_hidden: true,
            ..input(1400.0)
        });
        assert!(shown.show_inspector);
        assert!(!hidden.show_inspector);
        assert!(hidden.columns > shown.columns || hidden.tile_width > shown.tile_width);
    }

    #[test]
    fn zooming_in_makes_fewer_larger_tiles() {
        let small = Layout::compute(LayoutInput {
            min_tile_width: *TILE_WIDTH_RANGE.start(),
            ..input(1400.0)
        });
        let large = Layout::compute(LayoutInput {
            min_tile_width: *TILE_WIDTH_RANGE.end(),
            ..input(1400.0)
        });
        assert!(large.columns < small.columns);
        assert!(large.tile_width.as_f32() >= *TILE_WIDTH_RANGE.end());
    }

    #[test]
    fn the_benchmark_uses_its_fixed_geometry() {
        let layout = Layout::compute(LayoutInput {
            benchmarking: true,
            ..input(1200.0)
        });
        assert_eq!(layout.columns, benchmark::COLUMNS);
        assert!(!layout.show_sidebar);
        assert!(layout.show_inspector);
    }
}
