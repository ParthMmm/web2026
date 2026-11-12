use photo_upload_client::{
    ApiError, BeginRequest, GalleryManifest, PhotoDetails, PhotoPage, PhotoView,
};
use serde_json::{Value, from_value, json, to_value};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../fixtures/contract.json")).unwrap()
}

#[test]
fn shared_views_and_requests_round_trip() {
    let fixtures = fixtures();
    for value in fixtures["views"].as_array().unwrap() {
        let view: PhotoView = from_value(value.clone()).unwrap();
        assert_eq!(to_value(view).unwrap(), *value);
    }
    let begin: BeginRequest = from_value(fixtures["begin"].clone()).unwrap();
    assert_eq!(to_value(begin).unwrap(), fixtures["begin"]);
    let publish: PhotoDetails = from_value(fixtures["publish"].clone()).unwrap();
    assert_eq!(to_value(publish).unwrap(), fixtures["publish"]);
    let page: PhotoPage = from_value(fixtures["page"].clone()).unwrap();
    assert_eq!(to_value(page).unwrap(), fixtures["page"]);
}

#[test]
fn shared_gallery_manifest_round_trips() {
    let gallery: Value =
        serde_json::from_str(include_str!("../../fixtures/gallery-v1.json")).unwrap();
    let manifest: GalleryManifest = from_value(gallery.clone()).unwrap();
    assert_eq!(manifest.version, 1);
    assert_eq!(to_value(manifest).unwrap(), gallery);
}

#[test]
fn empty_details_send_an_empty_object() {
    assert_eq!(to_value(PhotoDetails::default()).unwrap(), json!({}));
}

#[test]
fn shared_error_bodies_round_trip() {
    for fixture in fixtures()["errors"].as_array().unwrap() {
        let error: ApiError = from_value(fixture["body"].clone()).unwrap();
        assert_eq!(to_value(error).unwrap(), fixture["body"]);
    }
}

#[test]
fn installed_clients_accept_new_fields_and_reject_unknown_tags() {
    let mut view = fixtures()["views"][2].clone();
    view["publishedAt"] = json!("ignored by installed client");
    assert!(matches!(
        from_value::<PhotoView>(view).unwrap(),
        PhotoView::Draft { .. }
    ));
    assert!(from_value::<PhotoView>(json!({"_tag":"Archived"})).is_err());
}
