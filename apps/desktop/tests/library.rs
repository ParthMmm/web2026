use exif::{Field, In, Rational, Tag, Value, experimental::Writer};
use image::{ImageEncoder, Rgb, RgbImage, codecs::jpeg::JpegEncoder};
use photo_prototype::{
    DerivativeSize, Event, ImportOutcome, ImportSummary, Library, LibraryOptions, PhotoId,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(20);

fn write_jpeg(path: &Path, image: &RgbImage, exif: Option<Vec<u8>>, icc: Option<Vec<u8>>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = JpegEncoder::new_with_quality(std::fs::File::create(path).unwrap(), 92);
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).unwrap();
    }
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc).unwrap();
    }
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
}

fn solid(path: &Path, width: u32, height: u32, color: [u8; 3]) {
    write_jpeg(
        path,
        &RgbImage::from_pixel(width, height, Rgb(color)),
        None,
        None,
    );
}

fn exif_block(fields: &[Field]) -> Vec<u8> {
    let mut writer = Writer::new();
    for field in fields {
        writer.push_field(field);
    }
    let mut buffer = std::io::Cursor::new(Vec::new());
    writer.write(&mut buffer, false).unwrap();
    buffer.into_inner()
}

fn field(tag: Tag, value: Value) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    }
}

fn open(root: &Path) -> Library {
    Library::open(LibraryOptions::new(root)).unwrap()
}

fn import(library: &Library, inputs: &[PathBuf]) -> (Vec<(PathBuf, ImportOutcome)>, ImportSummary) {
    library.import(inputs.to_vec());
    let mut outcomes = Vec::new();
    loop {
        match library.recv_timeout(TIMEOUT).expect("import stalled") {
            Event::Imported {
                source, outcome, ..
            } => outcomes.push((source, outcome)),
            Event::ImportFinished { summary } => return (outcomes, summary),
            _ => {}
        }
    }
}

fn added_id(outcome: &ImportOutcome) -> PhotoId {
    match outcome {
        ImportOutcome::Added(photo) => photo.id.clone(),
        other => panic!("Expected a new photo, got {other:?}"),
    }
}

fn wait_for_desktop(library: &Library, id: &PhotoId) -> Result<PathBuf, String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match library.recv_timeout(remaining).expect("preview stalled") {
            Event::Ready {
                id: ready,
                size: DerivativeSize::Desktop,
                path,
                ..
            } if &ready == id => return Ok(path),
            Event::DerivativeFailed {
                id: failed,
                size: DerivativeSize::Desktop,
                message,
            } if &failed == id => return Err(message),
            _ => {}
        }
    }
}

fn first_pixel(path: &Path) -> [u8; 3] {
    image::open(path).unwrap().to_rgb8().get_pixel(0, 0).0
}

#[test]
fn an_imported_photo_survives_restart_without_reimporting_and_the_original_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.JPG");
    solid(&source, 1200, 800, [120, 80, 40]);
    let original = std::fs::read(&source).unwrap();
    let root = temp.path().join("library");

    let library = open(&root);
    let (outcomes, summary) = import(&library, std::slice::from_ref(&source));
    assert_eq!(summary.added, 1);
    let id = added_id(&outcomes[0].1);
    let grid = library.derivative_path(&id, DerivativeSize::Grid);
    assert_eq!(image::image_dimensions(&grid).unwrap(), (480, 320));
    assert_eq!(
        image::ImageFormat::from_path(&grid).unwrap(),
        image::ImageFormat::WebP
    );
    let grid_modified = std::fs::metadata(&grid).unwrap().modified().unwrap();
    drop(library);

    let reopened = open(&root);
    let photos = reopened.photos();
    assert_eq!(photos.len(), 1);
    assert_eq!((photos[0].width, photos[0].height), (1200, 800));
    assert_eq!(photos[0].source, source.canonicalize().unwrap());
    assert!(!photos[0].source_missing);
    let (outcomes, summary) = import(&reopened, std::slice::from_ref(&source));
    assert_eq!(summary.unchanged, 1);
    assert_eq!(outcomes[0].1, ImportOutcome::Unchanged(id));
    assert_eq!(
        std::fs::metadata(&grid).unwrap().modified().unwrap(),
        grid_modified
    );
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn nested_folders_and_overlapping_selections_import_each_source_once() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("imports");
    let sources = [
        folder.join("photo.jpg"),
        folder.join("2025/photo.JPG"),
        folder.join("2025/trip/exports/photo.JPG"),
        folder.join("2024/photo.JpEg"),
    ];
    for (ix, source) in sources.iter().enumerate() {
        solid(source, 60, 40, [ix as u8 * 40, 80, 40]);
    }
    std::fs::create_dir_all(folder.join("2025/trip/empty")).unwrap();
    std::fs::write(folder.join("2025/trip/notes.txt"), b"not a photo").unwrap();

    let library = open(&temp.path().join("library"));
    let (outcomes, summary) = import(
        &library,
        &[folder.clone(), folder.join("2025"), sources[2].clone()],
    );
    assert_eq!(summary.added, 4, "{outcomes:?}");
    assert_eq!(outcomes.len(), 4);
    let photos = library.current_photos().unwrap();
    for source in &sources {
        let source = source.canonicalize().unwrap();
        assert!(
            photos.iter().any(|photo| photo.source == source),
            "Missing {source:?}"
        );
    }
}

