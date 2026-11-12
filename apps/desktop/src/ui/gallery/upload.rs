//! Sending photos to the cloud as private drafts: the Upload and Retry actions, each photo's
//! state as the library reports it, and the run the status bar counts.

use super::{Gallery, LibraryState, Overlay};
use crate::{
    PhotoId,
    api::Connection,
    cloud::{CloudState, UploadFailure},
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
        let selected = self.selected.clone();
        self.upload(selected.into_iter().collect());
    }

    /// Uploads every photo that is only in the library, and retries the failed ones a retry
    /// can help.
    pub(in crate::ui) fn upload_all(&mut self) {
        let ids = self
            .photos
            .iter()
            .map(|photo| photo.record.id.clone())
            .collect();
        self.upload(ids);
    }

    /// Retries every failed upload a retry can help.
    pub(in crate::ui) fn retry_uploads(&mut self) {
        let ids = self
            .upload_failures()
            .filter(|(_, failure)| failure.retry_helps())
            .map(|(id, _)| id.clone())
            .collect();
        self.upload(ids);
        self.close_overlay();
    }

    pub(in crate::ui) fn open_upload_failures(&mut self) {
        self.overlay = Overlay::UploadFailures;
    }

    /// Photos whose upload stopped, with why, in grid order.
    pub(in crate::ui) fn upload_failures(
        &self,
    ) -> impl Iterator<Item = (&PhotoId, &UploadFailure)> {
        self.photos
            .iter()
            .filter_map(|photo| match &photo.record.cloud {
                CloudState::Failed { failure, .. } => Some((&photo.record.id, failure)),
                _ => None,
            })
    }

    /// Queues the photos among `ids` that can upload. The library marks each queued before
    /// it does anything else; the gallery shows that at once.
    fn upload(&mut self, ids: Vec<PhotoId>) {
        let LibraryState::Open(library) = &self.library else {
            return;
        };
        if !self.is_connected() {
            return;
        }
        let ids: Vec<PhotoId> = ids
            .into_iter()
            .filter(|id| {
                self.photos
                    .iter()
                    .any(|photo| &photo.record.id == id && photo.record.cloud.can_upload())
            })
            .collect();
        library.upload(ids.clone());
        for id in ids {
            self.cloud_changed(&id, CloudState::Queued);
        }
    }

    /// Applies a photo's new cloud state and counts it in the run. When nothing is left to
    /// upload, the status bar says how the run ended.
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
                _ => run.drafts += 1,
            }
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
}
