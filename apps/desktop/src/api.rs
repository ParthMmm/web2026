//! The one door to the photo API. Only this module talks to the network: it reads where the
//! API is and the owner token, and turns each answer into what the upload worker does next.
//! The token never reaches the catalog, a log or an error message.

use crate::cloud::{CloudId, UploadFailure};
use photo_upload_client::{ApiError, Client, ClientError, Problem};
use std::{fmt, process::Command};

pub use photo_upload_client::{Derivative, Kind as ObjectKind, Manifest, Original, PhotoView};

const ADDRESS_VARIABLE: &str = "PHOTO_API_URL";
const TOKEN_VARIABLE: &str = "PHOTO_API_TOKEN";
const KEYCHAIN_SERVICE: &str = "dev.parth.photo-prototype";
const KEYCHAIN_ACCOUNT: &str = "owner-token";

/// The owner token. It has no `Display`, and its `Debug` hides the value.
#[derive(Clone)]
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
        let token = std::env::var(TOKEN_VARIABLE)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(keychain_token)
            .ok_or_else(|| {
                format!(
                    "Set {TOKEN_VARIABLE}, or add the Keychain item {KEYCHAIN_SERVICE} / {KEYCHAIN_ACCOUNT}"
                )
            })?;
        Ok(Self {
            address: address.trim().to_owned(),
            token: Token::new(token.trim()),
        })
    }
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

/// What a failed call means for the upload.
#[derive(Debug)]
pub(crate) enum CallError {
    /// May pass: a transport error or a server that is briefly unavailable.
    Transient(String),
    /// The photo became a draft before this write; ask the server again.
    AlreadyDraft,
    /// Retrying the same call cannot help.
    Stop(UploadFailure),
}

/// The photo API, for the upload workers.
pub(crate) struct Api {
    client: Client,
}

impl Api {
    pub(crate) fn connect(config: &Config) -> Result<Self, String> {
        Client::new(&config.address, &config.token.0)
            .map(|client| Self { client })
            .map_err(|error| format!("Can't use {}: {error}", config.address))
    }

    pub(crate) fn begin(&self, manifest: &Manifest) -> Result<PhotoView, CallError> {
        self.client.begin(manifest).map_err(classify)
    }

    pub(crate) fn put(&self, id: &CloudId, kind: ObjectKind, body: &[u8]) -> Result<(), CallError> {
        self.client.put(id.as_str(), kind, body).map_err(classify)
    }

    pub(crate) fn finalize(&self, id: &CloudId) -> Result<PhotoView, CallError> {
        self.client.finalize(id.as_str()).map_err(classify)
    }
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
        ClientError::Http { status, error } => (status, error),
    };
    match (status, error) {
        (401, _) => CallError::Stop(UploadFailure::Unauthorized),
        (409, Some(ApiError::PhotoAlreadyDraft { .. })) => CallError::AlreadyDraft,
        (409, _) => CallError::Stop(UploadFailure::Conflict),
        // The server forgot an upload it had begun; begin again.
        (404, _) | (408 | 429 | 500..=599, _) => CallError::Transient(format!("HTTP {status}")),
        (_, Some(ApiError::FinalizeRejected { problems, .. })) => {
            CallError::Stop(UploadFailure::Rejected {
                message: problems.iter().map(describe).collect::<Vec<_>>().join("; "),
            })
        }
        (_, Some(ApiError::HashMismatch { kind })) => CallError::Stop(UploadFailure::Rejected {
            message: format!("{} doesn't match its checksum", kind.as_str()),
        }),
        (_, Some(ApiError::LengthMismatch { kind, .. })) => {
            CallError::Stop(UploadFailure::Rejected {
                message: format!("{} has the wrong length", kind.as_str()),
            })
        }
        _ => CallError::Stop(UploadFailure::Rejected {
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
