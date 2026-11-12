//! Uploading photos as private drafts. The background lane marks a photo queued, then
//! prepares it: it hashes the original with SHA-256 and copies the three web sizes into the
//! photo's upload folder, so the bytes the manifest declares never change under it. The
//! transfer lanes then ask the server what it still misses, stream each missing object from
//! disk, and finalize. Every call is safe to repeat, so a lost answer costs one more call, never
//! a second upload. The catalog holds each state before the call that follows it, and the state
//! an upload ends in is the one the server reports: a photo another run published stays
//! published.

use super::{Event, Shared, ensure_derivative, ensure_unchanged};
use crate::{
    api::{Api, CallError, Derivative, Manifest, ObjectKind, Original, PhotoView},
    catalog::{PhotoId, SourceStamp},
    cloud::{CloudId, CloudState, UploadFailure},
    derivative::DerivativeSize,
};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};

const CHANGING_ANSWER: &str = "the server kept changing its answer";

/// At most this many uploads send at once. Each streams one object from disk at a time.
pub(super) const TRANSFER_LANES: usize = 2;

/// The web sizes the API takes, and the derivative each one is.
const RENDITIONS: [(ObjectKind, DerivativeSize); 3] = [
    (ObjectKind::W480, DerivativeSize::Grid),
    (ObjectKind::W1200, DerivativeSize::Mobile),
    (ObjectKind::W2400, DerivativeSize::Desktop),
];

/// How a transfer that failed for a reason that may pass is tried again: up to `attempts`
/// times in all, waiting `first_wait`, then twice as long each time, never longer than
/// `longest_wait`.
#[derive(Clone, Copy, Debug)]
pub struct UploadRetry {
    pub attempts: u32,
    pub first_wait: Duration,
    pub longest_wait: Duration,
}

impl Default for UploadRetry {
    fn default() -> Self {
        Self {
            attempts: 5,
            first_wait: Duration::from_secs(2),
            longest_wait: Duration::from_secs(60),
        }
    }
}

impl UploadRetry {
    fn wait(&self, attempt: u32) -> Duration {
        self.first_wait
            .saturating_mul(1 << attempt.min(16))
            .min(self.longest_wait)
    }
}

/// A prepared photo waiting for a transfer lane.
pub(super) struct Transfer {
    id: PhotoId,
    due: Instant,
    /// Tries that already failed for a reason that may pass.
    failed_tries: u32,
}

impl Transfer {
    pub(super) fn now(id: PhotoId) -> Self {
        Self {
            id,
            due: Instant::now(),
            failed_tries: 0,
        }
    }
}

/// Marks each photo that can upload as queued. A photo already queued, uploading or in the
/// cloud is skipped, so asking twice uploads once.
pub(super) fn queue(shared: &Shared, ids: Vec<PhotoId>) -> Vec<Event> {
    let mut events = Vec::new();
    for id in ids {
        let mut catalog = shared.catalog();
        let eligible = catalog.contains_photo(&id).unwrap_or(false)
            && catalog
                .cloud_state(&id)
                .is_ok_and(|state| state.can_upload());
        if eligible && catalog.set_cloud_state(&id, &CloudState::Queued).is_ok() {
            drop(catalog);
            shared.queue().prepares.push_back(id.clone());
            events.push(Event::CloudChanged {
                id,
                state: CloudState::Queued,
            });
        }
    }
    events
}

