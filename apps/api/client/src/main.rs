use photo_contract_client::{ApiError, Client, ClientError, Photo};
use serde_json::{Value, from_value, to_value};

fn main() -> anyhow::Result<()> {
    let base_url = std::env::args().nth(1).ok_or_else(|| {
        anyhow::anyhow!("Usage: photo-contract-client <base-url>; set PROBE_TOKEN")
    })?;
    let token = std::env::var("PROBE_TOKEN")?;
    let client = Client::new(&base_url, &token)?;
    let fixtures: Value = serde_json::from_str(include_str!("../../fixtures/contract.json"))?;

    for value in fixtures["validPhotos"].as_array().unwrap() {
        let photo: Photo = from_value(value.clone())?;
        anyhow::ensure!(
            to_value(client.echo_photo(&photo)?)? == *value,
            "Photo round-trip mismatch"
        );
    }
    let first = client.list_photos(None)?;
    anyhow::ensure!(
        to_value(&first)? == fixtures["firstPage"],
        "First page mismatch"
    );
    let last = client.list_photos(first.next_cursor.as_deref())?;
    anyhow::ensure!(
        to_value(last)? == fixtures["lastPage"],
        "Last page mismatch"
    );
    anyhow::ensure!(
        matches!(client.list_photos(Some("missing")), Err(ClientError::Http { status: 400, error: Some(ApiError::InvalidCursor { cursor }) }) if cursor == "missing"),
        "Expected a structured cursor error"
    );
    let unauthorized = Client::new(&base_url, "deliberately-wrong-test-token")?;
    anyhow::ensure!(
        matches!(
            unauthorized.list_photos(None),
            Err(ClientError::Http {
                status: 401,
                error: Some(ApiError::Unauthorized)
            })
        ),
        "Expected a structured authorization error"
    );
    for value in fixtures["invalidPhotos"].as_array().unwrap() {
        anyhow::ensure!(
            matches!(
                client.echo_json(value),
                Err(ClientError::Http { status: 400, .. })
            ),
            "Expected validation failure"
        );
    }
    // Repeated requests also exercise Worker request-scoped runtimes in one isolate.
    for _ in 0..10 {
        client.list_photos(None)?;
    }
    println!(
        "PASS: 2 photo round-trips, 2 pages, 2 structured errors, 6 validation errors, 10 repeated requests"
    );
    Ok(())
}
