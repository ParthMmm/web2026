//! The window's cloud actions: Upload, Publish, Unpublish and Retry for the selected photos or
//! for every photo, each photo's state as the library reports it, and the words the status bar
//! says about an upload run and each publish.

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

    /// Uploads each selected photo that is only in the library, or retries its upload. The
    /// other selected photos are skipped.
    pub(in crate::ui) fn upload_selected(&mut self) {
        self.ask_selected(CloudStep::Upload);
    }

    /// Publishes each selected draft, or retries its publish. Any unsaved description is saved
    /// first, so the publish carries it.
    pub(in crate::ui) fn publish_selected(&mut self) {
        self.save_description();
        self.ask_selected(CloudStep::Publish);
    }

    /// Takes each selected published photo out of the public gallery, or retries its unpublish.
    pub(in crate::ui) fn unpublish_selected(&mut self) {
        self.ask_selected(CloudStep::Unpublish);
    }

    /// Runs again, for each selected photo, the step that stopped, when a retry can help.
    pub(in crate::ui) fn retry_selected(&mut self) {
        let retries = self
            .selected_photos()
            .filter_map(|photo| Some((photo.record.cloud.retry()?, photo.record.id.clone())))
            .collect();
        self.ask_each(retries);
    }

    /// How many selected photos `step` can start from; a command is on when any can.
    pub(in crate::ui) fn selected_for(&self, step: CloudStep) -> usize {
        self.selected_photos()
            .filter(|photo| photo.record.cloud.start(step).is_some())
            .count()
    }

    /// How many selected photos a Retry can help.
    pub(in crate::ui) fn selected_retries(&self) -> usize {
        self.selected_photos()
            .filter(|photo| photo.record.cloud.retry().is_some())
            .count()
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
        let retries = self
            .cloud_failures()
            .filter_map(|(id, state)| Some((state.retry()?, id.clone())))
            .collect();
        self.ask_each(retries);
        self.close_overlay();
    }

    /// Asks for each retry with the step it needs.
    fn ask_each(&mut self, retries: Vec<(CloudStep, PhotoId)>) {
        for step in CloudStep::ALL {
            let ids = retries
                .iter()
                .filter(|(wanted, _)| *wanted == step)
                .map(|(_, id)| id.clone())
                .collect();
            self.ask(step, ids);
        }
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
        let selected = self.selected_ids();
        self.ask(step, selected);
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
                _ => run.drafts += 1,
            }
        }
        let name = photo.record.file_name();
        if let Some(words) = format::publish_outcome(&photo.record.cloud, &state, &name) {
            self.status = words;
        }
        photo.record.cloud = state;
        self.refilter();
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