/// Hashes the original and stages the web sizes, then hands the photo to a transfer lane.
pub(super) fn prepare(shared: &Shared, id: &PhotoId) -> Vec<Event> {
    if !matches!(shared.catalog().cloud_state(id), Ok(CloudState::Queued)) {
        return Vec::new();
    }
    let failure = match prepared_manifest(shared, id) {
        Ok(manifest) => {
            let state = CloudState::Uploading {
                cloud_id: CloudId::from_hex(&manifest.original.sha256),
                missing: ObjectKind::ALL.to_vec(),
            };
            match shared.catalog().set_prepared(id, &manifest, &state) {
                Ok(()) => {
                    shared.queue().transfers.push(Transfer::now(id.clone()));
                    shared.wake.notify_all();
                    return vec![Event::CloudChanged {
                        id: id.clone(),
                        state,
                    }];
                }
                Err(error) => renditions_failed(&error),
            }
        }
        Err(failure) => failure,
    };
    vec![record(
        shared,
        id,
        CloudState::Failed {
            cloud_id: None,
            failure,
        },
    )]
}

/// The manifest this photo's upload declares. A retry keeps the earlier manifest while its
/// staged copies still match it, so the server sees the same upload again.
fn prepared_manifest(shared: &Shared, id: &PhotoId) -> Result<Manifest, UploadFailure> {
    let folder = shared.upload_dir(id);
    let (original_sha256, original_bytes) = hash_original(shared, id)?;
    if let Ok(Some(manifest)) = shared.catalog().manifest(id)
        && manifest.original.sha256 == original_sha256
        && RENDITIONS
            .iter()
            .all(|&(kind, _)| staged_copy(&folder, kind, &manifest).is_ok())
    {
        return Ok(manifest);
    }
    fs::create_dir_all(&folder).map_err(|error| renditions_failed(&error.into()))?;
    let mut staged = Vec::new();
    for (kind, size) in RENDITIONS {
        let (rendered, _) =
            ensure_derivative(shared, id, size).map_err(|error| renditions_failed(&error))?;
        staged.push(
            stage(&rendered, &staged_path(&folder, kind))
                .map_err(|error| renditions_failed(&error))?,
        );
    }
    let [w480, w1200, w2400]: [Derivative; 3] = staged
        .try_into()
        .map_err(|_| renditions_failed(&anyhow::anyhow!("a web size is missing")))?;
    Ok(Manifest {
        original: Original {
            bytes: original_bytes,
            sha256: original_sha256,
        },
        w480,
        w1200,
        w2400,
    })
}

fn renditions_failed(error: &anyhow::Error) -> UploadFailure {
    UploadFailure::Renditions {
        message: format!("{error:#}"),
    }
}

fn staged_path(folder: &Path, kind: ObjectKind) -> PathBuf {
    folder.join(format!("{}.webp", kind.as_str()))
}

/// Copies a derivative into the upload folder and describes the copy.
fn stage(rendered: &Path, staged: &Path) -> anyhow::Result<Derivative> {
    let temporary = staged.with_extension("tmp");
    fs::copy(rendered, &temporary)?;
    fs::rename(&temporary, staged)?;
    let (sha256, bytes) = sha256_file(staged)?;
    let (width, height) = image::image_dimensions(staged).context("unreadable web size")?;
    Ok(Derivative {
        bytes,
        height,
        sha256,
        width,
    })
}

/// The original's SHA-256 and length, from a source that still holds the imported bytes. The
/// file must not change while it is read, as for any import.
fn hash_original(shared: &Shared, id: &PhotoId) -> Result<(String, u64), UploadFailure> {
    let paths = shared
        .catalog()
        .photo_source_paths(id)
        .map_err(|_| UploadFailure::OriginalMissing)?;
    let present: Vec<PathBuf> = paths.into_iter().filter(|path| path.is_file()).collect();
    if present.is_empty() {
        return Err(UploadFailure::OriginalMissing);
    }
    present
        .iter()
        .find_map(|path| {
            hash_both(path)
                .ok()
                .filter(|(blake3, ..)| blake3 == id)
                .map(|(_, sha256, bytes)| (sha256, bytes))
        })
        .ok_or(UploadFailure::OriginalChanged)
}

