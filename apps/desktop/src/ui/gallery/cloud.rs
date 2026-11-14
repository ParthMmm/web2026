//! The window's cloud actions: Upload, Cancel, Publish, Unpublish and Retry for the selected
//! photo or for every photo, each photo's state as the library reports it, and the words the
//! status bar says about an upload run, each publish, and a refused owner token.

use super::{Gallery, LibraryState, Overlay};
use crate::{
    PhotoId,
    api::Connection,
    cloud::{CloudFailure, CloudState, CloudStep},
    ui::format,
};

/// Uploads since the queue was last empty, for the status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct UploadRun {
    pub(in crate::ui) total: usize,
    pub(in crate::ui) drafts: usize,
    pub(in crate::ui) failed: usize,
}

impl UploadRun {
    pub(in crate::ui) fn finished(&self) -> usize {
        self.drafts + self.failed
    }
}

impl Gallery {
    pub(in crate::ui) fn is_connected(&self) -> bool {
        matches!(self.connection, Connection::Connected { .. })
    }

    /// Uploads the selected photo, or retries it.
    pub(in crate::ui) fn upload_selected(&mut self) {
        self.ask_selected(CloudStep::Upload);
    }

    /// Publishes the selected draft, or retries its publish. Any unsaved description is saved
    /// first, so the publish carries it.
    pub(in crate::ui) fn publish_selected(&mut self) {
        self.save_description();
        self.ask_selected(CloudStep::Publish);
    }

    /// Takes the selected photo out of the public gallery, or retries its unpublish.
    pub(in crate::ui) fn unpublish_selected(&mut self) {
        self.ask_selected(CloudStep::Unpublish);
    }

    /// Runs again the step that stopped for the selected photo, when a retry can help.
    pub(in crate::ui) fn retry_selected(&mut self) {
        let Some(step) = self
            .selected_photo()
            .and_then(|photo| photo.record.cloud.retry())
        else {
            return;
        };
        self.ask_selected(step);
    }

    /// Stops the selected photo's upload, if it is queued or uploading.
    pub(in crate::ui) fn cancel_selected(&mut self) {
        let selected = self.selected.clone();
        self.cancel(selected.into_iter().collect());
    }

    /// Stops every queued or uploading photo's upload.
    pub(in crate::ui) fn cancel_all(&mut self) {
        let ids = self
            .photos
            .iter()
            .map(|photo| photo.record.id.clone())
            .collect();
        self.cancel(ids);
    }

    /// Hands the queued or uploading photos among `ids` to the library to cancel, and shows
    /// each one only in the library at once. The demo library changes nothing.
    fn cancel(&mut self, ids: Vec<PhotoId>) {
        let LibraryState::Open(library) = &self.library else {
            return;
        };
        let pending: Vec<(PhotoId, String)> = ids
            .into_iter()
            .filter_map(|id| {
                let photo = self.photos.iter().find(|photo| photo.record.id == id)?;
                photo
                    .record
                    .cloud
                    .is_pending()
                    .then(|| (id, photo.record.file_name()))
            })
            .collect();
        let Some((_, name)) = pending.first() else {
            return;
        };
        let words = format::cancelled(pending.len(), name);
        library.cancel(pending.iter().map(|(id, _)| id.clone()).collect());
        for (id, _) in pending {
            self.cloud_changed(&id, CloudState::Local);
        }
        self.status = words;
    }

    /// Uploads every photo that is only in the library, and retries the failed uploads a retry
    /// can help.
    pub(in crate::ui) fn upload_all(&mut self) {
        let ids = self
            .photos
            .iter()
            .map(|photo| photo.record.id.clone())
            .collect();
        self.ask(CloudStep::Upload, ids);
    }

    /// Retries every failure a retry can help, each with the step it needs.
    pub(in crate::ui) fn retry_cloud(&mut self) {
        let retries: Vec<(CloudStep, PhotoId)> = self
            .cloud_failures()
            .filter_map(|(id, state)| Some((state.retry()?, id.clone())))
            .collect();
        for step in CloudStep::ALL {
            let ids = retries
                .iter()
                .filter(|(wanted, _)| *wanted == step)
                .map(|(_, id)| id.clone())
                .collect();
            self.ask(step, ids);
        }
        self.close_overlay();
    }

    pub(in crate::ui) fn open_cloud_failures(&mut self) {
        self.overlay = Overlay::CloudFailures;
    }

    /// Photos whose cloud work stopped, with their failed state, in grid order.
    pub(in crate::ui) fn cloud_failures(&self) -> impl Iterator<Item = (&PhotoId, &CloudState)> {
        self.photos
            .iter()
            .filter(|photo| matches!(photo.record.cloud, CloudState::Failed { .. }))
            .map(|photo| (&photo.record.id, &photo.record.cloud))
    }

