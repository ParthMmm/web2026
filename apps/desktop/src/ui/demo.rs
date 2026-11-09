//! Made-up photos for headless renders and the goldens, and the scenes `--scene` names. A
//! headless run never opens the library on disk, so a render never shows anyone's photos.
//! Every state the window can show has a scene: full, empty, opening, failed, importing,
//! and long names.

use super::{
    gallery::{Gallery, GalleryPhoto, LibraryState, Overlay, Thumbnail},
    images::ImageSlots,
};
use crate::{
    ImportFailure, PhotoId, PhotoRecord,
    metadata::{CaptureTime, ExposureTime, FilmSimulation, PhotoMetadata},
};
use rune::ImageCache;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// A scene: its name for `--scene`, and what it does to the demo gallery.
pub(super) type Scene = (&'static str, fn(&mut Gallery));

pub(super) static SCENES: &[Scene] = &[
    ("grid", |_| {}),
    ("opening", |s| {
        clear(s);
        s.library = LibraryState::Opening;
    }),
    ("unavailable", |s| {
        clear(s);
        let message = "Permission denied (os error 13) at ~/Library/Application Support/dev.parth.photo-prototype/library";
        s.status = format!("Couldn't open the library: {message}");
        s.library = LibraryState::Unavailable(message.into());
    }),
    ("empty", clear),
    ("scanning", |s| {
        clear(s);
        s.progress = Some((0, 0));
        s.status = "Importing…".into();
    }),
    ("importing", |s| {
        s.progress = Some((9, 40));
        s.status = "Importing…".into();
        for photo in s.photos.iter_mut().skip(18) {
            photo.thumbnail = Thumbnail::Pending;
        }
    }),
    ("attention", |s| {
        s.failures = failures();
        s.photos[2].record.source_missing = true;
        s.photos[4].thumbnail = Thumbnail::Failed;
        s.photos[5].thumbnail = Thumbnail::Pending;
        select(s, 2);
    }),
    ("viewer", |s| {
        select(s, 3);
        s.overlay = Overlay::Viewer;
    }),
    ("viewer-loading", |s| {
        select(s, 3);
        if let Some(preview) = &mut s.preview {
            preview.image = None;
        }
        s.overlay = Overlay::Viewer;
    }),
    ("failures", |s| {
        s.failures = failures();
        s.overlay = Overlay::Failures;
    }),
    ("long", |s| {
        let record = &mut s.photos[1].record;
        record.source = PathBuf::from(
            "/Users/demo/Pictures/Exports/2026/October/A weekend walking the coast from Porto down to Aveiro, mostly in the rain/DSCF4521 - the harbour wall at low tide, second attempt with the 23mm.JPG",
        );
        record.metadata.lens =
            Some("XF23mmF1.4 R LM WR with a very long adapter name for testing".into());
        record.source_count = 12;
        s.failures = failures();
        select(s, 1);
    }),
    ("performance", |s| s.toggle_performance()),
];

/// The shoots the demo library holds: a folder, a film simulation and a
/// lens each, with eight frames from each.
const SHOOTS: &[(&str, &str, &str, f64)] = &[
    (
        "2026-10 Lisbon",
        "Classic Chrome",
        "XF23mmF1.4 R LM WR",
        23.0,
    ),
    ("2026-09 Dungeness", "Acros", "XF35mmF2 R WR", 35.0),
    ("2026-08 Kyoto", "Classic Negative", "XF56mmF1.2 R WR", 56.0),
];
const FRAMES_PER_SHOOT: usize = 8;

/// The demo library, in grid order (by original path).
pub(super) fn photos() -> Vec<GalleryPhoto> {
    let mut photos: Vec<GalleryPhoto> = SHOOTS
        .iter()
        .enumerate()
        .flat_map(|(shoot, &(folder, film, lens, focal))| {
            (0..FRAMES_PER_SHOOT).map(move |frame| {
                let number = 4100 + shoot * 300 + frame * 7;
                let portrait = frame % 3 == 2;
                let (width, height) = if portrait { (4160, 6240) } else { (6240, 4160) };
                let id = PhotoId::from_hash(blake3::hash(format!("demo-{number}").as_bytes()));
                let record = PhotoRecord {
                    id: id.clone(),
                    source: PathBuf::from(format!(
                        "/Users/demo/Pictures/{folder}/DSCF{number}.JPG"
                    )),
                    source_missing: false,
                    source_count: 1,
                    width,
                    height,
                    byte_size: 9_400_000 + (number as u64 % 13) * 410_000,
                    imported_at: 0,
                    metadata: PhotoMetadata {
                        film_simulation: FilmSimulation::from_name(film),
                        iso: Some([160, 400, 800, 3200][frame % 4]),
                        exposure_time: ExposureTime::new(1, [250, 125, 60, 1000][frame % 4]),
                        f_number: Some([2.0, 2.8, 5.6, 8.0][frame % 4]),
                        focal_length_mm: Some(focal),
                        camera_make: Some("FUJIFILM".into()),
                        camera_model: Some("X-T5".into()),
                        lens: Some(lens.into()),
                        captured_at: CaptureTime::new(
                            format!("2026-{:02}-1{}T1{}:2{}:00", 10 - shoot, frame, frame, frame),
                            Some("+01:00".into()),
                        ),
                    },
                };
                GalleryPhoto::new(record, Thumbnail::Ready(path(&id, "grid")))
            })
        })
        .collect();
    photos.sort_by(|a, b| a.record.source.cmp(&b.record.source));
    photos
}

pub(super) fn preview_path(id: &PhotoId) -> Arc<Path> {
    path(id, "desktop")
}

fn path(id: &PhotoId, size: &str) -> Arc<Path> {
    PathBuf::from(format!("demo/{id}/{size}.jpg")).into()
}

/// Makes the pixels for every demo thumbnail and the preview that has
/// none yet. Runs once the gallery has its image cache.
pub(super) fn decode(gallery: &mut Gallery) {
    let Gallery {
        images: Some(cache),
        photos,
        thumbnails,
        previews,
        preview,
        ..
    } = gallery
    else {
        return;
    };
    for photo in photos.iter() {
        if let Thumbnail::Ready(path) = &photo.thumbnail {
            draw(cache, thumbnails, path, &photo.record, 240);
        }
    }
    let Some(preview) = preview else {
        return;
    };
    let (Some(path), Some(photo)) = (
        &preview.image,
        photos.iter().find(|photo| photo.record.id == preview.id),
    ) else {
        return;
    };
    draw(cache, previews, path, &photo.record, 960);
}

/// Makes `record`'s pixels under `path`, `long` points on its longer side,
/// unless `slots` already holds them.
fn draw(
    cache: &ImageCache,
    slots: &mut ImageSlots,
    path: &Arc<Path>,
    record: &PhotoRecord,
    long: u32,
) {
    if slots.pixels(path).is_some() {
        return;
    }
    let (w, h) = fit(record, long);
    let image = cache.rgba(w, h, gradient(w, h, seed(&record.id)));
    let _ = image.wait();
    slots.insert(path.clone(), image);
}

fn clear(s: &mut Gallery) {
    s.photos.clear();
    s.selected = None;
    s.preview = None;
}

fn select(s: &mut Gallery, index: usize) {
    s.selected = Some(s.photos[index].record.id.clone());
    s.request_preview();
}

pub(super) fn failures() -> Vec<ImportFailure> {
    [
        (
            "/Users/demo/Pictures/2026-10 Lisbon/DSCF4170.JPG",
            "The file ends before its image data does",
        ),
        (
            "/Users/demo/Pictures/Scans/roll-14/frame-03.tif",
            "Only JPEG files can be imported",
        ),
        (
            "/Volumes/Card/DCIM/103_FUJI/DSCF0031.JPG",
            "The disk was ejected during the import",
        ),
    ]
    .into_iter()
    .map(|(path, message)| ImportFailure {
        path: path.into(),
        message: message.into(),
        failed_at: 0,
    })
    .collect()
}

/// The longer side's length is `long`, keeping the photo's shape.
fn fit(record: &PhotoRecord, long: u32) -> (u32, u32) {
    let short = long * 2 / 3;
    if record.width >= record.height {
        (long, short)
    } else {
        (short, long)
    }
}

fn seed(id: &PhotoId) -> u32 {
    u32::from_str_radix(&id.as_str()[..4], 16).unwrap_or(0)
}

/// A soft two-colour gradient with a glow, a different pair of hues for each seed.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn gradient(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let hue = |deg: f32| {
        let h = deg.rem_euclid(360.0) / 60.0;
        let x = 1.0 - (h % 2.0 - 1.0).abs();
        let (r, g, b) = match h as u32 {
            0 => (1.0, x, 0.0),
            1 => (x, 1.0, 0.0),
            2 => (0.0, 1.0, x),
            3 => (0.0, x, 1.0),
            4 => (x, 0.0, 1.0),
            _ => (1.0, 0.0, x),
        };
        [r * 0.5 + 0.2, g * 0.5 + 0.2, b * 0.5 + 0.2]
    };
    let start = (seed % 360) as f32;
    let (a, b) = (hue(start), hue(start + 110.0));
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let t = (u * 0.35 + v * 0.65).clamp(0.0, 1.0);
            let glow =
                (1.0 - ((u - 0.6).powi(2) + (v - 0.35).powi(2)).sqrt() * 2.2).max(0.0) * 0.25;
            for c in 0..3 {
                out.push(((a[c] * (1.0 - t) + b[c] * t + glow).min(1.0) * 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}