/// BLAKE3 and SHA-256 in one read, with the same changed-file checks as an import.
fn hash_both(path: &Path) -> anyhow::Result<(PhotoId, String, u64)> {
    let before = SourceStamp::read(path)?;
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    let mut blake3 = blake3::Hasher::new();
    let mut sha256 = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    let mut bytes = 0;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        blake3.update(&buffer[..read]);
        sha256.update(&buffer[..read]);
        bytes += read as u64;
    }
    ensure_unchanged(path, &file, &opened, before)?;
    Ok((
        PhotoId::from_hash(blake3.finalize()),
        hex(&sha256.finalize()),
        bytes,
    ))
}

fn hex(digest: &[u8]) -> String {
    CloudId::from_digest(digest).as_str().to_owned()
}

/// A file's SHA-256 and length, read in chunks so no file is held whole in memory.
fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut sha256 = Sha256::new();
    let bytes = std::io::copy(&mut file, &mut sha256)?;
    Ok((hex(&sha256.finalize()), bytes))
}

/// The file to send for `kind`, checked against the manifest first. The server checks the hash
/// again as the file streams, so a change after this check is refused, not stored.
fn object_path(
    shared: &Shared,
    id: &PhotoId,
    kind: ObjectKind,
    manifest: &Manifest,
) -> Result<PathBuf, UploadFailure> {
    if kind != ObjectKind::Original {
        return staged_copy(&shared.upload_dir(id), kind, manifest);
    }
    let paths = shared
        .catalog()
        .photo_source_paths(id)
        .map_err(|_| UploadFailure::OriginalMissing)?;
    let mut present = false;
    for path in paths {
        let Ok((sha256, _)) = sha256_file(&path) else {
            continue;
        };
        present = true;
        if sha256 == manifest.original.sha256 {
            return Ok(path);
        }
    }
    Err(if present {
        UploadFailure::OriginalChanged
    } else {
        UploadFailure::OriginalMissing
    })
}

fn staged_copy(
    folder: &Path,
    kind: ObjectKind,
    manifest: &Manifest,
) -> Result<PathBuf, UploadFailure> {
    let (_, sha256) = manifest.facts(kind);
    let path = staged_path(folder, kind);
    sha256_file(&path)
        .ok()
        .filter(|(found, _)| found == sha256)
        .map(|_| path)
        .ok_or_else(|| UploadFailure::Renditions {
            message: format!("the prepared {} copy is gone or changed", kind.as_str()),
        })
}

/// One transfer lane: takes the next due photo and uploads it, until the library closes.
pub(super) fn run_transfers(shared: &Shared, api: &Api, sender: &SyncSender<Event>) {
    while let Some(transfer) = next_transfer(shared) {
        transfer_one(shared, api, sender, transfer);
    }
}

