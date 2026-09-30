use photo_contract_client::{Photo, PhotoPage};
use serde_json::{Value, from_value, json, to_value};

#[test]
fn shared_wire_fixtures_round_trip_without_losing_null_or_absence() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../fixtures/contract.json")).unwrap();
    for value in fixtures["validPhotos"].as_array().unwrap() {
        let photo: Photo = from_value(value.clone()).unwrap();
        assert_eq!(to_value(photo).unwrap(), *value);
    }
    for name in ["firstPage", "lastPage"] {
        let page: PhotoPage = from_value(fixtures[name].clone()).unwrap();
        assert_eq!(to_value(page).unwrap(), fixtures[name]);
    }
}

#[test]
fn required_nullable_fields_cannot_be_missing_and_optional_caption_cannot_be_null() {
    assert!(from_value::<Photo>(json!({"id":"x","state":{"_tag":"Draft"}})).is_err());
    assert!(
        from_value::<Photo>(
            json!({"id":"x","capturedAt":null,"caption":null,"state":{"_tag":"Draft"}})
        )
        .is_err()
    );
    assert!(from_value::<PhotoPage>(json!({"items":[]})).is_err());
}

#[test]
fn older_clients_accept_new_object_fields_without_losing_known_fields() {
    let mut value = json!({"id":"x","capturedAt":null,"state":{"_tag":"Draft"}});
    value["futureOptionalField"] = json!("ignored by installed client");
    let photo: Photo = from_value(value).unwrap();
    assert_eq!(photo.id, "x");
}

#[test]
fn optional_film_simulation_rejects_explicit_null_and_non_strings() {
    for film in [json!(null), json!(42), json!({"name":"Classic Chrome"})] {
        assert!(
            from_value::<Photo>(json!({"id":"x","capturedAt":null,
            "state":{"_tag":"Draft"},"filmSimulation":film}))
            .is_err()
        );
    }
}
