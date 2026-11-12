//! Imports JPEGs into a library, uploads every photo through the app's upload path, then asks
//! the photo API itself, with its own client, whether each one is a draft of the declared
//! files. Then it publishes every photo through the app, reads the public gallery manifest with
//! no token and finds each one there (the first with the description the app gave it),
//! unpublishes them all, and reads the manifest again to find none. Reads PHOTO_API_URL and
//! PHOTO_API_TOKEN (or the Keychain item). Never point it at production.

use anyhow::{Context, Result};
use photo_prototype::{
    Event, ImportOutcome, Library, LibraryOptions,
    api::{Config, PhotoView},
    cloud::CloudState,
};
use photo_upload_client::{Client, Gallery};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(300);
const DESCRIPTION: &str = "A generated test image of grey noise";

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .context("Usage: upload-check <library-directory> <JPEG-or-folder>...")?,
    );
    let inputs: Vec<PathBuf> = args.map(PathBuf::from).collect();
    anyhow::ensure!(!inputs.is_empty(), "Provide JPEGs or a folder");
    let config = Config::from_environment().map_err(anyhow::Error::msg)?;
    let start = Instant::now();
    let library = Library::open(LibraryOptions::new(root).cloud(Ok(config.clone())))?;

    library.import(inputs);
    loop {
        match library.recv_timeout(EVENT_TIMEOUT)? {
            Event::Imported {
                source,
                outcome: ImportOutcome::Failed(message),
                ..
            } => anyhow::bail!("{} didn't import: {message}", source.display()),
            Event::ImportFinished { .. } => break,
            _ => {}
        }
    }
    let photos = library.current_photos()?;
    anyhow::ensure!(!photos.is_empty(), "No JPEGs found");

    let mut states: HashMap<_, _> = photos
        .iter()
        .map(|photo| (photo.id.clone(), photo.cloud.clone()))
        .collect();
    library.upload(photos.iter().map(|photo| photo.id.clone()).collect());
    let mut transitions = 0;
    while states
        .values()
        .any(|state| matches!(state, CloudState::Local) || state.is_pending())
    {
        if let Event::CloudChanged { id, state } = library.recv_timeout(EVENT_TIMEOUT)? {
            transitions += 1;
            states.insert(id, state);
        }
    }
    let upload_seconds = start.elapsed().as_secs_f64();

    let token = std::env::var("PHOTO_API_TOKEN")
        .context("Set PHOTO_API_TOKEN, so the check can ask the API itself")?;
    let client = Client::new(&config.address, &token)?;
    let mut drafts = 0;
    for photo in &photos {
        let state = &states[&photo.id];
        let CloudState::Draft { cloud_id } = state else {
            anyhow::bail!("{} ended as {state:?}", photo.file_name());
        };
        match client.get(cloud_id.as_str())? {
            PhotoView::Draft { manifest, .. } => {
                anyhow::ensure!(
                    manifest.original.bytes == photo.byte_size,
                    "{}: the draft's original has another size",
                    photo.file_name()
                );
                drafts += 1;
            }
            other => {
                anyhow::bail!("{}: the Worker reports {other:?}", photo.file_name())
            }
        }
    }
    let reopened = Library::open(LibraryOptions::new(library.root()))?;
    let persisted = reopened
        .photos()
        .iter()
        .filter(|photo| matches!(photo.cloud, CloudState::Draft { .. }))
        .count();
    drop(reopened);
    anyhow::ensure!(persisted == photos.len(), "The catalog lost a draft");

    let ids: Vec<_> = photos.iter().map(|photo| photo.id.clone()).collect();
    library.describe(ids[0].clone(), DESCRIPTION.into());
    loop {
        if let Event::MetadataUpdated { photo } = library.recv_timeout(EVENT_TIMEOUT)?
            && photo.id == ids[0]
        {
            break;
        }
    }
    let publish_start = Instant::now();
    library.publish(ids.clone());
    transitions += settle(&library, &mut states, |state| {
        matches!(state, CloudState::Published { .. })
    })?;
    let publish_seconds = publish_start.elapsed().as_secs_f64();
    let gallery = Gallery::new(&format!(
        "{}/gallery/",
        config.address.trim_end_matches('/')
    ))?;
    let published = gallery.manifest()?;
    for (index, photo) in photos.iter().enumerate() {
        let cloud_id = states[&photo.id]
            .cloud_id()
            .context("a published photo has an id")?;
        let listed = published
            .photos
            .iter()
            .find(|listed| listed.id == cloud_id.as_str())
            .with_context(|| format!("{} isn't in the public gallery", photo.file_name()))?;
        if index == 0 {
            anyhow::ensure!(
                listed.alt.as_deref() == Some(DESCRIPTION),
                "the gallery's alt text is {:?}",
                listed.alt
            );
        }
    }

    let unpublish_start = Instant::now();
    library.unpublish(ids);
    transitions += settle(&library, &mut states, |state| {
        matches!(state, CloudState::Draft { .. })
    })?;
    let unpublish_seconds = unpublish_start.elapsed().as_secs_f64();
    let after = gallery.manifest()?;
    let still_public = photos
        .iter()
        .filter(|photo| {
            states[&photo.id].cloud_id().is_some_and(|cloud_id| {
                after
                    .photos
                    .iter()
                    .any(|listed| listed.id == cloud_id.as_str())
            })
        })
        .count();
    println!(
        "{}",
        serde_json::json!({
            "photos": photos.len(),
            "worker_drafts": drafts,
            "catalog_drafts_after_reopen": persisted,
            "gallery_after_publish": published.photos.len(),
            "alt_in_gallery": DESCRIPTION,
            "gallery_after_unpublish": after.photos.len(),
            "state_changes": transitions,
            "upload_seconds": upload_seconds,
            "publish_seconds": publish_seconds,
            "unpublish_seconds": unpublish_seconds,
        })
    );
    anyhow::ensure!(still_public == 0, "{still_public} photos are still public");
    Ok(())
}

/// Waits until every photo reaches a state `done` accepts, and fails on any failure. Returns
/// how many state changes it saw.
fn settle(
    library: &Library,
    states: &mut HashMap<photo_prototype::PhotoId, CloudState>,
    done: impl Fn(&CloudState) -> bool,
) -> Result<usize> {
    let mut transitions = 0;
    while !states.values().all(&done) {
        if let Event::CloudChanged { id, state } = library.recv_timeout(EVENT_TIMEOUT)? {
            transitions += 1;
            if let CloudState::Failed { failure, step, .. } = &state {
                anyhow::bail!("{step:?} failed: {}", failure.message());
            }
            states.insert(id, state);
        }
    }
    Ok(transitions)
}
