use std::{collections::HashMap, path::Path};

use bytes::Bytes;

use photo_upload_client::{
    ApiError, Client, ClientError, Gallery, Kind, Manifest, PhotoDetails, PhotoView, Problem,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (Some(base_url), Some(dir)) = (args.next(), args.next()) else {
        anyhow::bail!("Usage: photo-upload-client <base-url> <photo-dir>; set OWNER_TOKEN");
    };
    let base_url = base_url
        .into_string()
        .map_err(|_| anyhow::anyhow!("The base URL is not UTF-8"))?;
    let token = std::env::var("OWNER_TOKEN")?;
    let client = Client::new(&base_url, &token)?;
    let dir = Path::new(&dir);
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    let bodies = Kind::ALL
        .iter()
        .map(|&kind| Ok((kind, Bytes::from(std::fs::read(dir.join(kind.as_str()))?))))
        .collect::<anyhow::Result<HashMap<_, _>>>()?;
    let photo_id = manifest.original.sha256.clone();

    let stranger = Client::new(&base_url, "deliberately-wrong-test-token")?;
    expect_error(stranger.get(&photo_id), 401, &ApiError::Unauthorized)?;
    expect_error(
        client.get(&photo_id),
        404,
        &ApiError::PhotoNotFound {
            photo_id: photo_id.clone(),
        },
    )?;

    let begun = client.begin(&manifest)?;
    anyhow::ensure!(
        matches!(&begun, PhotoView::Uploading { missing, .. } if *missing == Kind::ALL),
        "Expected every object missing after begin, got {begun:?}"
    );
    anyhow::ensure!(client.begin(&manifest)? == begun, "begin is not idempotent");
    expect_error(
        client.publish(&photo_id, &PhotoDetails::default()),
        409,
        &ApiError::PhotoNotDraft {
            photo_id: photo_id.clone(),
        },
    )?;

    let mut tampered = bodies[&Kind::W480].to_vec();
    if let Some(last) = tampered.last_mut() {
        *last ^= 0xff;
    }
    expect_error(
        client.put(&photo_id, Kind::W480, tampered),
        422,
        &ApiError::HashMismatch { kind: Kind::W480 },
    )?;
    let short = bodies[&Kind::W480].slice(1..);
    expect_error(
        client.put(&photo_id, Kind::W480, short),
        422,
        &ApiError::LengthMismatch {
            expected: manifest.w480.bytes,
            kind: Kind::W480,
            received: manifest.w480.bytes - 1,
        },
    )?;
    anyhow::ensure!(
        client.get(&photo_id)? == begun,
        "A rejected upload changed the photo"
    );
    expect_error(
        client.finalize(&photo_id),
        422,
        &ApiError::FinalizeRejected {
            photo_id: photo_id.clone(),
            problems: Kind::ALL
                .iter()
                .map(|&kind| Problem::Missing { kind })
                .collect(),
        },
    )?;

    for kind in Kind::ALL {
        client.put_file(&photo_id, kind, &dir.join(kind.as_str()))?;
    }
    client.put(&photo_id, Kind::Original, bodies[&Kind::Original].clone())?;
    anyhow::ensure!(
        matches!(client.get(&photo_id)?, PhotoView::Uploading { missing, .. } if missing.is_empty()),
        "Expected nothing missing after every upload"
    );

    let draft = client.finalize(&photo_id)?;
    anyhow::ensure!(
        matches!(&draft, PhotoView::Draft { manifest: stored, .. } if *stored == manifest),
        "Expected a draft of the declared manifest, got {draft:?}"
    );
    anyhow::ensure!(
        client.finalize(&photo_id)? == draft,
        "A duplicate finalize changed the draft"
    );
    expect_error(
        client.put(&photo_id, Kind::W480, bodies[&Kind::W480].clone()),
        409,
        &ApiError::PhotoAlreadyDraft {
            photo_id: photo_id.clone(),
        },
    )?;
    for _ in 0..10 {
        anyhow::ensure!(client.get(&photo_id)? == draft, "A read changed the draft");
    }

    let gallery = Gallery::new(&format!("{}/gallery/", base_url.trim_end_matches('/')))?;
    let w480_path = format!("photos/{photo_id}/w480-{}.webp", manifest.w480.sha256);
    let original_paths = [
        format!(
            "photos/{photo_id}/original-{}.webp",
            manifest.original.sha256
        ),
        format!("photos/{photo_id}/original-{}", manifest.original.sha256),
        format!("staging/{photo_id}/original-{}", manifest.original.sha256),
    ];
    anyhow::ensure!(
        gallery.manifest()?.photos.is_empty(),
        "The gallery lists a photo before any publish"
    );
    expect_status(gallery.fetch(&w480_path), 404, "a draft's derivative")?;
    expect_error(
        stranger.publish(&photo_id, &PhotoDetails::default()),
        401,
        &ApiError::Unauthorized,
    )?;

    let details = PhotoDetails {
        alt: Some("Smoke test photo".to_owned()),
        captured_at: Some("2026-10-03T07:12:00".to_owned()),
        film: Some("Portra 400".to_owned()),
    };
    let published = client.publish(&photo_id, &details)?;
    anyhow::ensure!(
        matches!(&published, PhotoView::Published { details: stored, .. } if *stored == details),
        "Expected the published view with its details, got {published:?}"
    );
    anyhow::ensure!(
        client.publish(&photo_id, &details)? == published,
        "A repeated publish changed the photo"
    );
    let page = client.list(None, Some(1))?;
    anyhow::ensure!(
        page.photos == [published.clone()] && page.next.is_none(),
        "Expected one page holding the published photo, got {page:?}"
    );

    let listed = gallery.manifest()?;
    let entry = match listed.photos.as_slice() {
        [entry] if entry.id == photo_id => entry,
        other => anyhow::bail!("Expected the gallery to list only the photo, got {other:?}"),
    };
    anyhow::ensure!(
        entry.sizes.w480.path == w480_path && entry.alt == details.alt,
        "Unexpected gallery entry {entry:?}"
    );
    for size in [&entry.sizes.w480, &entry.sizes.w1200, &entry.sizes.w2400] {
        let kind = [Kind::W480, Kind::W1200, Kind::W2400]
            .into_iter()
            .find(|kind| size.path.contains(&format!("/{}-", kind.as_str())))
            .ok_or_else(|| anyhow::anyhow!("Unknown size path {}", size.path))?;
        anyhow::ensure!(
            gallery.fetch(&size.path)? == bodies[&kind],
            "The public {} is not the uploaded bytes",
            kind.as_str()
        );
    }
    for path in &original_paths {
        expect_status(gallery.fetch(path), 404, "the original")?;
    }

    let unpublished = client.unpublish(&photo_id)?;
    anyhow::ensure!(
        unpublished == draft,
        "Expected the same draft after unpublish, got {unpublished:?}"
    );
    anyhow::ensure!(
        client.unpublish(&photo_id)? == draft,
        "A repeated unpublish changed the draft"
    );
    anyhow::ensure!(
        gallery.manifest()?.photos.is_empty(),
        "The gallery still lists the photo after unpublish"
    );
    for size in [&entry.sizes.w480, &entry.sizes.w1200, &entry.sizes.w2400] {
        expect_status(gallery.fetch(&size.path), 404, "an unpublished derivative")?;
    }
    anyhow::ensure!(
        client.get(&photo_id)? == draft,
        "Unpublish changed the draft"
    );

    println!(
        "PASS: 401, 404, idempotent begin, publish refused while uploading, wrong-hash and short \
         uploads rejected, finalize refused while missing, 4 uploads, draft, duplicate finalize, \
         409 after draft, 10 repeated reads, empty gallery, draft not public, 401 publish, \
         publish twice, list, public manifest and 3 derivatives without a token, original not \
         public, unpublish twice, gallery empty and derivatives gone"
    );
    Ok(())
}

fn expect_status<T: std::fmt::Debug>(
    result: Result<T, ClientError>,
    status: u16,
    what: &str,
) -> anyhow::Result<()> {
    match result {
        Err(ClientError::Http { status: got, .. }) if got == status => Ok(()),
        other => anyhow::bail!("Expected HTTP {status} for {what}, got {other:?}"),
    }
}

fn expect_error<T: std::fmt::Debug>(
    result: Result<T, ClientError>,
    status: u16,
    error: &ApiError,
) -> anyhow::Result<()> {
    match result {
        Err(ClientError::Http {
            status: got,
            error: Some(got_error),
        }) if got == status && got_error == *error => Ok(()),
        other => anyhow::bail!("Expected HTTP {status} {error:?}, got {other:?}"),
    }
}
