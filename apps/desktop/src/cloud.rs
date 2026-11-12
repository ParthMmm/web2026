//! Where a photo stands with the photo API, as the catalog keeps it. One value per photo; the
//! catalog writes it before any network call, and the server's answer to `begin` decides what
//! is left after a restart.

use serde::{Deserialize, Serialize};
use std::fmt;

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
    Draft {
        cloud_id: CloudId,
    },
    Publishing {
        cloud_id: CloudId,
    },
    Published {
        cloud_id: CloudId,
    },
    Unpublishing {
        cloud_id: CloudId,
    },
    /// Stopped. `cloud_id` is set when the server may hold part of the upload.
    Failed {
        cloud_id: Option<CloudId>,
        failure: UploadFailure,
    },
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

    /// Waiting for, or in, an upload.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Queued | Self::Uploading { .. })
    }

    /// Whether an Upload or Retry action can start an upload from here.
    pub fn can_upload(&self) -> bool {
        match self {
            Self::Local => true,
            Self::Failed { failure, .. } => failure.retry_helps(),
            _ => false,
        }
    }
}

/// Why an upload stopped, in terms of what the person can do about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum UploadFailure {
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
}

impl UploadFailure {
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
        }
    }

    /// Whether the same upload can succeed later without the person changing the photo.
    pub fn retry_helps(&self) -> bool {
        !matches!(
            self,
            Self::OriginalChanged | Self::Conflict | Self::Rejected { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_round_trips_through_its_catalog_form() {
        let id = CloudId::from_digest(&[0xab; 32]);
        let states = [
            CloudState::Local,
            CloudState::Queued,
            CloudState::Uploading {
                cloud_id: id.clone(),
                missing: vec![ObjectKind::Original, ObjectKind::W2400],
            },
            CloudState::Draft {
                cloud_id: id.clone(),
            },
            CloudState::Failed {
                cloud_id: Some(id),
                failure: UploadFailure::Unavailable {
                    message: "HTTP 503".into(),
                },
            },
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(serde_json::from_str::<CloudState>(&json).unwrap(), state);
        }
    }

    #[test]
    fn only_states_the_server_may_know_carry_a_cloud_id() {
        assert_eq!(CloudState::Queued.cloud_id(), None);
        assert!(CloudState::Local.can_upload());
        assert!(!CloudState::Queued.can_upload());
        let changed = CloudState::Failed {
            cloud_id: None,
            failure: UploadFailure::OriginalChanged,
        };
        assert!(!changed.can_upload());
    }
}
