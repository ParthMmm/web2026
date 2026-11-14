//! Which items of a grid are selected, in model terms: a set, the focused item the viewer and
//! the inspector follow, and the anchor a Shift-click or Shift-arrow extends a range from.
//! Ranges run in the grid's order, which each call that needs it passes in. The focus is
//! always one of the selected items, and there is a focus whenever anything is selected.

use std::{collections::HashSet, hash::Hash};

#[derive(Clone, Debug)]
pub struct Selection<T> {
    selected: HashSet<T>,
    focus: Option<T>,
    anchor: Option<T>,
}

impl<T> Default for Selection<T> {
    fn default() -> Self {
        Self {
            selected: HashSet::new(),
            focus: None,
            anchor: None,
        }
    }
}

impl<T: Clone + Eq + Hash> Selection<T> {
    /// The item the viewer and the inspector show.
    pub fn focus(&self) -> Option<&T> {
        self.focus.as_ref()
    }

    pub fn contains(&self, item: &T) -> bool {
        self.selected.contains(item)
    }

    pub fn len(&self) -> usize {
        self.selected.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// A click or a plain arrow key: `item` alone, focused, and the anchor of the next range.
    pub fn select(&mut self, item: T) {
        self.selected.clear();
        self.selected.insert(item.clone());
        self.focus = Some(item.clone());
        self.anchor = Some(item);
    }

    /// A Cmd-click: adds `item`, or removes it when it is selected. An added item takes the
    /// focus and the anchor. A removed focus passes to the nearest selected item after it in
    /// `order`, else before it, else to nothing.
    pub fn toggle(&mut self, item: T, order: &[T]) {
        if !self.selected.remove(&item) {
            self.selected.insert(item.clone());
            self.focus = Some(item.clone());
            self.anchor = Some(item);
            return;
        }
        if self.focus.as_ref() != Some(&item) {
            return;
        }
        let at = order.iter().position(|other| *other == item);
        let after = at.map_or(0, |at| at + 1);
        let before = at.unwrap_or(order.len());
        let next = order[after.min(order.len())..]
            .iter()
            .chain(order[..before].iter().rev())
            .find(|other| self.selected.contains(*other))
            .cloned();
        self.focus.clone_from(&next);
        self.anchor = next;
    }

    /// A Shift-click or Shift-arrow: the items from the anchor to `item` in `order`, with
    /// `item` focused and the anchor kept. With no anchor in `order`, `item` alone.
    pub fn extend(&mut self, item: T, order: &[T]) {
        let from = self
            .anchor
            .as_ref()
            .and_then(|anchor| order.iter().position(|other| other == anchor));
        let to = order.iter().position(|other| *other == item);
        let (Some(from), Some(to)) = (from, to) else {
            self.select(item);
            return;
        };
        let range = from.min(to)..=from.max(to);
        self.selected = order[range].iter().cloned().collect();
        self.focus = Some(item);
    }

    /// Cmd-A: every item in `order`. The focus stays when it is among them, else the first
    /// takes it; the anchor goes with the focus.
    pub fn select_all(&mut self, order: &[T]) {
        self.selected = order.iter().cloned().collect();
        let keep = self
            .focus
            .as_ref()
            .is_some_and(|focus| self.selected.contains(focus));
        if !keep {
            self.focus = order.first().cloned();
        }
        self.anchor.clone_from(&self.focus);
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.focus = None;
        self.anchor = None;
    }

    /// Forgets `item`, as when it leaves the library. Losing the focus clears the selection.
    pub fn remove(&mut self, item: &T) {
        if self.focus.as_ref() == Some(item) {
            self.clear();
            return;
        }
        self.selected.remove(item);
        if self.anchor.as_ref() == Some(item) {
            self.anchor.clone_from(&self.focus);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Selection;

    const ORDER: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

    fn selected(selection: &Selection<u8>) -> Vec<u8> {
        ORDER
            .into_iter()
            .filter(|item| selection.contains(item))
            .collect()
    }

    fn after_click(item: u8) -> Selection<u8> {
        let mut selection = Selection::default();
        selection.select(item);
        selection
    }

    #[test]
    fn a_click_selects_one_item_and_focuses_it() {
        let mut selection = after_click(2);
        selection.toggle(5, &ORDER);
        selection.select(3);
        assert_eq!(selected(&selection), [3]);
        assert_eq!(selection.focus(), Some(&3));
    }

    #[test]
    fn a_cmd_click_adds_an_item_and_a_second_removes_it() {
        let mut selection = after_click(2);
        selection.toggle(5, &ORDER);
        assert_eq!(selected(&selection), [2, 5]);
        assert_eq!(
            selection.focus(),
            Some(&5),
            "the added item takes the focus"
        );
        selection.toggle(2, &ORDER);
        assert_eq!(selected(&selection), [5]);
        assert_eq!(
            selection.focus(),
            Some(&5),
            "removing another item keeps the focus"
        );
    }

    #[test]
    fn a_cmd_click_on_the_focus_passes_the_focus_to_a_neighbour() {
        let mut selection = after_click(2);
        selection.toggle(6, &ORDER);
        selection.toggle(4, &ORDER);
        selection.toggle(4, &ORDER);
        assert_eq!(
            selection.focus(),
            Some(&6),
            "the next selected item after it"
        );
        selection.toggle(6, &ORDER);
        assert_eq!(selection.focus(), Some(&2), "else the one before it");
        selection.toggle(2, &ORDER);
        assert!(selection.is_empty());
        assert_eq!(selection.focus(), None);
    }

    #[test]
    fn a_shift_click_selects_the_range_from_the_anchor() {
        let mut selection = after_click(2);
        selection.extend(5, &ORDER);
        assert_eq!(selected(&selection), [2, 3, 4, 5]);
        assert_eq!(selection.focus(), Some(&5));
        selection.extend(0, &ORDER);
        assert_eq!(
            selected(&selection),
            [0, 1, 2],
            "a second range replaces the first and keeps its anchor"
        );
    }

    #[test]
    fn a_cmd_click_moves_the_anchor_for_the_next_range() {
        let mut selection = after_click(1);
        selection.toggle(5, &ORDER);
        selection.extend(7, &ORDER);
        assert_eq!(selected(&selection), [5, 6, 7]);
    }

    #[test]
    fn a_shift_click_without_an_anchor_selects_one_item() {
        let mut selection = Selection::default();
        selection.extend(4, &ORDER);
        assert_eq!(selected(&selection), [4]);
        assert_eq!(selection.focus(), Some(&4));
    }

    #[test]
    fn shift_arrows_extend_from_the_anchor_and_shrink_back() {
        // Shift-Right twice, then Shift-Left once, from item 3: each arrow names the item
        // next to the focus.
        let mut selection = after_click(3);
        selection.extend(4, &ORDER);
        selection.extend(5, &ORDER);
        assert_eq!(selected(&selection), [3, 4, 5]);
        selection.extend(4, &ORDER);
        assert_eq!(selected(&selection), [3, 4]);
        assert_eq!(selection.focus(), Some(&4));
    }

    #[test]
    fn cmd_a_selects_every_item_and_keeps_the_focus() {
        let mut selection = after_click(3);
        selection.select_all(&ORDER);
        assert_eq!(selected(&selection), ORDER);
        assert_eq!(selection.focus(), Some(&3));
        let mut empty = Selection::default();
        empty.select_all(&ORDER);
        assert_eq!(
            empty.focus(),
            Some(&0),
            "with no focus the first item takes it"
        );
    }

    #[test]
    fn a_plain_arrow_collapses_the_selection_to_one_item() {
        let mut selection = after_click(3);
        selection.select_all(&ORDER);
        // A plain arrow is a select of the item next to the focus.
        selection.select(4);
        assert_eq!(selected(&selection), [4]);
        assert_eq!(selection.len(), 1);
    }

    #[test]
    fn a_removed_item_leaves_the_selection() {
        let mut selection = after_click(3);
        selection.toggle(5, &ORDER);
        selection.remove(&3);
        assert_eq!(selected(&selection), [5]);
        assert_eq!(selection.focus(), Some(&5));
        selection.remove(&5);
        assert!(
            selection.is_empty(),
            "losing the focus clears the selection"
        );
        assert_eq!(selection.focus(), None);
    }
}