    fn ask_selected(&mut self, step: CloudStep) {
        let selected = self.selected.clone();
        self.ask(step, selected.into_iter().collect());
    }

    /// Hands the photos among `ids` that can take `step` to the library. The library writes
    /// the new state to the catalog before any call; the gallery shows it at once.
    fn ask(&mut self, step: CloudStep, ids: Vec<PhotoId>) {
        let LibraryState::Open(library) = &self.library else {
            return;
        };
        if !self.is_connected() {
            return;
        }
        let started: Vec<(PhotoId, CloudState)> = ids
            .into_iter()
            .filter_map(|id| {
                let photo = self.photos.iter().find(|photo| photo.record.id == id)?;
                let next = photo.record.cloud.start(step)?;
                Some((id, next))
            })
            .collect();
        let ids: Vec<PhotoId> = started.iter().map(|(id, _)| id.clone()).collect();
        match step {
            CloudStep::Upload => library.upload(ids),
            CloudStep::Publish => library.publish(ids),
            CloudStep::Unpublish => library.unpublish(ids),
        }
        for (id, state) in started {
            self.cloud_changed(&id, state);
        }
    }

    /// Applies a photo's new cloud state. Uploads count in the run, and when nothing is left
    /// to upload, the status bar says how the run ended. A publish or unpublish that ends says
    /// so in the status bar.
    pub(in crate::ui) fn cloud_changed(&mut self, id: &PhotoId, state: CloudState) {
        let Some(photo) = self.photos.iter_mut().find(|photo| &photo.record.id == id) else {
            return;
        };
        let was_pending = photo.record.cloud.is_pending();
        let now_pending = state.is_pending();
        let run = self.uploads.get_or_insert_default();
        if !was_pending && now_pending {
            run.total += 1;
        }
        if was_pending && !now_pending {
            match state {
                CloudState::Failed { .. } => run.failed += 1,
                // Cancelled: no longer part of the run.
                CloudState::Local => run.total = run.total.saturating_sub(1),
                _ => run.drafts += 1,
            }
        }
        let name = photo.record.file_name();
        if let Some(words) = format::publish_outcome(&photo.record.cloud, &state, &name) {
            self.status = words;
        }
        photo.record.cloud = state;
        if !self
            .photos
            .iter()
            .any(|photo| photo.record.cloud.is_pending())
            && let Some(run) = self.uploads.take()
            && run.total > 0
        {
            self.status = format::upload_summary(&run);
        }
    }

    /// Says what the check against the server's list found, when there is anything to say.
    pub(in crate::ui) fn cloud_checked(&mut self, corrected: Result<usize, String>) {
        match corrected {
            Ok(0) => {}
            Ok(corrected) => {
                self.status = format!(
                    "Matched the photo API: updated {}",
                    format::count(corrected, "photo", "photos")
                );
            }
            Err(message) => self.status = format!("Couldn't compare with the photo API: {message}"),
        }
    }

    /// The API refused the owner token: one photo failed with the reason, and the rest wait.
    pub(in crate::ui) fn cloud_paused(&mut self) {
        self.cloud_paused = true;
        self.status = "The photo API refused the owner token, so uploads and publishing are paused. Update PHOTO_API_TOKEN or the Keychain item, then retry.".into();
    }

    pub(in crate::ui) fn cloud_resumed(&mut self) {
        self.cloud_paused = false;
        self.status = "Resumed uploads and publishing with the new owner token".into();
    }

    pub(in crate::ui) fn token_unchanged(&mut self) {
        self.status = "The owner token is the same, so uploads and publishing stay paused. Update PHOTO_API_TOKEN or the Keychain item, then retry.".into();
    }

    /// Counts the uploads a restart left pending, so the status bar shows them resuming.
    pub(super) fn resume_uploads(&mut self) {
        let pending = self
            .photos
            .iter()
            .filter(|photo| photo.record.cloud.is_pending())
            .count();
        self.uploads = (pending > 0).then_some(UploadRun {
            total: pending,
            ..UploadRun::default()
        });
    }

    /// How many photos are being published and unpublished, for the status bar.
    pub(in crate::ui) fn publishing(&self) -> (usize, usize) {
        self.photos
            .iter()
            .fold((0, 0), |(publishing, unpublishing), photo| {
                match photo.record.cloud {
                    CloudState::Publishing { .. } => (publishing + 1, unpublishing),
                    CloudState::Unpublishing { .. } => (publishing, unpublishing + 1),
                    _ => (publishing, unpublishing),
                }
            })
    }
}

/// The failure a failed state carries, for the sheet and the inspector.
pub(in crate::ui) fn failure_of(state: &CloudState) -> Option<(&CloudFailure, CloudStep)> {
    match state {
        CloudState::Failed { failure, step, .. } => Some((failure, *step)),
        _ => None,
    }
}
