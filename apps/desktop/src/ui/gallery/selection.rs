//! Which photos the grid shows and which of them are selected. The sidebar's destination
//! filters the library; the selection is a set with a focused photo that the viewer, the
//! inspector and the description follow. Grid positions count the photos the grid shows.

use super::{Gallery, GalleryPhoto, GridLayout, Overlay, row_key};
use crate::{
    PhotoId,
    cloud::CloudState,
    grid::{self, Movement},
};
use rune::{Mods, Tx};

/// The sidebar's destinations: the whole library, or one collection the photo API keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum Destination {
    #[default]
    All,
    Drafts,
    Published,
}

impl Destination {
    /// Its name in the sidebar and the title bar.
    pub(in crate::ui) fn title(self) -> &'static str {
        match self {
            Self::All => "All photos",
            Self::Drafts => "Drafts",
            Self::Published => "Published",
        }
    }

    /// Whether a photo in `state` belongs here.
    pub(in crate::ui) fn holds(self, state: &CloudState) -> bool {
        match self {
            Self::All => true,
            Self::Drafts => state.is_draft(),
            Self::Published => state.is_published(),
        }
    }
}

impl Gallery {
    /// How many photos the grid shows.
    pub(in crate::ui) fn shown_len(&self) -> usize {
        match self.destination {
            Destination::All => self.photos.len(),
            _ => self.filtered.len(),
        }
    }

    /// The library index of the photo at `position` in the grid.
    pub(super) fn shown_index(&self, position: usize) -> Option<usize> {
        match self.destination {
            Destination::All => (position < self.photos.len()).then_some(position),
            _ => self.filtered.get(position).copied(),
        }
    }

    /// The photo at `position` in the grid.
    pub(in crate::ui) fn shown(&self, position: usize) -> Option<&GalleryPhoto> {
        self.photos.get(self.shown_index(position)?)
    }

    /// The photos the grid shows, in grid order.
    pub(in crate::ui) fn shown_photos(&self) -> impl Iterator<Item = &GalleryPhoto> {
        (0..self.shown_len()).filter_map(|position| self.shown(position))
    }

    fn shown_ids(&self) -> Vec<PhotoId> {
        self.shown_photos()
            .map(|photo| photo.record.id.clone())
            .collect()
    }

    fn shown_position(&self, id: &PhotoId) -> Option<usize> {
        self.shown_photos().position(|photo| &photo.record.id == id)
    }

    /// Reads again which photos the destination holds, and keeps the selection to them. Runs
    /// after photos arrive or leave and after a cloud state changes, so the grid, the
    /// sidebar's counts and the selection agree.
    pub(super) fn refilter(&mut self) {
        self.filter();
        self.drop_hidden();
    }

    fn filter(&mut self) {
        let destination = self.destination;
        self.filtered = match destination {
            Destination::All => Vec::new(),
            _ => self
                .photos
                .iter()
                .enumerate()
                .filter(|(_, photo)| destination.holds(&photo.record.cloud))
                .map(|(index, _)| index)
                .collect(),
        };
    }

    /// Shows a sidebar destination's photos in the grid. The focused photo stays selected, on
    /// its own, when the destination holds it; else the destination's first photo is.
    pub(in crate::ui) fn open_destination(&mut self, destination: Destination) {
        if self.destination == destination {
            return;
        }
        self.destination = destination;
        self.filter();
        let before = self.focus().cloned();
        let kept = before
            .clone()
            .filter(|id| self.shown_position(id).is_some());
        match kept {
            Some(id) => self.selection.select(id),
            None => self.select_first_shown(),
        }
        self.focus_changed(before, None);
    }

    /// Takes the photos the destination no longer holds, such as a draft that was just
    /// published while Drafts is open, out of the selection, so no action runs on a photo the
    /// grid does not show. When the focus leaves, the destination's first photo is selected
    /// alone, as on opening it, and the viewer that showed the focus closes.
    fn drop_hidden(&mut self) {
        let destination = self.destination;
        if destination == Destination::All || self.selection.is_empty() {
            return;
        }
        let hidden: Vec<PhotoId> = self
            .photos
            .iter()
            .filter(|photo| {
                self.selection.contains(&photo.record.id) && !destination.holds(&photo.record.cloud)
            })
            .map(|photo| photo.record.id.clone())
            .collect();
        let before = self.focus().cloned();
        if before.as_ref().is_some_and(|focus| hidden.contains(focus)) {
            if self.overlay == Overlay::Viewer {
                self.overlay = Overlay::None;
            }
            self.select_first_shown();
            self.focus_changed(before, None);
            return;
        }
        for id in &hidden {
            self.selection.remove(id);
        }
    }

    /// The grid's first photo alone, or nothing when the grid is empty.
    fn select_first_shown(&mut self) {
        match self.shown(0).map(|photo| photo.record.id.clone()) {
            Some(id) => self.selection.select(id),
            None => self.selection.clear(),
        }
    }

