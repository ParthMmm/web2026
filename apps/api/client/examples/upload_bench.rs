//! Times repeated uploads of one of a photo's objects against a running Worker.
//!
//! Usage: OWNER_TOKEN=... cargo run --release --example upload_bench -- <base-url> <photo-dir> <uploads> [kind]
//!
//! Every tenth upload first sends a body one byte short, which the Worker refuses before it reads
//! the body. The correct upload that follows must still succeed.

use std::{path::Path, time::Instant};

use bytes::Bytes;

use photo_upload_client::{ApiError, Client, ClientError, Kind, Manifest};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(base_url), Some(dir), Some(uploads)) = (args.next(), args.next(), args.next()) else {
        anyhow::bail!("Usage: upload_bench <base-url> <photo-dir> <uploads>; set OWNER_TOKEN");
    };
    let uploads: u32 = uploads.parse()?;
    let kind = match args.next().as_deref() {
        None | Some("original") => Kind::Original,
        Some("w480") => Kind::W480,
        Some(other) => anyhow::bail!("Unknown kind {other}; use original or w480"),
    };
    let client = Client::new(&base_url, &std::env::var("OWNER_TOKEN")?)?;
    let dir = Path::new(&dir);
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    let body = Bytes::from(std::fs::read(dir.join(kind.as_str()))?);
    let photo_id = manifest.original.sha256.clone();
    client.begin(&manifest)?;

    let mut refused = 0;
    let started = Instant::now();
    for upload in 0..uploads {
        if upload % 10 == 0 {
            match client.put(&photo_id, kind, body.slice(1..)) {
                Err(ClientError::Http {
                    status: 422,
                    error: Some(ApiError::LengthMismatch { .. }),
                }) => refused += 1,
                other => anyhow::bail!("Expected a refused short upload, got {other:?}"),
            }
        }
        client.put(&photo_id, kind, body.clone())?;
    }
    let elapsed = started.elapsed();
    println!(
        "{uploads} uploads of {} bytes and {refused} refused uploads in {:.0} ms ({:.1} ms each)",
        body.len(),
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / f64::from(uploads),
    );
    Ok(())
}
