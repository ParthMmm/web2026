use std::{collections::HashMap, path::Path};

use photo_upload_client::{ApiError, Client, ClientError, Kind, Manifest, PhotoView, Problem};

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
        .map(|&kind| Ok((kind, std::fs::read(dir.join(kind.as_str()))?)))
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

    let mut tampered = bodies[&Kind::W480].clone();
    if let Some(last) = tampered.last_mut() {
        *last ^= 0xff;
    }
    expect_error(
        client.put(&photo_id, Kind::W480, &tampered),
        422,
        &ApiError::HashMismatch { kind: Kind::W480 },
    )?;
    let short = &bodies[&Kind::W480][1..];
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
        client.put(&photo_id, kind, &bodies[&kind])?;
    }
    client.put(&photo_id, Kind::Original, &bodies[&Kind::Original])?;
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
        client.put(&photo_id, Kind::W480, &bodies[&Kind::W480]),
        409,
        &ApiError::PhotoAlreadyDraft {
            photo_id: photo_id.clone(),
        },
    )?;
    for _ in 0..10 {
        anyhow::ensure!(client.get(&photo_id)? == draft, "A read changed the draft");
    }
    println!(
        "PASS: 401, 404, idempotent begin, wrong-hash and short uploads rejected, \
         finalize refused while missing, 4 uploads, draft, duplicate finalize, 409 after draft, \
         10 repeated reads"
    );
    Ok(())
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