#[test]
fn duplicates_are_detected_by_content_not_by_file_name() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("a/photo.jpg");
    let renamed = temp.path().join("b/renamed.jpg");
    let same_name_other_content = temp.path().join("c/photo.jpg");
    solid(&original, 60, 40, [10, 30, 90]);
    std::fs::create_dir_all(renamed.parent().unwrap()).unwrap();
    std::fs::copy(&original, &renamed).unwrap();
    solid(&same_name_other_content, 60, 40, [200, 30, 90]);

    let library = open(&temp.path().join("library"));
    let (outcomes, summary) = import(
        &library,
        &[original.clone(), renamed.clone(), same_name_other_content],
    );
    assert_eq!((summary.added, summary.duplicates), (2, 1), "{outcomes:?}");
    let photos = library.current_photos().unwrap();
    assert_eq!(photos.len(), 2);
    let shared = photos
        .iter()
        .find(|photo| photo.source == original.canonicalize().unwrap())
        .unwrap();
    assert_eq!(shared.source_count, 2);
}

#[test]
fn damaged_files_fail_individually_are_remembered_and_can_be_retried() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("imports");
    solid(&folder.join("good.jpg"), 60, 40, [10, 30, 90]);
    std::fs::write(folder.join("not-a-jpeg.jpg"), b"not a JPEG").unwrap();
    let complete = temp.path().join("complete.jpg");
    write_jpeg(
        &complete,
        &RgbImage::from_fn(400, 300, |x, y| Rgb([x as u8, y as u8, 90])),
        None,
        None,
    );
    let bytes = std::fs::read(&complete).unwrap();
    std::fs::write(folder.join("truncated.jpg"), &bytes[..bytes.len() / 2]).unwrap();
    let root = temp.path().join("library");

    let library = open(&root);
    let (outcomes, summary) = import(&library, std::slice::from_ref(&folder));
    assert_eq!((summary.added, summary.failed), (1, 2), "{outcomes:?}");
    for (source, outcome) in &outcomes {
        if let ImportOutcome::Failed(message) = outcome {
            assert!(!message.is_empty(), "{source:?}");
        }
    }
    drop(library);

    let reopened = open(&root);
    let failed: Vec<_> = reopened
        .failures()
        .iter()
        .map(|failure| failure.path.file_name().unwrap().to_owned())
        .collect();
    assert_eq!(failed, ["not-a-jpeg.jpg", "truncated.jpg"]);
    solid(&folder.join("not-a-jpeg.jpg"), 60, 40, [90, 30, 10]);
    let (_, summary) = import(&reopened, &[folder.join("not-a-jpeg.jpg")]);
    assert_eq!(summary.added, 1);
    reopened.clear_failures();
    while !matches!(
        reopened.recv_timeout(TIMEOUT).unwrap(),
        Event::FailuresCleared
    ) {}
    drop(reopened);
    assert!(open(&root).failures().is_empty());
}

#[test]
fn exif_orientation_is_applied_to_dimensions_and_derivatives() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("rotated.jpg");
    let exif = exif_block(&[field(Tag::Orientation, Value::Short(vec![6]))]);
    write_jpeg(
        &source,
        &RgbImage::from_pixel(60, 40, Rgb([0, 90, 200])),
        Some(exif),
        None,
    );

    let library = open(&temp.path().join("library"));
    let (outcomes, _) = import(&library, &[source]);
    let ImportOutcome::Added(photo) = &outcomes[0].1 else {
        panic!("{outcomes:?}")
    };
    assert_eq!((photo.width, photo.height), (40, 60));
    let grid = library.derivative_path(&photo.id, DerivativeSize::Grid);
    assert_eq!(image::image_dimensions(grid).unwrap(), (40, 60));
}

