fn main() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/contract.json")).unwrap();
    let mut compatible = true;
    for photo in fixtures["validPhotos"].as_array().unwrap() {
        let result = serde_json::from_value::<photo_probe_sdk::models::ProbePhoto>(photo.clone());
        println!("{}: {result:?}", photo["state"]["_tag"]);
        compatible &= result.is_ok();
    }
    if !compatible {
        eprintln!("Generated SDK rejects valid contract fixtures; use the reqwest/serde client.");
        std::process::exit(1);
    }
}
