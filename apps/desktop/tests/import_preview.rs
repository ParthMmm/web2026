use image::{Rgb, RgbImage};
use photo_prototype::{Event, Library, Pipeline, PreviewKind};
use std::time::Duration;

#[test]
fn import_creates_a_small_preview_without_changing_the_original() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.JPG");
    RgbImage::from_pixel(1200, 800, Rgb([120, 80, 40]))
        .save(&source)
        .unwrap();
    let original = std::fs::read(&source).unwrap();
    let library =
        Library::open(&[source.clone()], temp.path().join("cache"), Pipeline::Rust).unwrap();
    assert_eq!(library.photos().len(), 1);
    match library.recv_timeout(Duration::from_secs(10)).unwrap() {
        Event::Ready { path, .. } => {
            assert_eq!(image::image_dimensions(path).unwrap(), (480, 320));
        }
        event => panic!("Expected a thumbnail: {event:?}"),
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn importing_a_parent_folder_creates_previews_for_all_nested_jpegs() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("imports");
    let sources = [
        folder.join("photo.jpg"),
        folder.join("2025/photo.JPG"),
        folder.join("2025/trip/exports/photo.JPG"),
        folder.join("2024/photo.JpEg"),
    ];
    let mut originals = Vec::new();
    for source in &sources {
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        RgbImage::from_pixel(60, 40, Rgb([120, 80, 40]))
            .save(source)
            .unwrap();
        originals.push(std::fs::read(source).unwrap());
    }
    std::fs::create_dir_all(folder.join("2025/trip/empty")).unwrap();
    std::fs::write(folder.join("2025/trip/notes.txt"), b"not a photo").unwrap();

    let library = Library::open(&[folder], temp.path().join("cache"), Pipeline::Rust).unwrap();
    assert_eq!(library.photos().len(), 4);
    for source in &sources {
        assert!(
            library
                .photos()
                .iter()
                .any(|photo| photo.source == source.canonicalize().unwrap()),
            "Missing nested JPEG: {source:?}"
        );
    }
    let mut ready = std::collections::HashSet::new();
    let mut previews = std::collections::HashSet::new();
    for _ in 0..4 {
        match library.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Ready {
                index,
                kind: PreviewKind::Thumbnail,
                path,
                ..
            } => {
                assert!(index < 4 && ready.insert(index));
                assert_eq!(image::image_dimensions(&path).unwrap(), (60, 40));
                assert!(previews.insert(path), "Each source needs its own preview");
            }
            event => panic!("Expected a nested photo's thumbnail: {event:?}"),
        }
    }
    for (source, original) in sources.iter().zip(originals) {
        assert_eq!(std::fs::read(source).unwrap(), original);
    }
}

#[test]
fn selecting_a_parent_and_its_subfolder_imports_each_source_once() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("imports");
    let subfolder = folder.join("2025/trip");
    std::fs::create_dir_all(&subfolder).unwrap();
    let sources = [folder.join("photo.jpg"), subfolder.join("photo.jpg")];
    for source in &sources {
        RgbImage::from_pixel(60, 40, Rgb([10, 30, 90]))
            .save(source)
            .unwrap();
    }

    let library = Library::open(
        &[folder, subfolder, sources[1].clone()],
        temp.path().join("cache"),
        Pipeline::Rust,
    )
    .unwrap();
    assert_eq!(library.photos().len(), 2);
    for source in sources {
        assert!(
            library
                .photos()
                .iter()
                .any(|photo| photo.source == source.canonicalize().unwrap())
        );
    }
    let mut ready = std::collections::HashSet::new();
    for _ in 0..2 {
        match library.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Ready {
                index,
                kind: PreviewKind::Thumbnail,
                path,
                ..
            } => {
                assert!(index < 2 && ready.insert(index));
                assert_eq!(image::image_dimensions(path).unwrap(), (60, 40));
            }
            event => panic!("Expected a thumbnail: {event:?}"),
        }
    }
}

