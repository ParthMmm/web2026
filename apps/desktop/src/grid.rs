//! Keyboard movement through a row-major photo grid, in model coordinates.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// Returns the item selected after `movement`, or `None` for an empty grid.
///
/// With no current selection, any movement selects the first item. Left and
/// right continue across row boundaries; up and down keep the column and stop
/// at the first or last row, landing on the last item of a short final row.
pub fn move_selection(
    current: Option<usize>,
    len: usize,
    columns: usize,
    page_rows: usize,
    movement: Movement,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let Some(current) = current.map(|ix| ix.min(last)) else {
        return Some(if movement == Movement::Last { last } else { 0 });
    };
    let columns = columns.max(1);
    let page = columns * page_rows.max(1);
    let next = match movement {
        Movement::Left => current.saturating_sub(1),
        Movement::Right => (current + 1).min(last),
        Movement::Up if current >= columns => current - columns,
        Movement::Up => current,
        Movement::Down if current / columns < last / columns => (current + columns).min(last),
        Movement::Down => current,
        Movement::PageUp => current.checked_sub(page).unwrap_or(current % columns),
        Movement::PageDown => (current + page).min(last),
        Movement::First => 0,
        Movement::Last => last,
    };
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::{Movement::*, move_selection};

    #[test]
    fn an_empty_grid_has_no_selection() {
        assert_eq!(move_selection(None, 0, 4, 3, Down), None);
        assert_eq!(move_selection(Some(3), 0, 4, 3, Down), None);
    }

    #[test]
    fn the_first_movement_selects_an_end() {
        assert_eq!(move_selection(None, 10, 4, 3, Right), Some(0));
        assert_eq!(move_selection(None, 10, 4, 3, Down), Some(0));
        assert_eq!(move_selection(None, 10, 4, 3, Last), Some(9));
    }

    #[test]
    fn horizontal_movement_wraps_rows_and_stops_at_ends() {
        assert_eq!(move_selection(Some(3), 10, 4, 3, Right), Some(4));
        assert_eq!(move_selection(Some(4), 10, 4, 3, Left), Some(3));
        assert_eq!(move_selection(Some(0), 10, 4, 3, Left), Some(0));
        assert_eq!(move_selection(Some(9), 10, 4, 3, Right), Some(9));
    }

    #[test]
    fn vertical_movement_keeps_the_column_and_clamps_a_short_last_row() {
        assert_eq!(move_selection(Some(1), 10, 4, 3, Down), Some(5));
        assert_eq!(move_selection(Some(7), 10, 4, 3, Down), Some(9));
        assert_eq!(move_selection(Some(9), 10, 4, 3, Down), Some(9));
        assert_eq!(move_selection(Some(5), 10, 4, 3, Up), Some(1));
        assert_eq!(move_selection(Some(2), 10, 4, 3, Up), Some(2));
    }

    #[test]
    fn paging_moves_whole_rows() {
        assert_eq!(move_selection(Some(1), 100, 4, 3, PageDown), Some(13));
        assert_eq!(move_selection(Some(97), 100, 4, 3, PageDown), Some(99));
        assert_eq!(move_selection(Some(13), 100, 4, 3, PageUp), Some(1));
        assert_eq!(move_selection(Some(6), 100, 4, 3, PageUp), Some(2));
    }

    #[test]
    fn a_stale_selection_is_clamped_after_the_grid_shrinks() {
        assert_eq!(move_selection(Some(50), 10, 4, 3, Left), Some(8));
    }
}
