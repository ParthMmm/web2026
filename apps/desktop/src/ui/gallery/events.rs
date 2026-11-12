//! What the library reports, applied to the gallery: the library opening, imports landing
//! one photo at a time, renditions finishing, and photos leaving.

use super::{Gallery, GalleryPhoto, LibraryState, Overlay, Thumbnail};
use crate::{
    DerivativeSize, Event, ImportFailure, ImportOutcome, Library, LibraryOptions, PhotoId,
    PhotoRecord, photos, ui::format,
};
use rune::Tx;
use std::path::PathBuf;

const EVENTS_PER_TICK: usize = 32;

#[derive(Default)]
pub(super) struct Drained {
    pub(super) received: bool,
    /// Work is still running, so the next poll should come soon.
    pub(super) outstanding: bool,
}

impl Gallery {
    pub(super) fn library_opened(
        &mut self,
        opened: anyhow::Result<(Library, Vec<GalleryPhoto>)>,
        tx: &mut Tx,
    ) {
        // The library lands just after launch: show it in place, not animated in.
        tx.instant();
        match opened {
            Ok((library, photos)) => {
                self.failures = library.failures().to_vec();
                self.photos = photos;
                self.library = LibraryState::Open(library);
                // A photo is selected on open, so the inspector carries content
                // and the first arrow key moves somewhere.
                if self.selected.is_none() {
                    self.select_index(0, tx);
                }
                // The count lives on the sidebar and the weight in its footer, so
                // the status line is free for transient state.
                self.status = String::new();
                for pending in std::mem::take(&mut self.pending_imports) {
                    self.import(pending.inputs, pending.owned);
                }
            }
            Err(error) => {
                let message = format!("{error:#}");
                self.status = format!("Couldn't open the library: {message}");
                self.library = LibraryState::Unavailable(message);
                for pending in std::mem::take(&mut self.pending_imports) {
                    if pending.owned {
                        photos::cleanup_import(&pending.inputs);
                    }
                }
            }
        }
        self.request_visible();
    }

    /// Applies queued library events.
    pub(super) fn drain(&mut self, tx: &mut Tx) -> Drained {
        let LibraryState::Open(library) = &self.library else {
            return Drained::default();
        };
        let events: Vec<Event> = (0..EVENTS_PER_TICK)
            .map_while(|_| library.try_recv().ok())
            .collect();
        let received = !events.is_empty();
        for event in events {
            self.apply(event, tx);
        }
        if received {
            self.request_visible();
        }
        let outstanding = received
            || self.benchmark.is_some()
            || self.progress.is_some()
            || self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.image.is_none() && preview.error.is_none());
        Drained {
            received,
            outstanding,
        }
    }

    fn apply(&mut self, event: Event, tx: &mut Tx) {
        match event {
            Event::ImportProgress { processed, total } => {
                self.progress = Some((processed, total));
            }
            Event::Imported {
                source,
                outcome,
                processed,
                total,
                ..
            } => {
                self.progress = Some((processed, total));
                self.apply_outcome(source, outcome, tx);
            }
            Event::ImportFinished { summary } => {
                self.progress = None;
                self.status = format::import_summary(&summary);
                if let Some(note) = self.photos_note.take() {
                    self.status = format!("{} · {note}", self.status);
                }
            }
            Event::Removed { id } => self.remove(&id),
            Event::Ready {
                id,
                size,
                path,
                cache_hit,
                ..
            } => match size {
                DerivativeSize::Grid => {
                    if let Some(photo) = self.photo_mut(&id) {
                        photo.thumbnail = Thumbnail::Ready(path.into());
                    }
                }
                DerivativeSize::Desktop => self.preview_ready(&id, path, cache_hit),
                DerivativeSize::Mobile => {}
            },
            Event::DerivativeFailed { id, size, message } => match size {
                DerivativeSize::Grid => {
                    if let Some(photo) = self.photo_mut(&id) {
                        photo.thumbnail = Thumbnail::Failed;
                    }
                }
                DerivativeSize::Desktop => self.preview_failed(&id, message),
                DerivativeSize::Mobile => {}
            },
            Event::MetadataUpdated { photo: record } | Event::Detached { photo: record } => {
                if let Some(photo) = self.photo_mut(&record.id) {
                    photo.record = record;
                }
            }
            Event::CloudChanged { id, state } => {
                if let Some(photo) = self.photo_mut(&id) {
                    photo.record.cloud = state;
                }
            }
            Event::FilmMetadataFinished => {}
            Event::FailuresCleared => self.failures.clear(),
        }
    }

    fn apply_outcome(&mut self, source: PathBuf, outcome: ImportOutcome, tx: &mut Tx) {
        match outcome {
            ImportOutcome::Added(record) => {
                let LibraryState::Open(library) = &self.library else {
                    return;
                };
                let grid = library.derivative_path(&record.id, DerivativeSize::Grid);
                self.upsert(record, Thumbnail::Ready(grid.into()), tx);
            }
            ImportOutcome::Duplicate(record) | ImportOutcome::Relinked(record) => {
                self.upsert(record, Thumbnail::Pending, tx);
            }
            ImportOutcome::Unchanged(_) => {}
            ImportOutcome::Failed(message) => {
                self.session_failures += 1;
                self.failures.retain(|failure| failure.path != source);
                let index = self
                    .failures
                    .partition_point(|failure| failure.path < source);
                self.failures.insert(
                    index,
                    ImportFailure {
                        path: source,
                        message,
                        failed_at: crate::catalog::unix_now(),
                    },
                );
                return;
            }
        }
        self.failures.retain(|failure| failure.path != source);
    }

    /// Inserts or updates a photo, keeping the grid ordered by original path.
    fn upsert(&mut self, record: PhotoRecord, thumbnail: Thumbnail, tx: &mut Tx) {
        let existing = self
            .photos
            .iter()
            .position(|photo| photo.record.id == record.id)
            .map(|index| self.photos.remove(index));
        let photo = match existing {
            Some(mut photo) => {
                photo.record = record;
                photo
            }
            None => GalleryPhoto::new(record, thumbnail),
        };
        let index = self
            .photos
            .partition_point(|other| other.record.source < photo.record.source);
        self.photos.insert(index, photo);
        // A library that opens while its first import runs has nothing to
        // select at open time. The first photo to arrive repairs that.
        if self.selected.is_none() {
            self.select_index(0, tx);
        }
    }

    fn remove(&mut self, id: &PhotoId) {
        self.photos.retain(|photo| &photo.record.id != id);
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
            self.preview = None;
            if self.overlay == Overlay::Viewer {
                self.overlay = Overlay::None;
            }
        }
    }

    fn photo_mut(&mut self, id: &PhotoId) -> Option<&mut GalleryPhoto> {
        self.photos.iter_mut().find(|photo| &photo.record.id == id)
    }
}

pub(super) fn open_library(
    options: LibraryOptions,
) -> anyhow::Result<(Library, Vec<GalleryPhoto>)> {
    let library = Library::open(options)?;
    let photos = library
        .photos()
        .iter()
        .map(|record| {
            let grid = library.derivative_path(&record.id, DerivativeSize::Grid);
            let thumbnail = if grid.is_file() {
                Thumbnail::Ready(grid.into())
            } else {
                Thumbnail::Pending
            };
            GalleryPhoto::new(record.clone(), thumbnail)
        })
        .collect();
    Ok((library, photos))
}
