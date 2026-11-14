//! The upload and publish workflow, driven through the library the way the window drives it:
//! open a library against the fake API, import generated JPEGs, and wait for cloud states.

use super::{FakeApi, Route, TOKEN};
use image::{Rgb, RgbImage};
use photo_prototype::{
    Event, ImportOutcome, Library, LibraryOptions, PhotoId, UploadRetry,
    api::{Config, Token, TokenSource},
    cloud::{CloudFailure, CloudState},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The client sends a request once more by itself when the connection drops before an answer,
/// so a test loses this many answers in a row to reach the app's own recovery.
pub const LOST_BY_THE_CLIENT: usize = 2;

pub fn quick_retry(attempts: u32) -> UploadRetry {
    UploadRetry {
        attempts,
        first_wait: Duration::from_millis(5),
        longest_wait: Duration::from_millis(20),
    }
}

pub fn connected(root: &Path, api: &FakeApi, token: &str, retry: UploadRetry) -> Library {
    Library::open(
        LibraryOptions::new(root)
            .cloud(Ok(Config {
                address: api.address().to_owned(),
                token: Token::new(token),
            }))
            .upload_retry(retry),
    )
    .unwrap()
}

/// Where a test keeps the owner token a Retry reads again, as the person would update
/// `PHOTO_API_TOKEN` or the Keychain item.
#[derive(Clone)]
pub struct Keychain(Arc<Mutex<String>>);

impl Keychain {
    pub fn new(token: &str) -> Self {
        Self(Arc::new(Mutex::new(token.to_owned())))
    }

    pub fn set(&self, token: &str) {
        *self.0.lock().unwrap() = token.to_owned();
    }

    fn source(&self) -> TokenSource {
        let keychain = self.0.clone();
        Arc::new(move || Some(Token::new(keychain.lock().unwrap().clone())))
    }
}

/// A library that opens with the token `keychain` holds now, and reads it again on a Retry.
pub fn with_keychain(root: &Path, api: &FakeApi, keychain: &Keychain) -> Library {
    let token = keychain.0.lock().unwrap().clone();
    Library::open(
        LibraryOptions::new(root)
            .cloud(Ok(Config {
                address: api.address().to_owned(),
                token: Token::new(token),
            }))
            .token_source(keychain.source())
            .upload_retry(quick_retry(4)),
    )
    .unwrap()
}

pub fn open(root: &Path, api: &FakeApi) -> Library {
    connected(root, api, TOKEN, quick_retry(4))
}

pub fn photo(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let image = RgbImage::from_fn(900, 600, |x, y| {
        Rgb([
            (x % 251) as u8 ^ seed,
            (y % 241) as u8,
            seed.wrapping_mul(7),
        ])
    });
    image.save(path).unwrap();
}

/// Imports `sources` and returns their new identities, in order.
pub fn import(library: &Library, sources: &[PathBuf]) -> Vec<PhotoId> {
    library.import(sources.to_vec());
    let mut ids = Vec::new();
    loop {
        match library.recv_timeout(TIMEOUT).expect("import stalled") {
            Event::Imported {
                outcome: ImportOutcome::Added(photo),
                ..
            } => ids.push(photo.id),
            Event::Imported { outcome, .. } => panic!("unexpected import outcome {outcome:?}"),
            Event::ImportFinished { .. } => return ids,
            _ => {}
        }
    }
}

/// Waits until `id` reaches a state `done` accepts, and returns every state on the way.
pub fn wait_for(
    library: &Library,
    id: &PhotoId,
    done: impl Fn(&CloudState) -> bool,
) -> Vec<CloudState> {
    let deadline = Instant::now() + TIMEOUT;
    let mut seen = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match library.recv_timeout(remaining) {
            Ok(Event::CloudChanged { id: changed, state }) if &changed == id => {
                let finished = done(&state);
                seen.push(state);
                if finished {
                    return seen;
                }
            }
            Ok(_) => {}
            Err(_) => panic!("upload stalled; states so far: {seen:?}"),
        }
    }
}

pub fn is_draft(state: &CloudState) -> bool {
    matches!(state, CloudState::Draft { .. })
}

pub fn is_settled(state: &CloudState) -> bool {
    matches!(state, CloudState::Draft { .. } | CloudState::Failed { .. })
}

pub fn failure(state: &CloudState) -> &CloudFailure {
    match state {
        CloudState::Failed { failure, .. } => failure,
        other => panic!("expected a failure, got {other:?}"),
    }
}

pub fn stored_state(root: &Path, api: &FakeApi, id: &PhotoId) -> CloudState {
    let library = Library::open(LibraryOptions::new(root)).unwrap();
    let _ = api;
    library
        .photos()
        .iter()
        .find(|photo| &photo.id == id)
        .map(|photo| photo.cloud.clone())
        .expect("photo is in the catalog")
}

