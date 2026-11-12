//! Where a photo stands with the photo API, as the catalog keeps it. One value per photo; the
//! catalog writes it before any network call, and the server's answers decide what is true
//! after a restart: `begin` for an upload, a repeated `publish` or `unpublish` for those, and
//! the server's list, read once on open, for every photo no work is under way for.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::api::PhotoView;

pub use crate::api::ObjectKind;

/// The API's identity for a photo: the SHA-256 of the original, in lowercase hex. The library's
/// own identity stays the BLAKE3 hash ([`crate::PhotoId`]); the catalog keeps both.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CloudId(String);

impl CloudId {
    pub fn from_digest(digest: &[u8]) -> Self {
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        Self(hex)
    }

    /// An identity the server or a stored manifest gave, already in hex.
    pub fn from_hex(hex: &str) -> Self {
        Self(hex.to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CloudId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A photo's cloud state. Every state the server may know about carries its [`CloudId`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum CloudState {
    /// Only in this library.
    #[default]
    Local,
    /// Waiting to be prepared: hashed and given its web sizes. The server knows nothing yet.
    Queued,
    /// Prepared and sent to the server, which still misses `missing`. Until the server answers,
    /// every object counts as missing.
    Uploading {
        cloud_id: CloudId,
        missing: Vec<ObjectKind>,
    },
    /// A verified private draft.
    Draft { cloud_id: CloudId },
    /// Asked to publish; the server may or may not have answered.
    Publishing { cloud_id: CloudId },
    /// In the public gallery.
    Published { cloud_id: CloudId },
    /// Asked to unpublish, or the server reports that removal started and has not finished.
    /// Never shown as a draft until the server says it is one.
    Unpublishing { cloud_id: CloudId },
    /// Stopped during `step`. `cloud_id` is set when the server may hold the photo.
    Failed {
        cloud_id: Option<CloudId>,
        failure: CloudFailure,
        #[serde(default)]
        step: CloudStep,
    },
}

/// What the app was doing for a photo when it stopped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudStep {
    /// Catalogs from before publishing hold only upload failures, so a failure without a step
    /// is one.
    #[default]
    Upload,
    Publish,
    Unpublish,
}

impl CloudStep {
    pub const ALL: [Self; 3] = [Self::Upload, Self::Publish, Self::Unpublish];

    /// Whether the server's state shows this step done.
    fn done_in(self, server: &CloudState) -> bool {
        match self {
            Self::Upload => matches!(
                server,
                CloudState::Draft { .. }
                    | CloudState::Published { .. }
                    | CloudState::Unpublishing { .. }
            ),
            Self::Publish => matches!(server, CloudState::Published { .. }),
            Self::Unpublish => matches!(server, CloudState::Draft { .. }),
        }
    }
}

impl CloudState {
    /// The server's identity for the photo, when the server may know it.
    pub fn cloud_id(&self) -> Option<&CloudId> {
        match self {
            Self::Local | Self::Queued => None,
            Self::Uploading { cloud_id, .. }
            | Self::Draft { cloud_id }
            | Self::Publishing { cloud_id }
            | Self::Published { cloud_id }
            | Self::Unpublishing { cloud_id } => Some(cloud_id),
            Self::Failed { cloud_id, .. } => cloud_id.as_ref(),
        }
    }

    /// The state the server reports in `view`.
    pub fn reported(view: &PhotoView) -> Self {
        let cloud_id = CloudId::from_hex(view.photo_id());
        match view {
            PhotoView::Uploading { missing, .. } => Self::Uploading {
                cloud_id,
                missing: missing.clone(),
            },
            PhotoView::Draft { .. } => Self::Draft { cloud_id },
            PhotoView::Published { .. } => Self::Published { cloud_id },
            PhotoView::Unpublishing { .. } => Self::Unpublishing { cloud_id },
        }
    }

    /// Waiting for, or in, an upload.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Queued | Self::Uploading { .. })
    }

    /// Work is under way: a lane owns the photo until it settles, and resumes it after a
    /// restart.
    pub fn is_busy(&self) -> bool {
        self.is_pending() || matches!(self, Self::Publishing { .. } | Self::Unpublishing { .. })
    }

    /// The step a Retry runs, when one can help.
    pub fn retry(&self) -> Option<CloudStep> {
        match self {
            Self::Failed { failure, step, .. } => failure.retry_step(*step),
            _ => None,
        }
    }

