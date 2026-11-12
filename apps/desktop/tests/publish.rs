//! Publishing and unpublishing, driven through the library the way the window drives it,
//! against an in-process photo API that injects faults.

mod support;

use photo_prototype::{
    Event, PhotoId,
    cloud::{CloudFailure, CloudId, CloudState, CloudStep},
};
use serde_json::json;
use support::{Action, FakeApi, Route, TOKEN, flow::*, unavailable};

fn is_public(state: &CloudState) -> bool {
    matches!(
        state,
        CloudState::Published { .. } | CloudState::Failed { .. }
    )
}

fn is_private(state: &CloudState) -> bool {
    matches!(state, CloudState::Draft { .. } | CloudState::Failed { .. })
}

fn published(cloud_id: &str) -> CloudState {
    CloudState::Published {
        cloud_id: CloudId::from_hex(cloud_id),
    }
}

fn draft(cloud_id: &str) -> CloudState {
    CloudState::Draft {
        cloud_id: CloudId::from_hex(cloud_id),
    }
}

/// Publishes `id` and waits until the server's answer lands.
fn publish(library: &photo_prototype::Library, id: &PhotoId) -> Vec<CloudState> {
    library.publish(vec![id.clone()]);
    wait_for(library, id, is_public)
}

/// The description lands in the catalog before anything else happens to the photo.
fn describe(library: &photo_prototype::Library, id: &PhotoId, text: &str) {
    library.describe(id.clone(), text.into());
    loop {
        match library
            .recv_timeout(TIMEOUT)
            .expect("the description never landed")
        {
            Event::MetadataUpdated { photo } if &photo.id == id => {
                assert_eq!(photo.description.as_deref(), Some(text));
                return;
            }
            _ => {}
        }
    }
}

#[test]
fn publishing_a_draft_makes_it_public_with_its_details_and_unpublishing_takes_it_back() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    describe(&library, &id, "The harbour wall at low tide");
    assert_eq!(
        api.count(Route::Publish),
        0,
        "a draft's description is only kept"
    );

    let states = publish(&library, &id);
    assert_eq!(
        states.first(),
        Some(&CloudState::Publishing {
            cloud_id: CloudId::from_hex(&cloud_id)
        }),
        "the catalog says Publishing first: {states:?}"
    );
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(
        api.gallery(),
        vec![(
            cloud_id.clone(),
            json!({"alt": "The harbour wall at low tide", "capturedAt": "2025-09-12T18:04:31", "film": "Classic Chrome"})
        )],
        "the details come from the catalog, and nothing else from EXIF"
    );

    library.unpublish(vec![id.clone()]);
    let states = wait_for(&library, &id, is_private);
    assert!(
        matches!(states.first(), Some(CloudState::Unpublishing { .. })),
        "{states:?}"
    );
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    assert!(api.gallery().is_empty());
    drop(library);
    assert_eq!(stored_state(&setup.root, &api, &id), draft(&cloud_id));
}

#[test]
fn a_lost_answer_to_publish_still_ends_published_once() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    api.fault(Route::Publish, Action::LoseAnswer, LOST_BY_THE_CLIENT);

    let states = publish(&library, &id);
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(
        api.count(Route::Publish),
        LOST_BY_THE_CLIENT + 1,
        "the app asks again, and the server answers the photo it already published"
    );
    assert_eq!(api.gallery().len(), 1);
}

#[test]
fn a_lost_answer_to_unpublish_still_ends_a_draft() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    publish(&library, &id);
    api.fault(Route::Unpublish, Action::LoseAnswer, LOST_BY_THE_CLIENT);

    library.unpublish(vec![id.clone()]);
    let states = wait_for(&library, &id, is_private);
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    assert_eq!(api.count(Route::Unpublish), LOST_BY_THE_CLIENT + 1);
    assert!(api.gallery().is_empty());
}

#[test]
fn an_unpublish_that_stops_midway_finishes_on_the_next_try() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    publish(&library, &id);
    api.fault(Route::Unpublish, Action::Midway, 1);

    library.unpublish(vec![id.clone()]);
    let states = wait_for(&library, &id, is_private);
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    assert_eq!(
        states
            .iter()
            .filter(|state| **state == draft(&cloud_id))
            .count(),
        1,
        "Draft only at the end: {states:?}"
    );
    assert_eq!(api.count(Route::Unpublish), 2);
    assert!(api.gallery().is_empty());
}

