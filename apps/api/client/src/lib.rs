use reqwest::{blocking, header, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;

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

    #[error("HTTP {status}: {error:?}")]
    Http {
        status: u16,
        error: Option<ApiError>,
    },
}

pub struct Client {
    http: blocking::Client,
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
                // The Worker can refuse an upload before reading its body and close the connection; never reuse one.
                .pool_max_idle_per_host(0)
                .timeout(Duration::from_secs(120))
                .build()?,
            base,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.as_str().trim_end_matches('/'))
    }

    pub fn begin(&self, manifest: &Manifest) -> Result<PhotoView, ClientError> {
        let request = BeginRequest {
            manifest: manifest.clone(),
            photo_id: manifest.original.sha256.clone(),
        };
        decode(
            self.http
                .post(self.url("/v1/photos"))
                .json(&request)
                .send()?,
        )
    }

    pub fn get(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        decode(
            self.http
                .get(self.url(&format!("/v1/photos/{photo_id}")))
                .send()?,
        )
    }

    /// Sends one whole object. The body is a slice, so reqwest sets Content-Length, which the
    /// Worker requires before it streams the body to storage.
    pub fn put(&self, photo_id: &str, kind: Kind, body: &[u8]) -> Result<(), ClientError> {
        let response = self
            .http
            .put(self.url(&format!("/v1/photos/{photo_id}/{}", kind.as_str())))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(body.to_vec())
            .send()?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(failure(response))
        }
    }

    pub fn finalize(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        decode(
            self.http
                .post(self.url(&format!("/v1/photos/{photo_id}/finalize")))
                .send()?,
        )
    }

    /// Publishes a draft, or replaces the details of a published photo.
    pub fn publish(
        &self,
        photo_id: &str,
        details: &PhotoDetails,
    ) -> Result<PhotoView, ClientError> {
        decode(
            self.http
                .post(self.url(&format!("/v1/photos/{photo_id}/publish")))
                .json(details)
                .send()?,
        )
    }

    pub fn unpublish(&self, photo_id: &str) -> Result<PhotoView, ClientError> {
        decode(
            self.http
                .post(self.url(&format!("/v1/photos/{photo_id}/unpublish")))
                .send()?,
        )
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
        decode(self.http.get(self.url("/v1/photos")).query(&query).send()?)
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