    /// Whether an Upload or Retry action can start an upload from here.
    pub fn can_upload(&self) -> bool {
        matches!(self, Self::Local) || self.retry() == Some(CloudStep::Upload)
    }

    /// Whether a Publish or Retry action can publish from here.
    pub fn can_publish(&self) -> bool {
        matches!(self, Self::Draft { .. }) || self.retry() == Some(CloudStep::Publish)
    }

    /// Whether an Unpublish or Retry action can unpublish from here.
    pub fn can_unpublish(&self) -> bool {
        matches!(self, Self::Published { .. }) || self.retry() == Some(CloudStep::Unpublish)
    }

    /// The state `step` starts from here: queued, publishing or unpublishing, or `None` when
    /// the photo can't take that step now.
    pub fn start(&self, step: CloudStep) -> Option<CloudState> {
        let cloud_id = self.cloud_id().cloned();
        match (step, cloud_id) {
            (CloudStep::Upload, _) if self.can_upload() => Some(Self::Queued),
            (CloudStep::Publish, Some(cloud_id)) if self.can_publish() => {
                Some(Self::Publishing { cloud_id })
            }
            (CloudStep::Unpublish, Some(cloud_id)) if self.can_unpublish() => {
                Some(Self::Unpublishing { cloud_id })
            }
            _ => None,
        }
    }

    /// What this state becomes once the server's list is known, or `None` to keep it.
    /// `server` is the server's state for the photo, or `None` when a complete list lacks it.
    /// A busy state is left to its lane, which asks the server itself, and a failure is kept
    /// unless the server shows its step done.
    pub fn reconciled(&self, server: Option<&CloudState>) -> Option<CloudState> {
        let corrected = match (self, server) {
            (Self::Draft { .. } | Self::Published { .. }, None) => Self::Local,
            (Self::Draft { .. } | Self::Published { .. }, Some(server)) => server.clone(),
            (Self::Failed { step, .. }, Some(server)) if step.done_in(server) => server.clone(),
            _ => return None,
        };
        (corrected != *self).then_some(corrected)
    }
}

/// Why cloud work stopped, in terms of what the person can do about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum CloudFailure {
    /// No known path holds the original.
    OriginalMissing,
    /// Every known path holds different bytes than the ones imported.
    OriginalChanged,
    /// The web sizes couldn't be made or read back.
    Renditions { message: String },
    /// The API refused the owner token.
    Unauthorized,
    /// The API kept failing for a reason that may pass, until the retry cap.
    Unavailable { message: String },
    /// The server holds different files under this photo's identity.
    Conflict,
    /// The server checked the files and refused them.
    Rejected { message: String },
    /// The server holds an unfinished upload of the photo, so it can't publish it.
    NotDraft,
    /// The server no longer holds the photo.
    NotInCloud,
}

impl CloudFailure {
    /// One or two sentences: what happened, then what to do.
    pub fn message(&self) -> String {
        match self {
            Self::OriginalMissing => {
                "The original has moved or was deleted. Import it from its new location, then retry.".into()
            }
            Self::OriginalChanged => {
                "The original changed after it was imported. Import it again and upload the new photo."
                    .into()
            }
            Self::Renditions { message } => {
                format!("Couldn't prepare the web sizes ({message}). Retry to make them again.")
            }
            Self::Unauthorized => {
                "The photo API refused the owner token. Update PHOTO_API_TOKEN or the Keychain item, reopen the app, then retry."
                    .into()
            }
            Self::Unavailable { message } => {
                format!("The photo API didn't answer ({message}). Retry when it is reachable.")
            }
            Self::Conflict => {
                "The cloud holds different files for this photo. Nothing was replaced.".into()
            }
            Self::Rejected { message } => format!(
                "The photo API refused the files: {message}. A retry would send the same files, so report it."
            ),
            Self::NotDraft => {
                "The photo API holds an unfinished upload of this photo, so it can't publish it. Retry to finish the upload, then publish."
                    .into()
            }
            Self::NotInCloud => {
                "The photo API no longer holds this photo. Retry to upload it again.".into()
            }
        }
    }

    /// Whether the same step can succeed later without the person changing the photo.
    pub fn retry_helps(&self) -> bool {
        !matches!(
            self,
            Self::OriginalChanged
                | Self::Conflict
                | Self::Rejected { .. }
                | Self::NotDraft
                | Self::NotInCloud
        )
    }