#[test]
fn reopening_a_folder_reuses_cached_thumbnails_and_reports_bad_files() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("imports");
    std::fs::create_dir(&folder).unwrap();
    RgbImage::from_pixel(60, 40, Rgb([10, 30, 90]))
        .save(folder.join("a.jpg"))
        .unwrap();
    std::fs::write(folder.join("b.jpg"), b"not a JPEG").unwrap();
    std::fs::write(folder.join("notes.txt"), b"not a photo").unwrap();
    let cache = temp.path().join("cache");
    let library = Library::open(&[folder.clone()], cache.clone(), Pipeline::Rust).unwrap();
    assert_eq!(library.photos().len(), 2);
    let thumbnail = match library.recv_timeout(Duration::from_secs(10)).unwrap() {
        Event::Ready { path, .. } => path,
        event => panic!("Expected thumbnail, got {event:?}"),
    };
    assert_eq!(image::image_dimensions(&thumbnail).unwrap(), (60, 40));
    assert!(matches!(
        library.recv_timeout(Duration::from_secs(10)).unwrap(),
        Event::Failed { .. }
    ));
    let modified = std::fs::metadata(&thumbnail).unwrap().modified().unwrap();
    let reopened = Library::open(&[folder], cache, Pipeline::Rust).unwrap();
    assert!(matches!(
        reopened.recv_timeout(Duration::from_secs(10)).unwrap(),
        Event::Ready { .. }
    ));
    assert_eq!(
        std::fs::metadata(thumbnail).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn opening_a_photo_prioritizes_a_large_preview_over_pending_imports() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    RgbImage::from_pixel(2600, 1800, Rgb([20, 90, 130]))
        .save(&source)
        .unwrap();
    let mut inputs = Vec::new();
    for index in 0..6 {
        let path = temp.path().join(format!("{index}.jpg"));
        std::fs::copy(&source, &path).unwrap();
        inputs.push(path);
    }
    let library = Library::open(&inputs, temp.path().join("cache"), Pipeline::Rust).unwrap();
    library.request_preview(5).unwrap();
    let mut found = false;
    for _ in 0..3 {
        if let Event::Ready {
            index: 5,
            kind: PreviewKind::Large,
            path,
            ..
        } = library.recv_timeout(Duration::from_secs(15)).unwrap()
        {
            assert_eq!(image::image_dimensions(path).unwrap(), (2400, 1662));
            found = true;
            break;
        }
    }
    assert!(found, "Preview must not wait for the entire import queue");
    assert!(library.request_preview(999).is_err());
}

#[test]
fn a_small_interactive_preview_does_not_wait_for_an_in_flight_large_import() {
    let temp = tempfile::tempdir().unwrap();
    let large = temp.path().join("a-large.jpg");
    let small = temp.path().join("b-small.jpg");
    RgbImage::from_pixel(8000, 6000, Rgb([20, 90, 130]))
        .save(&large)
        .unwrap();
    RgbImage::from_pixel(60, 40, Rgb([20, 90, 130]))
        .save(&small)
        .unwrap();
    let library =
        Library::open(&[large, small], temp.path().join("cache"), Pipeline::Rust).unwrap();
    assert!(matches!(
        library.recv_timeout(Duration::from_millis(50)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    library.request_preview(1).unwrap();
    match library
        .recv_timeout(Duration::from_secs(1))
        .expect("Preview waited behind a background import")
    {
        Event::Ready {
            index: 1,
            kind: PreviewKind::Large,
            path,
            ..
        } => {
            assert_eq!(image::image_dimensions(path).unwrap(), (60, 40));
        }
        event => panic!("Expected the small interactive preview first: {event:?}"),
    }
}

#[test]
fn warming_a_thumbnail_does_not_warm_the_large_preview() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.jpg");
    RgbImage::from_pixel(60, 40, Rgb([120, 80, 40]))
        .save(&source)
        .unwrap();
    let library = Library::open(&[source], temp.path().join("cache"), Pipeline::Rust).unwrap();
    assert!(matches!(
        library.recv_timeout(Duration::from_secs(10)).unwrap(),
        Event::Ready {
            kind: PreviewKind::Thumbnail,
            cache_hit: false,
            ..
        }
    ));
    for expected_hit in [false, true] {
        library.request_preview(0).unwrap();
        match library.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Ready {
                kind: PreviewKind::Large,
                cache_hit,
                path,
                ..
            } => {
                assert_eq!(cache_hit, expected_hit);
                assert_eq!(image::image_dimensions(path).unwrap(), (60, 40));
            }
            event => panic!("Expected a large preview: {event:?}"),
        }
    }
}

#[cfg(unix)]
#[test]
fn vips_import_rejects_a_different_helper_version_before_processing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("vips");
    std::fs::write(&helper, "#!/bin/sh\nprintf 'vips-9.0.0\\n'\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let source = temp.path().join("photo.jpg");
    RgbImage::from_pixel(60, 40, Rgb([120, 80, 40]))
        .save(&source)
        .unwrap();
    // A separate process isolates PATH and any cached version check from other tests.
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_benchmark-pipeline"))
        .arg("vips")
        .arg(temp.path().join("cache"))
        .arg(source)
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("expected vips-8.18.3"), "{error}");
}

#[test]
fn vips_pipeline_produces_bounded_previews_without_upscaling() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("small.jpg");
    RgbImage::from_pixel(60, 40, Rgb([120, 80, 40]))
        .save(&source)
        .unwrap();
    let library = Library::open(&[source], temp.path().join("cache"), Pipeline::Vips).unwrap();
    match library.recv_timeout(Duration::from_secs(10)).unwrap() {
        Event::Ready { path, .. } => assert_eq!(image::image_dimensions(path).unwrap(), (60, 40)),
        event => panic!("Expected libvips preview: {event:?}"),
    }
}