fn next_transfer(shared: &Shared) -> Option<Transfer> {
    let mut queue = shared.queue();
    loop {
        if queue.stopped {
            return None;
        }
        let now = Instant::now();
        let next = queue
            .transfers
            .iter()
            .enumerate()
            .min_by_key(|(_, transfer)| transfer.due)
            .map(|(index, transfer)| (index, transfer.due));
        queue = match next {
            Some((index, due)) if due <= now => return Some(queue.transfers.swap_remove(index)),
            Some((_, due)) => {
                shared
                    .wake
                    .wait_timeout(queue, due - now)
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0
            }
            None => shared
                .wake
                .wait(queue)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

fn transfer_one(shared: &Shared, api: &Api, sender: &SyncSender<Event>, transfer: Transfer) {
    let id = transfer.id.clone();
    let Ok(Some(manifest)) = shared.catalog().manifest(&id) else {
        // Never prepared: prepare it again.
        let _ = sender.send(record(shared, &id, CloudState::Queued));
        shared.queue().prepares.push_back(id);
        shared.wake.notify_all();
        return;
    };
    let cloud_id = CloudId::from_hex(&manifest.original.sha256);
    let result = send(shared, api, sender, &id, &cloud_id, &manifest);
    if shared.queue().stopped {
        // The library closed mid-upload. The catalog keeps the last state, and the next
        // open asks the server what is left.
        return;
    }
    let state = match result {
        Ok(state) => {
            let _ = fs::remove_dir_all(shared.upload_dir(&id));
            state
        }
        Err(error @ (CallError::Transient(_) | CallError::File(_) | CallError::AlreadyDraft)) => {
            let message = match error {
                CallError::Transient(message) => message,
                CallError::File(message) => format!("couldn't read a file to send ({message})"),
                _ => CHANGING_ANSWER.into(),
            };
            let failed_tries = transfer.failed_tries + 1;
            if failed_tries < shared.upload_retry.attempts {
                shared.queue().transfers.push(Transfer {
                    due: Instant::now() + shared.upload_retry.wait(transfer.failed_tries),
                    id,
                    failed_tries,
                });
                shared.wake.notify_all();
                return;
            }
            CloudState::Failed {
                cloud_id: Some(cloud_id),
                failure: UploadFailure::Unavailable {
                    message: format!("{message}, {failed_tries} tries"),
                },
            }
        }
        Err(CallError::Stop(failure)) => CloudState::Failed {
            cloud_id: Some(cloud_id),
            failure,
        },
    };
    let _ = sender.send(record(shared, &id, state));
}

/// Writes the state to the catalog and returns the event that reports it. A lane that
/// outlives its library writes nothing, so it can't overwrite what a newer library recorded.
fn record(shared: &Shared, id: &PhotoId, state: CloudState) -> Event {
    if !shared.queue().stopped {
        let _ = shared.catalog().set_cloud_state(id, &state);
    }
    Event::CloudChanged {
        id: id.clone(),
        state,
    }
}

/// Begin, send what the server misses, finalize, and return the state the server reports once
/// the upload is done: Draft, or a later state another run reached. A write the server refuses
/// because the photo is past upload starts over from `begin`, which then reports that state.
fn send(
    shared: &Shared,
    api: &Api,
    sender: &SyncSender<Event>,
    id: &PhotoId,
    cloud_id: &CloudId,
    manifest: &Manifest,
) -> Result<CloudState, CallError> {
    'round: for _ in 0..2 {
        let missing = match api.begin(manifest)? {
            PhotoView::Uploading { missing, .. } => missing,
            done => return Ok(CloudState::reported(&done)),
        };
        let mut left = missing.clone();
        let _ = sender.send(record(shared, id, uploading(cloud_id, &left)));
        for kind in missing {
            if shared.queue().stopped {
                return Err(CallError::Transient("the library closed".into()));
            }
            let path = object_path(shared, id, kind, manifest).map_err(CallError::Stop)?;
            match api.put_file(cloud_id, kind, &path) {
                Ok(()) => {}
                Err(CallError::AlreadyDraft) => continue 'round,
                Err(
                    error @ (CallError::File(_) | CallError::Stop(UploadFailure::Rejected { .. })),
                ) => {
                    // The file may have gone or changed while it streamed. If so, say that: the
                    // person can act on it, and a server refusal would only say to report it.
                    object_path(shared, id, kind, manifest).map_err(CallError::Stop)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
            left.retain(|other| *other != kind);
            let _ = sender.send(record(shared, id, uploading(cloud_id, &left)));
        }
        match api.finalize(cloud_id) {
            Ok(PhotoView::Uploading { .. }) | Err(CallError::AlreadyDraft) => continue 'round,
            Ok(done) => return Ok(CloudState::reported(&done)),
            Err(error) => return Err(error),
        }
    }
    Err(CallError::Transient(CHANGING_ANSWER.into()))
}

fn uploading(cloud_id: &CloudId, missing: &[ObjectKind]) -> CloudState {
    CloudState::Uploading {
        cloud_id: cloud_id.clone(),
        missing: missing.to_vec(),
    }
}
