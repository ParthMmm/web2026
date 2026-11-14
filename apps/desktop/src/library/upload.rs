//! Uploading photos as private drafts. The background lane marks a photo queued, then
//! prepares it: it hashes the original with SHA-256 and copies the three web sizes into the
//! photo's upload folder, so the bytes the manifest declares never change under it. The
//! transfer lanes (`transfer.rs`) then ask the server what it still misses, stream each missing
//! object from disk, and finalize. Every call is safe to repeat, so a lost answer costs one more call, never
//! a second upload. The catalog holds each state before the call that follows it, and the state
//! an upload ends in is the one the server reports: a photo another run published stays
//! published.

use super::{
    Event, Shared, ensure_derivative, ensure_unchanged,
    transfer::{CHANGING_ANSWER, Claim, Transfer, record, report},
};
use crate::{
    api::{Api, CallError, Derivative, Manifest, ObjectKind, Original, PhotoView},
    catalog::{PhotoId, SourceStamp},
    cloud::{CloudFailure, CloudId, CloudState, CloudStep},
    derivative::DerivativeSize,
};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::SyncSender,
};

/// The web sizes the API takes, and the derivative each one is.
const RENDITIONS: [(ObjectKind, DerivativeSize); 3] = [
    (ObjectKind::W480, DerivativeSize::Grid),
    (ObjectKind::W1200, DerivativeSize::Mobile),
    (ObjectKind::W2400, DerivativeSize::Desktop),
];

/// Hashes the original and stages the web sizes, then hands the photo to a transfer lane.
pub(super) fn prepare(shared: &Shared, id: &PhotoId) -> Vec<Event> {
    let claim = Claim::take(shared, id);
    if !matches!(shared.catalog().cloud_state(id), Ok(CloudState::Queued)) {
        return Vec::new();
    }
    let failure = match prepared_manifest(shared, id) {
        Ok(manifest) => {
            let state = CloudState::Uploading {
                cloud_id: CloudId::from_hex(&manifest.original.sha256),
                missing: ObjectKind::ALL.to_vec(),
            };
            let mut queue = shared.queue();
            if !claim.holds(&queue) {
                return Vec::new();
            }
            let prepared = shared.catalog().set_prepared(id, &manifest, &state);
            match prepared {
                Ok(()) => {
                    queue.transfers.push(Transfer::now(id.clone()));
                    drop(queue);
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
    let failed = CloudState::Failed {
        cloud_id: None,
        failure,
        step: CloudStep::Upload,
    };
    record(shared, &claim, failed).into_iter().collect()
}

/// The manifest this photo's upload declares. A retry keeps the earlier manifest while its
/// staged copies still match it, so the server sees the same upload again.
fn prepared_manifest(shared: &Shared, id: &PhotoId) -> Result<Manifest, CloudFailure> {
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

fn renditions_failed(error: &anyhow::Error) -> CloudFailure {
    CloudFailure::Renditions {
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
fn hash_original(shared: &Shared, id: &PhotoId) -> Result<(String, u64), CloudFailure> {
    let paths = shared
        .catalog()
        .photo_source_paths(id)
        .map_err(|_| CloudFailure::OriginalMissing)?;
    let present: Vec<PathBuf> = paths.into_iter().filter(|path| path.is_file()).collect();
    if present.is_empty() {
        return Err(CloudFailure::OriginalMissing);
    }
    present
        .iter()
        .find_map(|path| {
            hash_both(path)
                .ok()
                .filter(|(blake3, ..)| blake3 == id)
                .map(|(_, sha256, bytes)| (sha256, bytes))
        })
        .ok_or(CloudFailure::OriginalChanged)
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
) -> Result<PathBuf, CloudFailure> {
    if kind != ObjectKind::Original {
        return staged_copy(&shared.upload_dir(id), kind, manifest);
    }
    let paths = shared
        .catalog()
        .photo_source_paths(id)
        .map_err(|_| CloudFailure::OriginalMissing)?;
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
        CloudFailure::OriginalChanged
    } else {
        CloudFailure::OriginalMissing
    })
}

fn staged_copy(
    folder: &Path,
    kind: ObjectKind,
    manifest: &Manifest,
) -> Result<PathBuf, CloudFailure> {
    let (_, sha256) = manifest.facts(kind);
    let path = staged_path(folder, kind);
    sha256_file(&path)
        .ok()
        .filter(|(found, _)| found == sha256)
        .map(|_| path)
        .ok_or_else(|| CloudFailure::Renditions {
            message: format!("the prepared {} copy is gone or changed", kind.as_str()),
        })
}

/// Sends a prepared upload and returns the state the server reports once it is done. Returns
/// `None` when the photo was never prepared, after handing it back to be prepared.
pub(super) fn send_prepared(
    shared: &Shared,
    api: &Api,
    sender: &SyncSender<Event>,
    claim: &Claim,
) -> Option<Result<CloudState, CallError>> {
    let id = &claim.id;
    let Ok(Some(manifest)) = shared.catalog().manifest(id) else {
        if let Some(event) = record(shared, claim, CloudState::Queued) {
            let _ = sender.send(event);
            shared.queue().prepares.push_back(id.clone());
            shared.wake.notify_all();
        }
        return None;
    };
    let cloud_id = CloudId::from_hex(&manifest.original.sha256);
    let result =
        send(shared, api, sender, claim, &cloud_id, &manifest).map_err(|error| match error {
            // The server forgot an upload it had begun; begin again.
            CallError::NotFound => CallError::Transient("HTTP 404".into()),
            other => other,
        });
    // A cancelled upload's folder may already hold a newer upload's copies.
    if result.is_ok() && claim.holds(&shared.queue()) {
        let _ = fs::remove_dir_all(shared.upload_dir(id));
    }
    Some(result)
}

/// Begin, send what the server misses, finalize, and return the state the server reports once
/// the upload is done: Draft, or a later state another run reached. A write the server refuses
/// because the photo is past upload starts over from `begin`, which then reports that state.
/// A cancel or a pause stops it before the next call.
fn send(
    shared: &Shared,
    api: &Api,
    sender: &SyncSender<Event>,
    claim: &Claim,
    cloud_id: &CloudId,
    manifest: &Manifest,
) -> Result<CloudState, CallError> {
    let id = &claim.id;
    'round: for _ in 0..2 {
        claim.may_call(shared)?;
        let missing = match api.begin(manifest)? {
            PhotoView::Uploading { missing, .. } => missing,
            done => return Ok(CloudState::reported(&done)),
        };
        let mut left = missing.clone();
        report(sender, record(shared, claim, uploading(cloud_id, &left)));
        for kind in missing {
            claim.may_call(shared)?;
            let path = object_path(shared, id, kind, manifest).map_err(CallError::Stop)?;
            match api.put_file(cloud_id, kind, &path) {
                Ok(()) => {}
                Err(CallError::AlreadyDraft) => continue 'round,
                Err(
                    error @ (CallError::File(_) | CallError::Stop(CloudFailure::Rejected { .. })),
                ) => {
                    // The file may have gone or changed while it streamed. If so, say that: the
                    // person can act on it, and a server refusal would only say to report it.
                    object_path(shared, id, kind, manifest).map_err(CallError::Stop)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
            left.retain(|other| *other != kind);
            report(sender, record(shared, claim, uploading(cloud_id, &left)));
        }
        claim.may_call(shared)?;
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
