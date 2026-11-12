//! A small in-process photo API for upload tests. It follows the Worker's contract for
//! `begin`, `get`, `put` and `finalize` (identity by SHA-256, checked lengths and hashes, `409`
//! after a draft), and lets a test inject faults: an HTTP status, a lost answer, a delay, or a
//! hold until the test releases it.

#![allow(dead_code)]

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
    Finalize,
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
}

struct Fault {
    route: Route,
    action: Action,
    remaining: usize,
}

struct Photo {
    manifest: Value,
    stored: HashSet<ObjectKind>,
    draft: bool,
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
            .filter(|(_, photo)| photo.draft)
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
        let authorized =
            headers.get("authorization").map(String::as_str) == Some(&format!("Bearer {TOKEN}"));
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
        match &action {
            Some(Action::Delay(wait)) => std::thread::sleep(*wait),
            Some(Action::Hold) => self.wait_for_release(),
            _ => {}
        }
        let (status, answer) = if authorized {
            self.handle(route, &path, &body)
        } else {
            (401, json!({"_tag": "Unauthorized"}))
        };
        self.lock().calls.push(route);
        if matches!(action, Some(Action::LoseAnswer)) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        }
        respond(&mut stream, status, &answer);
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

    fn handle(&self, route: Route, path: &str, body: &[u8]) -> (u16, Value) {
        let mut state = self.lock();
        let id = path.split('/').nth(3).unwrap_or_default().to_owned();
        match route {
            Route::Begin => {
                let request: Value = serde_json::from_slice(body).unwrap();
                let id = request["photoId"].as_str().unwrap().to_owned();
                let manifest = request["manifest"].clone();
                let photo = state.photos.entry(id.clone()).or_insert_with(|| Photo {
                    manifest: manifest.clone(),
                    stored: HashSet::new(),
                    draft: false,
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
                if photo.draft {
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
            Route::Finalize => {
                let Some(photo) = state.photos.get_mut(&id) else {
                    return (404, json!({"_tag": "PhotoNotFound", "photoId": id}));
                };
                let missing = missing(photo);
                if !photo.draft && !missing.is_empty() {
                    let problems: Vec<Value> = missing
                        .iter()
                        .map(|kind| json!({"_tag": "Missing", "kind": kind}))
                        .collect();
                    return (
                        422,
                        json!({"_tag": "FinalizeRejected", "photoId": id, "problems": problems}),
                    );
                }
                photo.draft = true;
                (200, view(&id, photo))
            }
        }
    }
}

fn missing(photo: &Photo) -> Vec<ObjectKind> {
    ObjectKind::ALL
        .into_iter()
        .filter(|kind| !photo.stored.contains(kind))
        .collect()
}

fn view(id: &str, photo: &Photo) -> Value {
    if photo.draft {
        json!({"_tag": "Draft", "finalizedAt": "2026-11-12T00:00:00.000Z", "manifest": photo.manifest, "photoId": id})
    } else {
        json!({"_tag": "Uploading", "manifest": photo.manifest, "missing": missing(photo), "photoId": id})
    }
}

fn route(method: &str, path: &str) -> Option<Route> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    Some(match (method, parts.as_slice()) {
        ("POST", ["v1", "photos"]) => Route::Begin,
        ("GET", ["v1", "photos", _]) => Route::Get,
        ("POST", ["v1", "photos", _, "finalize"]) => Route::Finalize,
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

pub fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn unavailable() -> Action {
    Action::Status(
        503,
        json!({"_tag": "StorageUnavailable", "operation": "test"}),
    )
}
