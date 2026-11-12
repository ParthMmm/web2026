//! Imports JPEGs into a library, uploads every photo through the app's upload path, then asks
//! the photo API itself, with its own client, whether each one is a draft of the declared
//! files. Reads PHOTO_API_URL and PHOTO_API_TOKEN (or the Keychain item). Never point it at
//! production.

use anyhow::{Context, Result};
use photo_prototype::{
    Event, ImportOutcome, Library, LibraryOptions,
    api::{Config, PhotoView},
    cloud::CloudState,
};
use photo_upload_client::Client;
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(300);

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
        .any(|state| !matches!(state, CloudState::Draft { .. } | CloudState::Failed { .. }))
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
            other @ PhotoView::Uploading { .. } => {
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
    println!(
        "{}",
        serde_json::json!({
            "photos": photos.len(),
            "worker_drafts": drafts,
            "catalog_drafts_after_reopen": persisted,
            "state_changes": transitions,
            "seconds": upload_seconds,
        })
    );
    anyhow::ensure!(persisted == photos.len(), "The catalog lost a draft");
    Ok(())
}
