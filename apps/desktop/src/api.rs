//! The one door to the photo API. Only this module talks to the network: it reads where the
//! API is and the owner token, and turns each answer into what the transfer lanes do next.
//! The token never reaches the catalog, a log or an error message.

use crate::cloud::{CloudFailure, CloudId};
use photo_upload_client::{ApiError, Client, ClientError, PhotoDetails, Problem};
use std::{
    fmt,
    path::Path,
    process::Command,
    sync::{Arc, Mutex, PoisonError},
};

pub use photo_upload_client::{
    Derivative, Kind as ObjectKind, Manifest, Original, PhotoDetails as Details, PhotoView,
};

/// The most photos one page of the list holds; the API's own limit.
const LIST_PAGE: u32 = 200;

const ADDRESS_VARIABLE: &str = "PHOTO_API_URL";
const TOKEN_VARIABLE: &str = "PHOTO_API_TOKEN";
const KEYCHAIN_SERVICE: &str = "dev.parth.photo-prototype";
const KEYCHAIN_ACCOUNT: &str = "owner-token";

/// The owner token. It has no `Display`, and its `Debug` hides the value.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(hidden)")
    }
}

/// Where the library reads the owner token again after the API refused it: by default
/// [`read_token`]. Runs off the UI thread.
pub type TokenSource = Arc<dyn Fn() -> Option<Token> + Send + Sync>;

/// Where the API is, and the token that opens it.
#[derive(Clone, Debug)]
pub struct Config {
    pub address: String,
    pub token: Token,
}

impl Config {
    /// Reads `PHOTO_API_URL`, then `PHOTO_API_TOKEN` or the Keychain item. Runs `security`, so
    /// call it off the UI thread. The error says what to set.
    pub fn from_environment() -> Result<Self, String> {
        let address = std::env::var(ADDRESS_VARIABLE)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("Set {ADDRESS_VARIABLE} to the photo API's address"))?;
        let token = read_token().ok_or_else(|| {
            format!(
                "Set {TOKEN_VARIABLE}, or add the Keychain item {KEYCHAIN_SERVICE} / {KEYCHAIN_ACCOUNT}"
            )
        })?;
        Ok(Self {
            address: address.trim().to_owned(),
            token,
        })
    }
}

/// The owner token from `PHOTO_API_TOKEN`, else the Keychain item. Runs `security`, so call
/// it off the UI thread.
pub fn read_token() -> Option<Token> {
    std::env::var(TOKEN_VARIABLE)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(keychain_token)
        .map(|token| Token::new(token.trim()))
}

fn keychain_token() -> Option<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE])
        .args(["-a", KEYCHAIN_ACCOUNT, "-w"])
        .output()
        .ok()?;
    let token = String::from_utf8(output.stdout).ok()?;
    (output.status.success() && !token.trim().is_empty()).then_some(token)
}

/// Whether uploads can run, as the window shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    Connected { address: String },
    NotConnected { reason: String },
}

/// What a failed call means for the work that made it.
#[derive(Debug)]
pub(crate) enum CallError {
    /// May pass: a transport error or a server that is briefly unavailable.
    Transient(String),
    /// The server doesn't know the photo: an upload begins again, a publish stops.
    NotFound,
    /// The photo became a draft before this write; ask the server again.
    AlreadyDraft,
    /// The local file to send couldn't be opened or read.
    File(String),
    /// Retrying the same call cannot help.
    Stop(CloudFailure),
    /// The queue paused after the API refused the owner token, so the call was not made.
    Paused,
}

/// The photo API, for the upload workers. The token can change while the lanes run: see
/// [`Api::reauthorize`].
pub(crate) struct Api {
    address: String,
    current: Mutex<(Token, Arc<Client>)>,
}

impl Api {
    pub(crate) fn connect(config: &Config) -> Result<Self, String> {
        let client = client(&config.address, &config.token)?;
        Ok(Self {
            address: config.address.clone(),
            current: Mutex::new((config.token.clone(), Arc::new(client))),
        })
    }

    /// Uses `token` for every later call. Returns whether it differs from the token in use;
    /// the same token changes nothing.
    pub(crate) fn reauthorize(&self, token: Token) -> Result<bool, String> {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        if current.0 == token {
            return Ok(false);
        }
        let client = client(&self.address, &token)?;
        *current = (token, Arc::new(client));
        Ok(true)
    }

    fn client(&self) -> Arc<Client> {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .clone()
    }

    pub(crate) fn begin(&self, manifest: &Manifest) -> Result<PhotoView, CallError> {
        self.client().begin(manifest).map_err(classify)
    }

