use bytes::Bytes;
use reqwest::{blocking, header, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, fs::File, path::Path, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Original,
    W480,
    W1200,
    W2400,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Original, Kind::W480, Kind::W1200, Kind::W2400];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Original => "original",
            Kind::W480 => "w480",
            Kind::W1200 => "w1200",
            Kind::W2400 => "w2400",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Derivative {
    pub bytes: u64,
    pub height: u32,
    pub sha256: String,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub original: Original,
    pub w480: Derivative,
    pub w1200: Derivative,
    pub w2400: Derivative,
}

impl Manifest {
    pub fn facts(&self, kind: Kind) -> (u64, &str) {
        match kind {
            Kind::Original => (self.original.bytes, &self.original.sha256),
            Kind::W480 => (self.w480.bytes, &self.w480.sha256),
            Kind::W1200 => (self.w1200.bytes, &self.w1200.sha256),
            Kind::W2400 => (self.w2400.bytes, &self.w2400.sha256),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginRequest {
    pub manifest: Manifest,
    pub photo_id: String,
}

/// What a visitor sees beside a published photo. `captured_at` is the camera's local date-time as
/// text, with no zone. The API limits `alt` to 300 characters, `captured_at` to 40 and `film` to 64.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub film: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum PhotoView {
    Uploading {
        manifest: Manifest,
        missing: Vec<Kind>,
        photo_id: String,
    },
    Draft {
        finalized_at: String,
        manifest: Manifest,
        photo_id: String,
    },
    Published {
        details: PhotoDetails,
        finalized_at: String,
        manifest: Manifest,
        photo_id: String,
        published_at: String,
    },
    /// Public removal started and has not finished. Another `unpublish` finishes it.
    Unpublishing {
        details: PhotoDetails,
        finalized_at: String,
        manifest: Manifest,
        photo_id: String,
    },
}

impl PhotoView {
    pub fn photo_id(&self) -> &str {
        match self {
            PhotoView::Uploading { photo_id, .. }
            | PhotoView::Draft { photo_id, .. }
            | PhotoView::Published { photo_id, .. }
            | PhotoView::Unpublishing { photo_id, .. } => photo_id,
        }
    }
}

/// A presigned PUT that sends an original straight to storage. Send the bytes to `url` with
/// exactly these `headers` before `expires_at`. Storage refuses other bytes, other headers, or a
/// late start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginalUpload {
    pub expires_at: String,
    pub headers: HashMap<String, String>,
    pub method: String,
    pub url: String,
}

/// One page of `list`. Pass `next` as `after` to read the following page; it is `None` on the last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotoPage {
    pub next: Option<String>,
    pub photos: Vec<PhotoView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GallerySize {
    pub height: u32,
    /// Relative to the gallery base, such as `https://host/gallery/`.
    pub path: String,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GallerySizes {
    pub w480: GallerySize,
    pub w1200: GallerySize,
    pub w2400: GallerySize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPhoto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub film: Option<String>,
    pub id: String,
    pub published_at: String,
    pub sizes: GallerySizes,
}

/// The public gallery manifest, version 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryManifest {
    pub generated_at: String,
    pub photos: Vec<GalleryPhoto>,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dimensions {
    pub height: u32,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum Problem {
    Missing {
        kind: Kind,
    },
    Corrupt {
        kind: Kind,
    },
    NotWebp {
        kind: Kind,
    },
    HasMetadata {
        kind: Kind,
    },
    Animated {
        kind: Kind,
    },
    DimensionsMismatch {
        expected: Dimensions,
        found: Dimensions,
        kind: Kind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_tag", rename_all_fields = "camelCase")]
pub enum ApiError {
    Unauthorized,
    PhotoNotFound {
        photo_id: String,
    },
    PhotoManifestConflict {
        photo_id: String,
    },
    PhotoAlreadyDraft {
        photo_id: String,
    },
    PhotoNotDraft {
        photo_id: String,
    },
    LengthMismatch {
        expected: u64,
        kind: Kind,
        received: u64,
    },
    HashMismatch {
        kind: Kind,
    },
    FinalizeRejected {
        photo_id: String,
        problems: Vec<Problem>,
    },
    StorageUnavailable {
        operation: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error(transparent)]
    File(#[from] std::io::Error),

    #[error("HTTP {status}: {error:?}")]
    Http {
        status: u16,
        error: Option<ApiError>,
    },

    /// Storage refused a presigned upload. `code` is its S3 error code, such as `BadDigest`
    /// for bytes that do not match the signed SHA-256, or `AccessDenied` for an expired URL.
    #[error("storage HTTP {status}: {code:?}")]
    Storage { status: u16, code: Option<String> },
}

/// Time for a request to storage before the body: connect, TLS and the first answer.
const STORAGE_BASE_TIMEOUT: Duration = Duration::from_secs(120);
/// The slowest upload rate an original may take before the request times out.
const STORAGE_MIN_BYTES_PER_SECOND: u64 = 256 * 1024;

pub struct Client {
    http: blocking::Client,
    /// Sends presigned uploads. It has no default headers, so the owner token stays with the API.
    storage: blocking::Client,
    base: reqwest::Url,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> anyhow::Result<Self> {
        let base = reqwest::Url::parse(base_url)?;
        anyhow::ensure!(
            base.scheme() == "https"
                || (base.scheme() == "http" && base.host_str() == Some("127.0.0.1")),
            "Use HTTPS, or HTTP on 127.0.0.1 for local tests"
        );
        let mut authorization = header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        authorization.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        Ok(Self {
            http: blocking::Client::builder()
                .default_headers(headers)
                .redirect(Policy::none())
                .timeout(Duration::from_secs(120))
                .build()?,
            storage: blocking::Client::builder()
                .redirect(Policy::none())
                .build()?,
            base,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.as_str().trim_end_matches('/'))
    }

    /// Sends a request, and sends it once more on a new connection if the connection dropped
    /// before any response. The Worker can refuse an upload before it reads the body and close
    /// the connection, and a pooled connection can close between requests. Every call is safe to
    /// repeat, so one retry is safe.
    fn send(
        &self,
        request: impl Fn() -> Result<blocking::RequestBuilder, ClientError>,
    ) -> Result<blocking::Response, ClientError> {
        match request()?.send() {
            Err(error) if is_dropped_connection(&error) => Ok(request()?.send()?),
            result => Ok(result?),
        }
    }

    fn put_body(
        &self,
        photo_id: &str,
        kind: Kind,
        body: impl Fn() -> Result<blocking::Body, ClientError>,
    ) -> Result<(), ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}/{}", kind.as_str()));
        let response = self.send(|| {
            Ok(self
                .http
                .put(&url)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(body()?))
        })?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(failure(response))
        }
    }

    pub fn begin(&self, manifest: &Manifest) -> Result<PhotoView, ClientError> {
        let request = BeginRequest {
            manifest: manifest.clone(),
            photo_id: manifest.original.sha256.clone(),
        };
        decode(self.send(|| Ok(self.http.post(self.url("/v1/photos")).json(&request)))?)
    }

    pub fn get(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}"));
        decode(self.send(|| Ok(self.http.get(&url)))?)
    }

    /// Sends one whole object from memory. A `Vec<u8>` or `Bytes` moves in without a copy. The
    /// length is known, so reqwest sets Content-Length, which the Worker requires before it
    /// streams the body to storage.
    pub fn put(
        &self,
        photo_id: &str,
        kind: Kind,
        body: impl Into<Bytes>,
    ) -> Result<(), ClientError> {
        let body = body.into();
        self.put_body(photo_id, kind, || Ok(body.clone().into()))
    }

    /// Streams one whole object from a file, with Content-Length set from its size on disk.
    pub fn put_file(&self, photo_id: &str, kind: Kind, path: &Path) -> Result<(), ClientError> {
        self.put_body(photo_id, kind, || {
            let file = File::open(path)?;
            let length = file.metadata()?.len();
            Ok(blocking::Body::sized(file, length))
        })
    }

    /// A presigned PUT for the original of a photo that is still uploading.
    pub fn original_upload(&self, photo_id: &str) -> Result<OriginalUpload, ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}/original/upload-url"));
        decode(self.send(|| Ok(self.http.post(&url)))?)
    }

    /// Sends the original straight to storage from memory, through a fresh presigned URL.
    pub fn put_original(&self, photo_id: &str, body: impl Into<Bytes>) -> Result<(), ClientError> {
        let body = body.into();
        let length = body.len() as u64;
        self.put_original_body(photo_id, length, || Ok(body.clone().into()))
    }

    /// Streams the original from a file straight to storage, through a fresh presigned URL. The
    /// Worker never sees the bytes, so the original may be larger than a Worker request.
    pub fn put_original_file(&self, photo_id: &str, path: &Path) -> Result<(), ClientError> {
        let length = std::fs::metadata(path)?.len();
        self.put_original_body(photo_id, length, || {
            let file = File::open(path)?;
            let length = file.metadata()?.len();
            Ok(blocking::Body::sized(file, length))
        })
    }

    fn put_original_body(
        &self,
        photo_id: &str,
        length: u64,
        body: impl Fn() -> Result<blocking::Body, ClientError>,
    ) -> Result<(), ClientError> {
        let upload = self.original_upload(photo_id)?;
        let timeout = STORAGE_BASE_TIMEOUT
            + Duration::from_secs(length.div_ceil(STORAGE_MIN_BYTES_PER_SECOND));
        let request = || -> Result<blocking::RequestBuilder, ClientError> {
            let mut request = self.storage.put(&upload.url).timeout(timeout).body(body()?);
            // reqwest sets Content-Length from the sized body; the signed value must match it.
            for (name, value) in &upload.headers {
                if !name.eq_ignore_ascii_case("content-length") {
                    request = request.header(name, value);
                }
            }
            Ok(request)
        };
        let response = self.send(request)?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status().as_u16();
        let code = response.text().ok().and_then(|text| s3_code(&text));
        Err(ClientError::Storage { status, code })
    }

    pub fn finalize(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}/finalize"));
        decode(self.send(|| Ok(self.http.post(&url)))?)
    }