pub struct Setup {
    pub _temp: tempfile::TempDir,
    pub root: PathBuf,
    pub sources: Vec<PathBuf>,
}

pub fn setup(count: u8) -> Setup {
    let temp = tempfile::tempdir().unwrap();
    let sources = (0..count)
        .map(|index| {
            let path = temp.path().join(format!("photos/DSCF{index:04}.jpg"));
            photo(&path, index * 40 + 1);
            path
        })
        .collect();
    Setup {
        root: temp.path().join("library"),
        _temp: temp,
        sources,
    }
}

pub fn is_published(state: &CloudState) -> bool {
    matches!(state, CloudState::Published { .. })
}

pub fn is_finished(state: &CloudState) -> bool {
    is_settled(state) || is_published(state)
}

/// Waits until the API holds a call, so the test can change things while it waits.
pub fn until_held(api: &FakeApi) {
    let deadline = Instant::now() + TIMEOUT;
    while !api.holding() {
        assert!(Instant::now() < deadline, "the held call never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Waits until the API has answered `route` at least once, as a held call does once released.
pub fn until_called(api: &FakeApi, route: Route) {
    let deadline = Instant::now() + TIMEOUT;
    while api.count(route) == 0 {
        assert!(Instant::now() < deadline, "no call to {route:?} finished");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A JPEG with the EXIF a publish sends: a capture time and a Fujifilm film simulation
/// (Classic Chrome).
pub fn film_photo(path: &Path, seed: u8) {
    use exif::{Field, In, Value, experimental::Writer};
    use image::{ImageEncoder, codecs::jpeg::JpegEncoder};
    let field = |tag, value| Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    };
    let mut note = b"FUJIFILM".to_vec();
    note.extend_from_slice(&12_u32.to_le_bytes());
    note.extend_from_slice(&1_u16.to_le_bytes());
    note.extend_from_slice(&0x1401_u16.to_le_bytes());
    note.extend_from_slice(&3_u16.to_le_bytes());
    note.extend_from_slice(&1_u32.to_le_bytes());
    note.extend_from_slice(&0x600_u16.to_le_bytes());
    note.extend_from_slice(&[0; 6]);
    let mut writer = Writer::new();
    let fields = [
        field(exif::Tag::MakerNote, Value::Undefined(note, 0)),
        field(
            exif::Tag::DateTimeOriginal,
            Value::Ascii(vec![b"2025:09:12 18:04:31".to_vec()]),
        ),
    ];
    for field in &fields {
        writer.push_field(field);
    }
    let mut exif = std::io::Cursor::new(Vec::new());
    writer.write(&mut exif, false).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let image = RgbImage::from_fn(900, 600, |x, y| {
        Rgb([
            (x % 251) as u8 ^ seed,
            (y % 241) as u8,
            seed.wrapping_mul(7),
        ])
    });
    let mut encoder = JpegEncoder::new_with_quality(std::fs::File::create(path).unwrap(), 90);
    encoder.set_exif_metadata(exif.into_inner()).unwrap();
    encoder
        .write_image(image.as_raw(), 900, 600, image::ExtendedColorType::Rgb8)
        .unwrap();
}

/// Waits until every photo in `ids` reaches a state `done` accepts, and returns the last
/// state of each, in order.
pub fn wait_all(
    library: &Library,
    ids: &[PhotoId],
    done: impl Fn(&CloudState) -> bool,
) -> Vec<CloudState> {
    let deadline = Instant::now() + TIMEOUT;
    let mut last: Vec<Option<CloudState>> = vec![None; ids.len()];
    while !last.iter().all(|state| state.as_ref().is_some_and(&done)) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match library.recv_timeout(remaining) {
            Ok(Event::CloudChanged { id, state }) => {
                if let Some(index) = ids.iter().position(|known| *known == id) {
                    last[index] = Some(state);
                }
            }
            Ok(_) => {}
            Err(_) => panic!("stalled; last states: {last:?}"),
        }
    }
    last.into_iter().map(Option::unwrap).collect()
}

/// Film photos, one per source, imported and uploaded as drafts, with their cloud identities.
pub fn drafts(library: &Library, setup: &Setup) -> Vec<(PhotoId, String)> {
    for (index, source) in setup.sources.iter().enumerate() {
        film_photo(source, u8::try_from(index).unwrap() * 40 + 3);
    }
    let ids = import(library, &setup.sources);
    library.upload(ids.clone());
    let states = wait_all(library, &ids, is_settled);
    ids.into_iter()
        .zip(states)
        .map(|(id, state)| {
            let CloudState::Draft { cloud_id } = state else {
                panic!("expected a draft: {state:?}");
            };
            (id, cloud_id.as_str().to_owned())
        })
        .collect()
}
