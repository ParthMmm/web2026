//! The selected photo's description: the words a visitor's screen reader says for it, sent as
//! alt text when the photo is published. The inspector edits a copy; Save (or ⌘↩) keeps it, and
//! so does moving to another photo. A published photo is published again with the new words.

use super::{Gallery, GalleryPhoto, LibraryState};
use crate::{DESCRIPTION_LIMIT, PhotoId, clean_description, single_line};
use rune::TextEdit;

/// The description being edited, and the photo it belongs to.
#[derive(Default)]
pub(in crate::ui) struct DescriptionEdit {
    pub(in crate::ui) id: Option<PhotoId>,
    pub(in crate::ui) edit: TextEdit,
}

impl Gallery {
    /// The field's text for `photo`: the edit when it is this photo's, else what is kept.
    pub(in crate::ui) fn description_edit(&self, photo: &GalleryPhoto) -> TextEdit {
        if self.description.id.as_ref() == Some(&photo.record.id) {
            self.description.edit.clone()
        } else {
            TextEdit::new(photo.record.description.clone().unwrap_or_default())
        }
    }

    /// The edit the field changes, started from the selected photo's kept words.
    pub(in crate::ui) fn description_mut(&mut self) -> &mut TextEdit {
        let selected = self.selected_photo().map(|photo| {
            (
                photo.record.id.clone(),
                photo.record.description.clone().unwrap_or_default(),
            )
        });
        let id = selected.as_ref().map(|(id, _)| id.clone());
        if self.description.id != id {
            let kept = selected.map(|(_, kept)| kept).unwrap_or_default();
            self.description = DescriptionEdit {
                id,
                edit: TextEdit::new(kept),
            };
        }
        &mut self.description.edit
    }

    /// Whether the field holds words for `photo` that differ from what is kept.
    pub(in crate::ui) fn description_changed(&self, photo: &GalleryPhoto) -> bool {
        self.description.id.as_ref() == Some(&photo.record.id)
            && clean_description(&self.description.edit.text) != photo.record.description
    }

    /// Keeps the edited words for their photo, when they changed and fit. The library writes
    /// them to the catalog and publishes a published photo again; the demo library keeps them
    /// in memory.
    pub(in crate::ui) fn save_description(&mut self) {
        let Some(id) = self.description.id.clone() else {
            return;
        };
        let text = self.description.edit.text.clone();
        if description_length(&text) > DESCRIPTION_LIMIT {
            return;
        }
        let kept = clean_description(&text);
        let Some(photo) = self.photos.iter_mut().find(|photo| photo.record.id == id) else {
            return;
        };
        if photo.record.description == kept {
            return;
        }
        photo.record.description = kept;
        if let LibraryState::Open(library) = &self.library {
            library.describe(id, text);
        }
    }
}

/// How long the words are once kept, in characters, as the API counts them.
pub(in crate::ui) fn description_length(text: &str) -> usize {
    single_line(text).chars().count()
}