#[test]
fn an_unpublish_that_keeps_stopping_midway_fails_without_showing_a_draft_and_a_retry_finishes_it() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(&setup.root, &api, TOKEN, quick_retry(2));
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    publish(&library, &id);
    api.fault(Route::Unpublish, Action::Midway, usize::MAX);

    library.unpublish(vec![id.clone()]);
    let states = wait_for(&library, &id, is_private);
    let last = states.last().unwrap();
    let CloudState::Failed { failure, step, .. } = last else {
        panic!("expected a failure, got {states:?}");
    };
    assert_eq!(*step, CloudStep::Unpublish);
    assert!(
        matches!(failure, CloudFailure::Unavailable { .. }),
        "{failure:?}"
    );
    assert!(last.can_unpublish(), "a retry can finish it");
    assert!(!last.can_publish());
    assert_eq!(api.stage(&cloud_id).as_deref(), Some("Unpublishing"));
    assert!(!states.contains(&draft(&cloud_id)), "{states:?}");

    api.clear_faults();
    library.unpublish(vec![id.clone()]);
    let states = wait_for(&library, &id, is_private);
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    assert!(api.gallery().is_empty());
}

#[test]
fn a_restart_during_publishing_resumes_and_takes_the_server_s_answer() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    api.fault(Route::Publish, Action::Hold, 1);

    library.publish(vec![id.clone()]);
    until_held(&api);
    drop(library);
    assert_eq!(
        stored_state(&setup.root, &api, &id),
        CloudState::Publishing {
            cloud_id: CloudId::from_hex(&cloud_id)
        },
        "the catalog held Publishing before the call"
    );
    api.release();

    let library = open(&setup.root, &api);
    let states = wait_for(&library, &id, is_public);
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(api.gallery().len(), 1);
    drop(library);
    assert_eq!(stored_state(&setup.root, &api, &id), published(&cloud_id));
}

#[test]
fn a_restart_while_the_server_is_unpublishing_never_shows_a_draft_and_finishes_it() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(&setup.root, &api, TOKEN, quick_retry(1000));
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    publish(&library, &id);
    api.fault(Route::Unpublish, Action::Midway, usize::MAX);

    library.unpublish(vec![id.clone()]);
    let deadline = std::time::Instant::now() + TIMEOUT;
    while api.count(Route::Unpublish) < 2 {
        assert!(std::time::Instant::now() < deadline, "unpublish never ran");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(library);
    assert_eq!(api.stage(&cloud_id).as_deref(), Some("Unpublishing"));
    assert_eq!(
        stored_state(&setup.root, &api, &id),
        CloudState::Unpublishing {
            cloud_id: CloudId::from_hex(&cloud_id)
        }
    );
    api.clear_faults();

    let library = open(&setup.root, &api);
    let states = wait_for(&library, &id, is_private);
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    assert!(api.gallery().is_empty());
}

#[test]
fn publishing_a_photo_the_server_holds_as_an_upload_is_refused_and_an_upload_retry_fixes_it() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    api.reopen(&cloud_id);

    let states = publish(&library, &id);
    let last = states.last().unwrap();
    let CloudState::Failed { failure, step, .. } = last else {
        panic!("expected a failure, got {states:?}");
    };
    assert_eq!(
        (failure, *step),
        (&CloudFailure::NotDraft, CloudStep::Publish)
    );
    assert!(
        failure.message().contains("upload"),
        "{}",
        failure.message()
    );
    assert_eq!(
        api.count(Route::Publish),
        1,
        "a retry of the publish can't help"
    );
    assert!(last.can_upload() && !last.can_publish());
    assert!(api.gallery().is_empty());

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert_eq!(states.last(), Some(&draft(&cloud_id)), "{states:?}");
    let states = publish(&library, &id);
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
}

