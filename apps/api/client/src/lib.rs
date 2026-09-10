use reqwest::{blocking, header, redirect::Policy};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum PhotoState {
    Draft,
    Published { url: String },
}

fn caption<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "caption"
    )]
    pub caption: Option<String>,
    // deserialize_with distinguishes a required null from an absent key.
    #[serde(deserialize_with = "Option::deserialize")]
    pub captured_at: Option<String>,
    pub state: PhotoState,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPage {
    pub items: Vec<Photo>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "_tag")]
pub enum ApiError {
    Unauthorized,
    InvalidCursor { cursor: String },
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
    endpoint: String,
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
                .timeout(Duration::from_secs(20))
                .build()?,
            endpoint: base.join("/v1/probe/photos")?.to_string(),
        })
    }

    pub fn echo_photo(&self, photo: &Photo) -> Result<Photo, ClientError> {
        self.echo_json(photo)
    }

    /// Sends raw JSON for contract validation probes, including intentionally invalid fixtures.
    pub fn echo_json(&self, photo: &impl Serialize) -> Result<Photo, ClientError> {
        decode(self.http.post(&self.endpoint).json(photo).send()?)
    }

    pub fn list_photos(&self, cursor: Option<&str>) -> Result<PhotoPage, ClientError> {
        let mut request = self.http.get(&self.endpoint);
        if let Some(cursor) = cursor {
            request = request.query(&[("cursor", cursor)]);
        }
        decode(request.send()?)
    }
}

fn decode<T: DeserializeOwned>(response: blocking::Response) -> Result<T, ClientError> {
    if response.status().is_success() {
        Ok(response.json()?)
    } else {
        Err(ClientError::Http {
            status: response.status().as_u16(),
            error: response.json().ok(),
        })
    }
}