    /// Photos in each destination the photo API keeps: drafts, then published photos.
    pub(in crate::ui) fn collection_counts(&self) -> (usize, usize) {
        self.photos
            .iter()
            .fold((0, 0), |(drafts, published), photo| {
                let state = &photo.record.cloud;
                (
                    drafts + usize::from(state.is_draft()),
                    published + usize::from(state.is_published()),
                )
            })
    }

    pub(in crate::ui) fn focus(&self) -> Option<&PhotoId> {
        self.selection.focus()
    }

    /// The focused photo's place in the grid, when the grid shows it.
    pub(in crate::ui) fn focused_position(&self) -> Option<usize> {
        self.shown_position(self.focus()?)
    }

    /// The photo the viewer, the inspector and the description show.
    pub(in crate::ui) fn focused_photo(&self) -> Option<&GalleryPhoto> {
        let focus = self.focus()?;
        self.photos.iter().find(|photo| &photo.record.id == focus)
    }

    /// The selected photos, in grid order. One selected photo is the focused one, found
    /// without a pass over the selection, as `poll` asks for it before every frame.
    pub(in crate::ui) fn selected_photos(&self) -> impl Iterator<Item = &GalleryPhoto> {
        let several = self.selection.len() > 1;
        let one = if several { None } else { self.focused_photo() };
        let many = several.then_some(&self.photos).into_iter().flatten();
        one.into_iter()
            .chain(many.filter(|photo| self.selection.contains(&photo.record.id)))
    }

    /// The selected photos' identities, in grid order.
    pub(super) fn selected_ids(&self) -> Vec<PhotoId> {
        self.selected_photos()
            .map(|photo| photo.record.id.clone())
            .collect()
    }

    /// Selects the photo at grid `position` alone.
    pub(in crate::ui) fn select_position(&mut self, position: usize, tx: &mut Tx) {
        if let Some(id) = self.shown(position).map(|photo| photo.record.id.clone()) {
            let before = self.focus().cloned();
            self.selection.select(id);
            self.focus_changed(before, Some(tx));
        }
    }

    /// A click on a tile: Cmd-click adds or removes the photo, Shift-click selects the range
    /// from the anchor, and a plain click selects the photo alone.
    pub(in crate::ui) fn click(&mut self, id: &PhotoId, mods: Mods, tx: &mut Tx) {
        let before = self.focus().cloned();
        if mods.cmd {
            self.selection.toggle(id.clone(), &self.shown_ids());
        } else if mods.shift {
            self.selection.extend(id.clone(), &self.shown_ids());
        } else {
            self.selection.select(id.clone());
        }
        self.focus_changed(before, Some(tx));
    }

    /// Makes sure the menu a right-click opens acts on the photo under the pointer: a photo
    /// outside the selection is selected alone first, and one inside keeps the selection.
    pub(in crate::ui) fn select_for_menu(&mut self, id: &PhotoId, tx: &mut Tx) {
        if !self.selection.contains(id) {
            self.click(id, Mods::default(), tx);
        }
    }

    /// An arrow, Page or Home/End key: moves the focus. With Shift the selection runs from the
    /// anchor to the new focus; without, it collapses to the new focus alone.
    pub(in crate::ui) fn move_selection(&mut self, movement: Movement, extend: bool, tx: &mut Tx) {
        let GridLayout {
            columns, page_rows, ..
        } = self.layout();
        let Some(position) = grid::move_selection(
            self.focused_position(),
            self.shown_len(),
            columns,
            page_rows,
            movement,
        ) else {
            return;
        };
        let Some(id) = self.shown(position).map(|photo| photo.record.id.clone()) else {
            return;
        };
        let before = self.focus().cloned();
        if extend {
            self.selection.extend(id, &self.shown_ids());
        } else {
            self.selection.select(id);
        }
        self.focus_changed(before, Some(tx));
    }

    /// Cmd-A: every photo the grid shows.
    pub(in crate::ui) fn select_all(&mut self) {
        let before = self.focus().cloned();
        self.selection.select_all(&self.shown_ids());
        self.focus_changed(before, None);
    }

    /// Forgets a photo that left the library. The viewer closes when it showed it.
    pub(super) fn deselect(&mut self, id: &PhotoId) {
        let focused = self.focus() == Some(id);
        self.selection.remove(id);
        if focused {
            self.preview = None;
            if self.overlay == Overlay::Viewer {
                self.overlay = Overlay::None;
            }
        }
    }

    /// After the focus moves: what was written for the photo left behind is kept, the new
    /// photo's large rendition is asked for, and the grid scrolls to it.
    fn focus_changed(&mut self, before: Option<PhotoId>, tx: Option<&mut Tx>) {
        if self.focus() == before.as_ref() {
            return;
        }
        self.save_description();
        if self.focus().is_none() {
            self.preview = None;
            return;
        }
        self.request_preview();
        if let (Some(tx), Some(position)) = (tx, self.focused_position()) {
            tx.scroll_into_view(row_key(self, position, self.layout().columns));
        }
    }
}