#[test]
fn icc_profiles_convert_to_srgb_and_invalid_profiles_fall_back_to_srgb() {
    let display_p3 = std::fs::read("/System/Library/ColorSync/Profiles/Display P3.icc")
        .expect("macOS ships the Display P3 profile");
    let temp = tempfile::tempdir().unwrap();
    let pixels = RgbImage::from_pixel(64, 64, Rgb([40, 180, 60]));
    let untagged = temp.path().join("untagged.jpg");
    let tagged = temp.path().join("display-p3.jpg");
    let invalid = temp.path().join("invalid-profile.jpg");
    write_jpeg(&untagged, &pixels, None, None);
    write_jpeg(&tagged, &pixels, None, Some(display_p3));
    write_jpeg(
        &invalid,
        &pixels,
        None,
        Some(b"not an ICC profile".repeat(8)),
    );

    let library = open(&temp.path().join("library"));
    let (outcomes, summary) = import(&library, &[untagged, tagged, invalid]);
    assert_eq!(summary.added, 3, "{outcomes:?}");
    let pixel = |name: &str| {
        let (_, outcome) = outcomes
            .iter()
            .find(|(source, _)| source.file_name().unwrap() == name)
            .unwrap();
        first_pixel(&library.derivative_path(&added_id(outcome), DerivativeSize::Grid))
    };
    let untagged_pixel = pixel("untagged.jpg");
    let p3_pixel = pixel("display-p3.jpg");
    let distance =
        |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).map(|(a, b)| a.abs_diff(b)).max().unwrap();
    assert!(
        distance(untagged_pixel, p3_pixel) > 8,
        "Display P3 green must become more saturated sRGB: {untagged_pixel:?} vs {p3_pixel:?}"
    );
    assert!(distance(untagged_pixel, pixel("invalid-profile.jpg")) <= 2);
}

#[test]
fn photographic_metadata_is_stored_and_private_tags_never_reach_the_catalog_or_derivatives() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("shot.jpg");
    let exif = exif_block(&[
        field(Tag::Make, Value::Ascii(vec![b"Canon".to_vec()])),
        field(Tag::Model, Value::Ascii(vec![b"Canon EOS R5".to_vec()])),
        field(
            Tag::BodySerialNumber,
            Value::Ascii(vec![b"SERIAL-8675309".to_vec()]),
        ),
        field(
            Tag::LensModel,
            Value::Ascii(vec![b"RF85mm F1.2 L USM".to_vec()]),
        ),
        field(Tag::PhotographicSensitivity, Value::Short(vec![400])),
        field(
            Tag::ExposureTime,
            Value::Rational(vec![Rational { num: 1, denom: 250 }]),
        ),
        field(
            Tag::FNumber,
            Value::Rational(vec![Rational { num: 28, denom: 10 }]),
        ),
        field(
            Tag::FocalLength,
            Value::Rational(vec![Rational { num: 85, denom: 1 }]),
        ),
        field(
            Tag::DateTimeOriginal,
            Value::Ascii(vec![b"2025:09:12 18:04:31".to_vec()]),
        ),
        field(
            Tag::OffsetTimeOriginal,
            Value::Ascii(vec![b"-07:00".to_vec()]),
        ),
        Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![
                Rational { num: 37, denom: 1 },
                Rational { num: 46, denom: 1 },
                Rational { num: 30, denom: 1 },
            ]),
        },
    ]);
    write_jpeg(
        &source,
        &RgbImage::from_pixel(80, 60, Rgb([60, 60, 60])),
        Some(exif),
        None,
    );
    let root = temp.path().join("library");

    let library = open(&root);
    let (outcomes, _) = import(&library, &[source]);
    let id = added_id(&outcomes[0].1);
    drop(library);
    let reopened = open(&root);
    let photo = &reopened.photos()[0];
    let metadata = &photo.metadata;
    assert_eq!(
        metadata.exposure_summary().as_deref(),
        Some("ISO 400 · 1/250 s · f/2.8 · 85 mm")
    );
    assert_eq!(metadata.camera().as_deref(), Some("Canon EOS R5"));
    assert_eq!(metadata.lens.as_deref(), Some("RF85mm F1.2 L USM"));
    assert_eq!(
        metadata
            .captured_at
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("2025-09-12 18:04 (UTC-07:00)")
    );

    drop(reopened);
    for file in std::fs::read_dir(&root).unwrap() {
        let path = file.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes.windows(14).any(|window| window == b"SERIAL-8675309"),
                "Serial number leaked into {path:?}"
            );
        }
    }
    let grid = root.join("derivatives").join(format!("{id}-grid.webp"));
    let bytes = std::fs::read(&grid).unwrap();
    for chunk in [b"EXIF", b"ICCP", b"XMP "] {
        assert!(
            !bytes.windows(4).any(|window| window == chunk),
            "{chunk:?} in derivative"
        );
    }
}