#[test]
fn a_refused_token_stops_a_publish_and_says_what_to_do() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, _) = drafts(&library, &setup).remove(0);
    drop(library);

    let library = connected(
        &setup.root,
        &api,
        "not-the-owner-token-0123456789abcd",
        quick_retry(4),
    );
    let before = api.count(Route::Publish);
    let states = publish(&library, &id);
    let CloudState::Failed { failure, step, .. } = states.last().unwrap() else {
        panic!("expected a failure, got {states:?}");
    };
    assert_eq!(
        (failure, *step),
        (&CloudFailure::Unauthorized, CloudStep::Publish)
    );
    assert!(failure.message().contains("PHOTO_API_TOKEN"));
    assert_eq!(
        api.count(Route::Publish) - before,
        1,
        "no retry on a refused token"
    );
    assert!(api.gallery().is_empty());
}

#[test]
fn opening_corrects_local_states_the_server_disagrees_with() {
    let api = FakeApi::start();
    let setup = setup(4);
    let library = open(&setup.root, &api);
    let photos = drafts(&library, &setup);
    for (id, _) in &photos[1..=2] {
        publish(&library, id);
    }
    drop(library);
    // Answers another run never heard: one published, one unpublished, one stopped midway,
    // and one the server forgot.
    api.publish_directly(&photos[0].1);
    api.unpublish_directly(&photos[1].1);
    api.fault(Route::Unpublish, Action::Midway, 1);
    let midway = reqwest_unpublish(&api, &photos[2].1);
    assert_eq!(midway, 503);
    api.forget(&photos[3].1);
    let calls_before = api.calls().len();

    let library = open(&setup.root, &api);
    let ids: Vec<PhotoId> = photos.iter().map(|(id, _)| id.clone()).collect();
    let states = wait_all(&library, &ids, |state| {
        !matches!(state, CloudState::Unpublishing { .. })
    });
    assert_eq!(
        states,
        vec![
            published(&photos[0].1),
            draft(&photos[1].1),
            draft(&photos[2].1),
            CloudState::Local,
        ]
    );
    let calls: Vec<Route> = api.calls()[calls_before..].to_vec();
    assert_eq!(
        calls,
        vec![Route::List, Route::Unpublish],
        "one list, then only the unpublish the server had started"
    );
    drop(library);
    assert_eq!(
        stored_state(&setup.root, &api, &photos[0].0),
        published(&photos[0].1)
    );
}

/// Sends an unpublish as another run would, outside the library under test.
fn reqwest_unpublish(api: &FakeApi, cloud_id: &str) -> u16 {
    let client = photo_upload_client::Client::new(api.address(), TOKEN).unwrap();
    match client.unpublish(cloud_id) {
        Ok(_) => 200,
        Err(photo_upload_client::ClientError::Http { status, .. }) => status,
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn a_changed_description_republishes_a_published_photo() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    describe(&library, &id, "First words");
    publish(&library, &id);

    library.describe(id.clone(), "  Better words  ".into());
    let states = wait_for(&library, &id, is_public);
    assert!(
        matches!(states.first(), Some(CloudState::Publishing { .. })),
        "{states:?}"
    );
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(api.count(Route::Publish), 2);
    assert_eq!(api.gallery()[0].1["alt"], json!("Better words"));

    library.describe(id.clone(), "   ".into());
    let states = wait_for(&library, &id, is_public);
    assert_eq!(states.last(), Some(&published(&cloud_id)));
    assert_eq!(
        api.gallery()[0].1.get("alt"),
        None,
        "an empty description sends no alt"
    );
}

#[test]
fn an_unavailable_api_during_publish_is_retried_until_it_answers() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    api.fault(Route::Publish, unavailable(), 2);

    let states = publish(&library, &id);
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(api.count(Route::Publish), 3);
}

#[test]
fn a_description_saved_while_its_publish_is_in_flight_reaches_the_gallery() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let (id, cloud_id) = drafts(&library, &setup).remove(0);
    describe(&library, &id, "First words");
    api.fault(Route::Publish, Action::Hold, 1);
    library.publish(vec![id.clone()]);
    until_held(&api);

    describe(&library, &id, "Later words");
    api.release();
    let states = wait_for(&library, &id, is_public);
    assert_eq!(states.last(), Some(&published(&cloud_id)), "{states:?}");
    assert_eq!(
        api.count(Route::Publish),
        2,
        "sent again with the later words"
    );
    assert_eq!(api.gallery()[0].1["alt"], json!("Later words"));
}