    /// Streams the file at `path` as one object, with its length from disk.
    pub(crate) fn put_file(
        &self,
        id: &CloudId,
        kind: ObjectKind,
        path: &Path,
    ) -> Result<(), CallError> {
        self.client()
            .put_file(id.as_str(), kind, path)
            .map_err(classify)
    }

    pub(crate) fn finalize(&self, id: &CloudId) -> Result<PhotoView, CallError> {
        self.client().finalize(id.as_str()).map_err(classify)
    }

    /// Publishes a draft, or replaces a published photo's details.
    pub(crate) fn publish(
        &self,
        id: &CloudId,
        details: &PhotoDetails,
    ) -> Result<PhotoView, CallError> {
        self.client()
            .publish(id.as_str(), details)
            .map_err(classify)
    }

    pub(crate) fn unpublish(&self, id: &CloudId) -> Result<PhotoView, CallError> {
        self.client().unpublish(id.as_str()).map_err(classify)
    }

    /// Every photo the server holds, read page by page, up to `max_pages` pages. The flag says
    /// whether the list is complete, so a caller can tell a photo the server lacks from one on a
    /// page it never read.
    pub(crate) fn list(&self, max_pages: usize) -> Result<(Vec<PhotoView>, bool), CallError> {
        let client = self.client();
        let mut photos = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..max_pages {
            let page = client
                .list(after.as_deref(), Some(LIST_PAGE))
                .map_err(classify)?;
            photos.extend(page.photos);
            match page.next {
                Some(next) => after = Some(next),
                None => return Ok((photos, true)),
            }
        }
        Ok((photos, false))
    }
}

/// The error names the address, never the token.
fn client(address: &str, token: &Token) -> Result<Client, String> {
    Client::new(address, &token.0).map_err(|error| format!("Can't use {address}: {error}"))
}

fn classify(error: ClientError) -> CallError {
    let (status, error) = match error {
        ClientError::Transport(error) => {
            let reason = if error.is_timeout() {
                "the request timed out"
            } else if error.is_connect() {
                "couldn't connect"
            } else {
                "the connection failed"
            };
            return CallError::Transient(reason.into());
        }
        ClientError::File(error) => return CallError::File(error.kind().to_string()),
        ClientError::Http { status, error } => (status, error),
    };
    match (status, error) {
        (401, _) => CallError::Stop(CloudFailure::Unauthorized),
        (409, Some(ApiError::PhotoAlreadyDraft { .. })) => CallError::AlreadyDraft,
        (409, Some(ApiError::PhotoNotDraft { .. })) => CallError::Stop(CloudFailure::NotDraft),
        (409, _) => CallError::Stop(CloudFailure::Conflict),
        (404, _) => CallError::NotFound,
        (408 | 429 | 500..=599, _) => CallError::Transient(format!("HTTP {status}")),
        (_, Some(ApiError::FinalizeRejected { problems, .. })) => {
            CallError::Stop(CloudFailure::Rejected {
                message: problems.iter().map(describe).collect::<Vec<_>>().join("; "),
            })
        }
        (_, Some(ApiError::HashMismatch { kind })) => CallError::Stop(CloudFailure::Rejected {
            message: format!("{} doesn't match its checksum", kind.as_str()),
        }),
        (_, Some(ApiError::LengthMismatch { kind, .. })) => {
            CallError::Stop(CloudFailure::Rejected {
                message: format!("{} has the wrong length", kind.as_str()),
            })
        }
        _ => CallError::Stop(CloudFailure::Rejected {
            message: format!("HTTP {status}"),
        }),
    }
}

fn describe(problem: &Problem) -> String {
    match problem {
        Problem::Missing { kind } => format!("{} is missing", kind.as_str()),
        Problem::Corrupt { kind } => format!("{} is damaged", kind.as_str()),
        Problem::NotWebp { kind } => format!("{} is not a WebP", kind.as_str()),
        Problem::HasMetadata { kind } => format!("{} carries metadata", kind.as_str()),
        Problem::Animated { kind } => format!("{} is animated", kind.as_str()),
        Problem::DimensionsMismatch { kind, .. } => {
            format!("{} has other dimensions than declared", kind.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishing_an_unfinished_upload_stops_with_a_reason() {
        let refused = ClientError::Http {
            status: 409,
            error: Some(ApiError::PhotoNotDraft {
                photo_id: "ab".into(),
            }),
        };
        assert!(matches!(
            classify(refused),
            CallError::Stop(CloudFailure::NotDraft)
        ));
    }

    #[test]
    fn a_local_file_error_is_not_blamed_on_the_server() {
        let error = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(matches!(
            classify(ClientError::File(error)),
            CallError::File(_)
        ));
    }
}