#[test]
fn malformed_metadata_does_not_block_an_import() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("odd.jpg");
    let exif = exif_block(&[
        field(
            Tag::DateTimeOriginal,
            Value::Ascii(vec![b"0000:00:00 00:00:00".to_vec()]),
        ),
        field(
            Tag::FNumber,
            Value::Rational(vec![Rational { num: 0, denom: 0 }]),
        ),
        field(Tag::Orientation, Value::Short(vec![42])),
    ]);
    write_jpeg(
        &source,
        &RgbImage::from_pixel(60, 40, Rgb([1, 2, 3])),
        Some(exif),
        None,
    );

    let library = open(&temp.path().join("library"));
    let (outcomes, _) = import(&library, &[source]);
    let ImportOutcome::Added(photo) = &outcomes[0].1 else {
        panic!("{outcomes:?}")
    };
    assert_eq!((photo.width, photo.height), (60, 40));
    assert_eq!(photo.metadata.captured_at, None);
    assert_eq!(photo.metadata.f_number, None);
}

#[test]
fn a_moved_original_is_flagged_missing_and_reconnected_by_importing_it_again() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("before/photo.jpg");
    let moved = temp.path().join("after/photo.jpg");
    solid(&source, 600, 400, [30, 60, 90]);
    let root = temp.path().join("library");
    let library = open(&root);
    let (outcomes, _) = import(&library, std::slice::from_ref(&source));
    let id = added_id(&outcomes[0].1);
    drop(library);

    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&source, &moved).unwrap();
    let reopened = open(&root);
    assert!(reopened.photos()[0].source_missing);
    assert!(
        reopened
            .derivative_path(&id, DerivativeSize::Grid)
            .is_file()
    );
    reopened.request_preview(&id);
    let message = wait_for_desktop(&reopened, &id).unwrap_err();
    assert!(message.contains("missing"), "{message}");

    let (outcomes, summary) = import(&reopened, std::slice::from_ref(&moved));
    assert_eq!(summary.relinked, 1, "{outcomes:?}");
    reopened.request_preview(&id);
    assert!(wait_for_desktop(&reopened, &id).is_ok());
    drop(reopened);
    let restored = open(&root);
    let photo = &restored.photos()[0];
    assert!(!photo.source_missing);
    assert_eq!(photo.source, moved.canonicalize().unwrap());
}

#[test]
fn replacing_an_originals_content_in_place_replaces_its_photo() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.jpg");
    solid(&source, 60, 40, [30, 60, 90]);
    let library = open(&temp.path().join("library"));
    let (outcomes, _) = import(&library, std::slice::from_ref(&source));
    let first = added_id(&outcomes[0].1);

    std::thread::sleep(Duration::from_millis(20));
    solid(&source, 80, 40, [90, 60, 30]);
    library.import(vec![source]);
    let mut removed = None;
    let mut added = None;
    loop {
        match library.recv_timeout(TIMEOUT).unwrap() {
            Event::Removed { id } => removed = Some(id),
            Event::Imported { outcome, .. } => added = Some(added_id(&outcome)),
            Event::ImportFinished { .. } => break,
            _ => {}
        }
    }
    assert_eq!(removed, Some(first.clone()));
    assert_ne!(added, Some(first.clone()));
    assert_eq!(library.current_photos().unwrap().len(), 1);
    assert!(
        !library
            .derivative_path(&first, DerivativeSize::Grid)
            .exists()
    );
}

