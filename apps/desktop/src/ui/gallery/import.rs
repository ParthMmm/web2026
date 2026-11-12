//! Bringing photos in: the Import… panel, the Photos picker, and retrying or clearing the
//! files that didn't import.

use super::{Gallery, LibraryState};
use crate::{
    photos,
    ui::{format, open_panel},
};
use std::path::PathBuf;

/// An import asked for before the library finished opening.
pub(super) struct PendingImport {
    pub(super) inputs: Vec<PathBuf>,
    /// The files are copies the app made (from Photos), so they are deleted
    /// once imported or if the import can't run.
    pub(super) owned: bool,
}

impl Gallery {
    pub(super) fn import(&mut self, inputs: Vec<PathBuf>, owned: bool) {
        match &self.library {
            LibraryState::Open(library) => {
                if owned {
                    library.import_copies(inputs);
                } else {
                    library.import(inputs);
                }
                self.progress.get_or_insert((0, 0));
                self.status = "Importing…".into();
            }
            LibraryState::Opening => self.pending_imports.push(PendingImport { inputs, owned }),
            LibraryState::Unavailable(_) | LibraryState::Demo => {
                if owned {
                    photos::cleanup_import(&inputs);
                }
            }
        }
    }

    pub(in crate::ui) fn choose_files(&mut self) {
        let Some(handle) = self.handle.clone().filter(|_| !self.picking) else {
            return;
        };
        let receiver = match open_panel::open() {
            Ok(receiver) => receiver,
            Err(error) => {
                self.status = format!("Couldn't choose files: {error:#}");
                return;
            }
        };
        self.picking = true;
        std::thread::spawn(move || {
            let paths = receiver.recv().unwrap_or_default();
            handle.update(move |gallery: &mut Gallery, _| {
                gallery.picking = false;
                if !paths.is_empty() {
                    gallery.import(paths, false);
                }
            });
        });
    }

    pub(in crate::ui) fn choose_from_photos(&mut self) {
        let Some(handle) = self.handle.clone().filter(|_| !self.picking) else {
            return;
        };
        let receiver = match photos::open(photos::import_root(&self.library_root)) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.status = format!("Couldn't open Photos: {error:#}");
                return;
            }
        };
        self.picking = true;
        std::thread::spawn(move || {
            let result = receiver.recv();
            handle.update(move |gallery: &mut Gallery, _| {
                gallery.picking = false;
                match result {
                    Ok(Ok(picked)) => {
                        let note = picked
                            .failures
                            .first()
                            .map(|reason| format::photos_not_loaded(picked.failures.len(), reason));
                        if picked.copies.is_empty() {
                            gallery.status = note.unwrap_or_default();
                        } else {
                            gallery.photos_note = note;
                            gallery.import(picked.copies, true);
                        }
                    }
                    Ok(Err(error)) => {
                        gallery.status = format!("Couldn't import from Photos: {error}");
                    }
                    // The picker closed without delivering a result.
                    Err(_) => {}
                }
            });
        });
    }

    pub(in crate::ui) fn retry_failures(&mut self) {
        let paths = self
            .failures
            .iter()
            .map(|failure| failure.path.clone())
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            self.import(paths, false);
        }
        self.close_overlay();
    }

    pub(in crate::ui) fn clear_failures(&mut self) {
        if let LibraryState::Open(library) = &self.library {
            library.clear_failures();
        }
        self.failures.clear();
        self.close_overlay();
    }
}
