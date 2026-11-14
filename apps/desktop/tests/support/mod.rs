//! A small in-process photo API for upload and publish tests. It follows the Worker's contract
//! for `begin`, `get`, `put`, `originalUpload`, `finalize`, `publish`, `unpublish` and `list`
//! (identity by SHA-256, checked lengths and hashes, `409` on a write after upload, `finalize`
//! answering a published photo's real view, `PhotoNotDraft` for publishing an upload). It also
//! stands in for storage: the original arrives by a presigned PUT under `/r2/`, which needs no
//! token and refuses one, and is counted as `Route::Put(ObjectKind::Original)`. It lets a test
//! inject faults: an HTTP status, a lost answer, a delay, an unpublish that stops midway, or a
//! hold until the test releases it.

#![allow(dead_code)]

pub mod flow;

use photo_prototype::api::{Manifest, ObjectKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

pub const TOKEN: &str = "test-owner-token-0123456789abcdef";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Begin,
    Get,
    Put(ObjectKind),
    /// `originalUpload`: a presigned PUT for the original.
    OriginalUpload,
    Finalize,
    Publish,
    Unpublish,
    List,
}

#[derive(Clone, Debug)]
pub enum Action {
    /// Answer with this status and a tagged error body, without doing the work.
    Status(u16, Value),
    /// Do the work, then close the connection without answering.
    LoseAnswer,
    /// Wait, then do the work and answer.
    Delay(Duration),
    /// Wait until the test calls [`FakeApi::release`], then do the work and answer.
    Hold,
    /// For `unpublish`: start the removal, then fail with 503 before it finishes, as the
    /// Worker does when deleting the public copies fails.
    Midway,
}

struct Fault {
    route: Route,
    action: Action,
    remaining: usize,
}

/// Where a photo stands on the server, as the Worker's ledger keeps it.
#[derive(Clone, Debug, PartialEq)]
enum Stage {
    Uploading,
    Draft,
    Published { details: Value },
    Unpublishing { details: Value },
}

struct Photo {
    manifest: Value,
    stored: HashSet<ObjectKind>,
    stage: Stage,
}

#[derive(Default)]
struct State {
    photos: HashMap<String, Photo>,
    calls: Vec<Route>,
    faults: Vec<Fault>,
    held: bool,
    released: bool,
}

#[derive(Clone)]
pub struct FakeApi {
    address: String,
    state: Arc<(Mutex<State>, Condvar)>,
}