#[test]
fn opening_a_photo_waits_for_at_most_one_in_flight_background_import() {
    let temp = tempfile::tempdir().unwrap();
    let small = temp.path().join("small.jpg");
    solid(&small, 600, 400, [20, 90, 130]);
    let library = open(&temp.path().join("library"));
    let (outcomes, _) = import(&library, &[small]);
    let small_id = added_id(&outcomes[0].1);

    let large: Vec<PathBuf> = (0..4)
        .map(|ix| {
            let path = temp.path().join(format!("large-{ix}.jpg"));
            write_jpeg(
                &path,
                &RgbImage::from_fn(4000, 3000, |x, y| {
                    Rgb([ix * 60, (x % 256) as u8, (y % 256) as u8])
                }),
                None,
                None,
            );
            path
        })
        .collect();
    library.import(large);
    library.request_preview(&small_id);
    let mut imports_before_preview = 0;
    loop {
        match library.recv_timeout(TIMEOUT).unwrap() {
            Event::Imported { .. } => imports_before_preview += 1,
            Event::Ready {
                size: DerivativeSize::Desktop,
                id,
                path,
                ..
            } if id == small_id => {
                assert_eq!(image::image_dimensions(path).unwrap(), (600, 400));
                break;
            }
            _ => {}
        }
    }
    assert!(
        imports_before_preview <= 1,
        "{imports_before_preview} imports ran first"
    );
}

#[test]
fn a_repeated_preview_hits_the_desktop_derivative_without_rerendering() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.jpg");
    solid(&source, 3000, 2000, [120, 80, 40]);
    let library = open(&temp.path().join("library"));
    let (outcomes, _) = import(&library, &[source]);
    let id = added_id(&outcomes[0].1);
    for expected_hit in [false, true] {
        library.request_preview(&id);
        loop {
            if let Event::Ready {
                size: DerivativeSize::Desktop,
                cache_hit,
                path,
                ..
            } = library.recv_timeout(TIMEOUT).unwrap()
            {
                assert_eq!(cache_hit, expected_hit);
                assert_eq!(image::image_dimensions(path).unwrap(), (2400, 1600));
                break;
            }
        }
    }
}

#[test]
fn web_sizes_are_prepared_in_the_background_without_upscaling() {
    let temp = tempfile::tempdir().unwrap();
    let wide = temp.path().join("wide.jpg");
    let small = temp.path().join("small.jpg");
    solid(&wide, 3000, 2000, [10, 20, 30]);
    solid(&small, 900, 600, [30, 20, 10]);
    let library =
        Library::open(LibraryOptions::new(temp.path().join("library")).prepare_web_sizes(true))
            .unwrap();
    let (outcomes, _) = import(&library, &[wide.clone(), small]);
    let mut expected = std::collections::HashMap::new();
    for (source, outcome) in &outcomes {
        let id = added_id(outcome);
        let (mobile, desktop) = if source == &wide.canonicalize().unwrap() {
            ((1200, 800), (2400, 1600))
        } else {
            ((900, 600), (900, 600))
        };
        expected.insert((id.clone(), DerivativeSize::Mobile), mobile);
        expected.insert((id, DerivativeSize::Desktop), desktop);
    }
    while !expected.is_empty() {
        if let Event::Ready { id, size, path, .. } = library.recv_timeout(TIMEOUT).unwrap() {
            let dimensions = expected.remove(&(id, size)).expect("unexpected derivative");
            assert_eq!(image::image_dimensions(path).unwrap(), dimensions);
        }
    }
}

#[test]
fn app_owned_copies_of_existing_photos_are_deleted_instead_of_kept() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("photo.jpg");
    solid(&original, 60, 40, [10, 30, 90]);
    let session = temp.path().join("photos-import/session");
    let copy = session.join("copy.jpg");
    std::fs::create_dir_all(&session).unwrap();
    std::fs::copy(&original, &copy).unwrap();

    let library = open(&temp.path().join("library"));
    import(&library, std::slice::from_ref(&original));
    library.import_copies(vec![copy.clone()]);
    loop {
        match library.recv_timeout(TIMEOUT).unwrap() {
            Event::Imported { outcome, .. } => {
                assert!(
                    matches!(outcome, ImportOutcome::Duplicate(_)),
                    "{outcome:?}"
                )
            }
            Event::ImportFinished { .. } => break,
            _ => {}
        }
    }
    assert!(!copy.exists());
    assert!(!session.exists());
    assert!(original.exists());
    assert_eq!(library.current_photos().unwrap()[0].source_count, 1);
}

#[cfg(unix)]
#[test]
fn opening_a_library_rejects_a_different_libvips_version_before_processing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("vips");
    std::fs::write(&helper, "#!/bin/sh\nprintf 'vips-9.0.0\\n'\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let source = temp.path().join("photo.jpg");
    solid(&source, 60, 40, [120, 80, 40]);
    // A separate process isolates PATH and the cached version check.
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_benchmark-pipeline"))
        .arg(temp.path().join("library"))
        .arg(source)
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("expected vips-8.18.3"), "{error}");
    assert!(!temp.path().join("library").exists());
}