    /// The step a retry runs after this failure stopped `step`: the same step, or an upload
    /// when the server lacks a finished copy, or none when nothing would change.
    pub fn retry_step(&self, step: CloudStep) -> Option<CloudStep> {
        match self {
            Self::NotDraft | Self::NotInCloud => Some(CloudStep::Upload),
            _ => self.retry_helps().then_some(step),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> CloudId {
        CloudId::from_digest(&[0xab; 32])
    }

    fn failed(failure: CloudFailure, step: CloudStep) -> CloudState {
        CloudState::Failed {
            cloud_id: Some(id()),
            failure,
            step,
        }
    }

    fn unavailable() -> CloudFailure {
        CloudFailure::Unavailable {
            message: "HTTP 503".into(),
        }
    }

    #[test]
    fn the_state_round_trips_through_its_catalog_form() {
        let states = [
            CloudState::Local,
            CloudState::Queued,
            CloudState::Uploading {
                cloud_id: id(),
                missing: vec![ObjectKind::Original, ObjectKind::W2400],
            },
            CloudState::Draft { cloud_id: id() },
            CloudState::Publishing { cloud_id: id() },
            CloudState::Published { cloud_id: id() },
            CloudState::Unpublishing { cloud_id: id() },
            failed(unavailable(), CloudStep::Unpublish),
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(serde_json::from_str::<CloudState>(&json).unwrap(), state);
        }
    }

    #[test]
    fn a_failure_stored_before_publishing_existed_is_an_upload_failure() {
        let stored = r#"{"_tag":"Failed","cloud_id":null,"failure":{"_tag":"OriginalMissing"}}"#;
        let state: CloudState = serde_json::from_str(stored).unwrap();
        assert_eq!(state.retry(), Some(CloudStep::Upload));
    }

    #[test]
    fn each_action_is_offered_only_where_it_can_start() {
        assert_eq!(CloudState::Queued.cloud_id(), None);
        assert!(CloudState::Local.can_upload());
        assert!(!CloudState::Queued.can_upload());
        let draft = CloudState::Draft { cloud_id: id() };
        let published = CloudState::Published { cloud_id: id() };
        assert!(draft.can_publish() && !draft.can_unpublish() && !draft.can_upload());
        assert!(published.can_unpublish() && !published.can_publish());
        let changed = failed(CloudFailure::OriginalChanged, CloudStep::Upload);
        assert_eq!(changed.retry(), None);
        let publish = failed(unavailable(), CloudStep::Publish);
        assert!(publish.can_publish() && !publish.can_upload());
        let not_draft = failed(CloudFailure::NotDraft, CloudStep::Publish);
        assert!(not_draft.can_upload() && !not_draft.can_publish());
        assert!(failed(unavailable(), CloudStep::Unpublish).can_unpublish());
    }

    #[test]
    fn the_server_s_list_corrects_settled_states_and_leaves_busy_ones_to_their_lane() {
        let draft = CloudState::Draft { cloud_id: id() };
        let published = CloudState::Published { cloud_id: id() };
        let unpublishing = CloudState::Unpublishing { cloud_id: id() };
        assert_eq!(draft.reconciled(Some(&published)), Some(published.clone()));
        assert_eq!(published.reconciled(Some(&draft)), Some(draft.clone()));
        assert_eq!(
            published.reconciled(Some(&unpublishing)),
            Some(unpublishing.clone()),
            "never Draft while the server says Unpublishing"
        );
        assert_eq!(draft.reconciled(Some(&draft)), None);
        assert_eq!(draft.reconciled(None), Some(CloudState::Local));
        let busy = CloudState::Publishing { cloud_id: id() };
        assert_eq!(busy.reconciled(Some(&draft)), None);
        assert_eq!(busy.reconciled(None), None);
        let publish = failed(unavailable(), CloudStep::Publish);
        assert_eq!(
            publish.reconciled(Some(&draft)),
            None,
            "the person still asked"
        );
        assert_eq!(
            publish.reconciled(Some(&published)),
            Some(published.clone())
        );
        let unpublish = failed(unavailable(), CloudStep::Unpublish);
        assert_eq!(unpublish.reconciled(Some(&unpublishing)), None);
        assert_eq!(unpublish.reconciled(Some(&draft)), Some(draft.clone()));
        assert_eq!(unpublish.reconciled(None), None);
        assert_eq!(CloudState::Local.reconciled(Some(&draft)), None);
    }
}