impl FakeApi {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let api = Self {
            address,
            state: Arc::default(),
        };
        let server = api.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let server = server.clone();
                std::thread::spawn(move || server.serve(stream));
            }
        });
        api
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// The next `times` calls to `route` meet `action`.
    pub fn fault(&self, route: Route, action: Action, times: usize) {
        self.lock().faults.push(Fault {
            route,
            action,
            remaining: times,
        });
    }

    pub fn clear_faults(&self) {
        self.lock().faults.clear();
    }

    /// Lets held calls go on, and every later one too.
    pub fn release(&self) {
        self.lock().released = true;
        self.state.1.notify_all();
    }

    /// Whether a call is waiting on a hold.
    pub fn holding(&self) -> bool {
        self.lock().held
    }

    /// Every call the API answered or dropped, in order.
    pub fn calls(&self) -> Vec<Route> {
        self.lock().calls.clone()
    }

    pub fn count(&self, route: Route) -> usize {
        self.calls().iter().filter(|call| **call == route).count()
    }

    pub fn puts(&self) -> usize {
        self.calls()
            .iter()
            .filter(|call| matches!(call, Route::Put(_)))
            .count()
    }

    pub fn drafts(&self) -> Vec<String> {
        let state = self.lock();
        let mut ids: Vec<String> = state
            .photos
            .iter()
            .filter(|(_, photo)| photo.stage == Stage::Draft)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn manifest(&self, id: &str) -> Option<Manifest> {
        let state = self.lock();
        let photo = state.photos.get(id)?;
        serde_json::from_value(photo.manifest.clone()).ok()
    }

    /// Publishes a photo on the server, as another run of the app would have, with every
    /// object stored.
    pub fn publish_directly(&self, id: &str) {
        let mut state = self.lock();
        let photo = state
            .photos
            .get_mut(id)
            .expect("the server knows the photo");
        photo.stored = ObjectKind::ALL.into_iter().collect();
        photo.stage = Stage::Published { details: json!({}) };
    }

    /// Makes a published photo a draft again, as another run's unpublish would have.
    pub fn unpublish_directly(&self, id: &str) {
        let mut state = self.lock();
        state
            .photos
            .get_mut(id)
            .expect("the server knows the photo")
            .stage = Stage::Draft;
    }

    /// Reopens a draft for upload and drops its largest web size, as a `begin` with new
    /// renditions from another run would have.
    pub fn reopen(&self, id: &str) {
        let mut state = self.lock();
        let photo = state
            .photos
            .get_mut(id)
            .expect("the server knows the photo");
        photo.stored.remove(&ObjectKind::W2400);
        photo.stage = Stage::Uploading;
    }

    /// Forgets a photo, as a fresh local Worker would.
    pub fn forget(&self, id: &str) {
        self.lock().photos.remove(id);
    }

    /// The public gallery: each published photo's identity and details, by identity.
    pub fn gallery(&self) -> Vec<(String, Value)> {
        let state = self.lock();
        let mut photos: Vec<(String, Value)> = state
            .photos
            .iter()
            .filter_map(|(id, photo)| match &photo.stage {
                Stage::Published { details } => Some((id.clone(), details.clone())),
                _ => None,
            })
            .collect();
        photos.sort_by(|a, b| a.0.cmp(&b.0));
        photos
    }

    /// The photo's view tag, as `get` would answer it.
    pub fn stage(&self, id: &str) -> Option<String> {
        let state = self.lock();
        let photo = state.photos.get(id)?;
        view(id, photo)["_tag"].as_str().map(str::to_owned)
    }

    pub fn knows_any_photo(&self) -> bool {
        !self.lock().photos.is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn serve(&self, mut stream: TcpStream) {
        let Some((method, path, headers, body)) = read_request(&stream) else {
            return;
        };
        let storage = path.starts_with("/r2/");
        let authorized = if storage {
            // A presigned PUT carries its signature in the URL; the owner token must stay home.
            !headers.contains_key("authorization")
        } else {
            headers.get("authorization").map(String::as_str) == Some(&format!("Bearer {TOKEN}"))
        };
        let Some(route) = route(&method, &path) else {
            respond(&mut stream, 404, &json!({"_tag": "NotFound"}));
            return;
        };
        let action = self.take_fault(route);
        if let Some(Action::Status(status, body)) = &action {
            self.lock().calls.push(route);
            respond(&mut stream, *status, body);
            return;
        }
        if matches!(action, Some(Action::Midway)) && authorized {
            let mut state = self.lock();
            state.calls.push(route);
            if let Some(photo) = state.photos.get_mut(photo_id(&path))
                && let Stage::Published { details } = &photo.stage
            {
                photo.stage = Stage::Unpublishing {
                    details: details.clone(),
                };
            }
            drop(state);
            respond(
                &mut stream,
                503,
                &json!({"_tag": "StorageUnavailable", "operation": "delete"}),
            );
            return;
        }
        match &action {
            Some(Action::Delay(wait)) => std::thread::sleep(*wait),
            Some(Action::Hold) => self.wait_for_release(),
            _ => {}
        }
        let (status, answer) = if !authorized {
            (401, json!({"_tag": "Unauthorized"}))
        } else if storage {
            self.store_original(&path, &headers, &body)
        } else {
            self.handle(route, &path, &body)
        };
        self.lock().calls.push(route);
        if matches!(action, Some(Action::LoseAnswer)) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        }
        match answer["Code"].as_str() {
            Some(code) if storage => respond_xml(&mut stream, status, code),
            _ => respond(&mut stream, status, &answer),
        }
    }

    fn take_fault(&self, route: Route) -> Option<Action> {
        let mut state = self.lock();
        let fault = state
            .faults
            .iter_mut()
            .find(|fault| fault.route == route && fault.remaining > 0)?;
        fault.remaining -= 1;
        Some(fault.action.clone())
    }

    fn wait_for_release(&self) {
        let mut state = self.lock();
        state.held = true;
        while !state.released {
            state = self.state.1.wait(state).unwrap();
        }
        state.held = false;
    }

    /// What R2 does with a presigned PUT: the bytes must match the signed length and SHA-256.
    fn store_original(
        &self,
        path: &str,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> (u16, Value) {
        let mut state = self.lock();
        let id = photo_id(path).to_owned();
        let Some(photo) = state.photos.get_mut(&id) else {
            return (404, json!({"Code": "NoSuchUpload"}));
        };
        let signed = |name: &str| headers.get(name).map(String::as_str).unwrap_or_default();
        if signed("x-amz-checksum-sha256") != hex(&Sha256::digest(body))
            || signed("content-length") != body.len().to_string()
        {
            return (400, json!({"Code": "BadDigest"}));
        }
        photo.stored.insert(ObjectKind::Original);
        (200, Value::Null)
    }

    fn handle(&self, route: Route, path: &str, body: &[u8]) -> (u16, Value) {
        let mut state = self.lock();
        let id = photo_id(path).to_owned();
        match route {
            Route::Begin => {
                let request: Value = serde_json::from_slice(body).unwrap();
                let id = request["photoId"].as_str().unwrap().to_owned();
                let manifest = request["manifest"].clone();
                let photo = state.photos.entry(id.clone()).or_insert_with(|| Photo {
                    manifest: manifest.clone(),
                    stored: HashSet::new(),
                    stage: Stage::Uploading,
                });
                if photo.manifest != manifest {
                    return (409, json!({"_tag": "PhotoManifestConflict", "photoId": id}));
                }
                (200, view(&id, photo))
            }
            Route::Get => match state.photos.get(&id) {
                Some(photo) => (200, view(&id, photo)),
                None => (404, json!({"_tag": "PhotoNotFound", "photoId": id})),
            },
            Route::Put(kind) => {
                let Some(photo) = state.photos.get_mut(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                if photo.stage != Stage::Uploading {
                    return (409, json!({"_tag": "PhotoAlreadyDraft", "photoId": id}));
                }
                let facts = &photo.manifest[kind.as_str()];
                let expected = facts["bytes"].as_u64().unwrap();
                if body.len() as u64 != expected {
                    return (
                        422,
                        json!({"_tag": "LengthMismatch", "expected": expected, "kind": kind, "received": body.len()}),
                    );
                }
                if hex(&Sha256::digest(body)) != facts["sha256"].as_str().unwrap() {
                    return (422, json!({"_tag": "HashMismatch", "kind": kind}));
                }
                photo.stored.insert(kind);
                (204, Value::Null)
            }
            Route::OriginalUpload => {
                let Some(photo) = state.photos.get(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                if photo.stage != Stage::Uploading {
                    return (409, json!({"_tag": "PhotoAlreadyDraft", "photoId": id}));
                }
                let facts = &photo.manifest["original"];
                (
                    200,
                    json!({
                        "expiresAt": FINALIZED_AT,
                        "headers": {
                            "content-length": facts["bytes"].as_u64().unwrap().to_string(),
                            "x-amz-checksum-sha256": facts["sha256"],
                        },
                        "method": "PUT",
                        "url": format!("{}/r2/photos/{id}/original", self.address),
                    }),
                )
            }
            Route::Finalize => {
                let Some(photo) = state.photos.get_mut(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                if photo.stage != Stage::Uploading {
                    // Already past upload: the Worker answers the photo's real view.
                    return (200, view(&id, photo));
                }
                let missing = missing(photo);
                if !missing.is_empty() {
                    let problems: Vec<Value> = missing
                        .iter()
                        .map(|kind| json!({"_tag": "Missing", "kind": kind}))
                        .collect();
                    return (
                        422,
                        json!({"_tag": "FinalizeRejected", "photoId": id, "problems": problems}),
                    );
                }
                photo.stage = Stage::Draft;
                (200, view(&id, photo))
            }
            Route::Publish => {
                let Some(photo) = state.photos.get_mut(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                if photo.stage == Stage::Uploading {
                    return (409, json!({"_tag": "PhotoNotDraft", "photoId": id}));
                }
                let details: Value = serde_json::from_slice(body).unwrap_or_else(|_| json!({}));
                photo.stage = Stage::Published { details };
                (200, view(&id, photo))
            }
            Route::Unpublish => {
                let Some(photo) = state.photos.get_mut(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                if photo.stage == Stage::Uploading {
                    return (409, json!({"_tag": "PhotoNotDraft", "photoId": id}));
                }
                photo.stage = Stage::Draft;
                (200, view(&id, photo))
            }
            Route::List => list(&state.photos, path),
        }
    }
}

/// The photo identity in a path such as `/v1/photos/<id>/publish`.
fn photo_id(path: &str) -> &str {
    let path = path.split('?').next().unwrap_or_default();
    path.split('/').nth(3).unwrap_or_default()
}

/// One page of `list`, in identity order, after `after`, at most `limit` long.
fn list(photos: &HashMap<String, Photo>, path: &str) -> (u16, Value) {
    let query = path.split_once('?').map_or("", |(_, query)| query);
    let parameter = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .map(str::to_owned)
    };
    let after = parameter("after");
    let limit: usize = parameter("limit")
        .and_then(|limit| limit.parse().ok())
        .unwrap_or(100);
    let mut ids: Vec<&String> = photos
        .keys()
        .filter(|id| {
            after
                .as_ref()
                .is_none_or(|after| id.as_str() > after.as_str())
        })
        .collect();
    ids.sort();
    let next = (ids.len() > limit).then(|| ids[limit - 1].clone());
    let page: Vec<Value> = ids
        .into_iter()
        .take(limit)
        .map(|id| view(id, &photos[id]))
        .collect();
    (200, json!({"next": next, "photos": page}))
}

fn missing(photo: &Photo) -> Vec<ObjectKind> {
    ObjectKind::ALL
        .into_iter()
        .filter(|kind| !photo.stored.contains(kind))
        .collect()
}

const FINALIZED_AT: &str = "2026-11-12T00:00:00.000Z";

fn view(id: &str, photo: &Photo) -> Value {
    let manifest = &photo.manifest;
    match &photo.stage {
        Stage::Uploading => {
            json!({"_tag": "Uploading", "manifest": manifest, "missing": missing(photo), "photoId": id})
        }
        Stage::Draft => {
            json!({"_tag": "Draft", "finalizedAt": FINALIZED_AT, "manifest": manifest, "photoId": id})
        }
        Stage::Published { details } => json!({
            "_tag": "Published", "details": details, "finalizedAt": FINALIZED_AT,
            "manifest": manifest, "photoId": id, "publishedAt": FINALIZED_AT,
        }),
        Stage::Unpublishing { details } => json!({
            "_tag": "Unpublishing", "details": details, "finalizedAt": FINALIZED_AT,
            "manifest": manifest, "photoId": id,
        }),
    }
}

fn route(method: &str, path: &str) -> Option<Route> {
    let path = path.split('?').next().unwrap_or_default();
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    Some(match (method, parts.as_slice()) {
        ("POST", ["v1", "photos"]) => Route::Begin,
        ("GET", ["v1", "photos", _]) => Route::Get,
        ("POST", ["v1", "photos", _, "finalize"]) => Route::Finalize,
        ("POST", ["v1", "photos", _, "original", "upload-url"]) => Route::OriginalUpload,
        ("PUT", ["r2", "photos", _, "original"]) => Route::Put(ObjectKind::Original),
        // The desktop sends the original straight to storage, never through the API.
        ("PUT", ["v1", "photos", _, "original"]) => return None,
        ("POST", ["v1", "photos", _, "publish"]) => Route::Publish,
        ("POST", ["v1", "photos", _, "unpublish"]) => Route::Unpublish,
        ("GET", ["v1", "photos"]) => Route::List,
        ("PUT", ["v1", "photos", _, kind]) => Route::Put(
            ObjectKind::ALL
                .into_iter()
                .find(|known| known.as_str() == *kind)?,
        ),
        _ => return None,
    })
}

type Request = (String, String, HashMap<String, String>, Vec<u8>);

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    let length = headers
        .get("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some((method, path, headers, body))
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) {
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

/// An S3 error, as storage answers a refused presigned PUT.
fn respond_xml(stream: &mut TcpStream, status: u16, code: &str) {
    let body =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>{code}</Code></Error>");
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

pub fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn unavailable() -> Action {
    Action::Status(
        503,
        json!({"_tag": "StorageUnavailable", "operation": "test"}),
    )
}