    /// Publishes a draft, or replaces the details of a published photo.
    pub fn publish(
        &self,
        photo_id: &str,
        details: &PhotoDetails,
    ) -> Result<PhotoView, ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}/publish"));
        decode(self.send(|| Ok(self.http.post(&url).json(details)))?)
    }

    pub fn unpublish(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        let url = self.url(&format!("/v1/photos/{photo_id}/unpublish"));
        decode(self.send(|| Ok(self.http.post(&url)))?)
    }

    /// One page of every photo in photo ID order. `limit` is 1 to 200; the API defaults to 100.
    pub fn list(&self, after: Option<&str>, limit: Option<u32>) -> Result<PhotoPage, ClientError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(after) = after {
            query.push(("after", after.to_owned()));
        }
        if let Some(limit) = limit {
            query.push(("limit", limit.to_string()));
        }
        decode(self.send(|| Ok(self.http.get(self.url("/v1/photos")).query(&query)))?)
    }
}

/// Reads the public gallery the way a visitor does: no token.
pub struct Gallery {
    http: blocking::Client,
    base: String,
}

impl Gallery {
    /// `base_url` is the gallery base, such as `https://host/gallery/`.
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        reqwest::Url::parse(base_url)?;
        let base = base_url.trim_end_matches('/').to_owned();
        Ok(Self {
            http: blocking::Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(60))
                .build()?,
            base,
        })
    }

    pub fn manifest(&self) -> Result<GalleryManifest, ClientError> {
        decode(self.http.get(format!("{}/v1.json", self.base)).send()?)
    }

    /// The bytes at a manifest `path`, or the HTTP status when there are none.
    pub fn fetch(&self, path: &str) -> Result<Vec<u8>, ClientError> {
        let response = self.http.get(format!("{}/{path}", self.base)).send()?;
        if response.status().is_success() {
            Ok(response.bytes()?.to_vec())
        } else {
            Err(failure(response))
        }
    }
}

/// The `<Code>` of an S3 XML error body.
fn s3_code(body: &str) -> Option<String> {
    let start = body.find("<Code>")? + "<Code>".len();
    let end = start + body[start..].find("</Code>")?;
    Some(body[start..end].to_owned())
}

fn is_dropped_connection(error: &reqwest::Error) -> bool {
    !error.is_timeout() && (error.is_request() || error.is_connect())
}

fn failure(response: blocking::Response) -> ClientError {
    ClientError::Http {
        status: response.status().as_u16(),
        error: response.json().ok(),
    }
}

fn decode<T: DeserializeOwned>(response: blocking::Response) -> Result<T, ClientError> {
    if response.status().is_success() {
        Ok(response.json()?)
    } else {
        Err(failure(response))
    }
}
