//! Uploads, driven through the library the way the window drives it, against an in-process
//! photo API that injects faults.

mod support;

use photo_prototype::{
    Event, Library, LibraryOptions,
    api::{Connection, ObjectKind},
    cloud::{CloudFailure, CloudState},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use support::{Action, FakeApi, Route, TOKEN, flow::*, hex, unavailable};

#[test]
fn an_upload_becomes_a_verified_draft_that_survives_restart() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    assert!(matches!(library.connection(), Connection::Connected { .. }));
    let id = import(&library, &setup.sources)[0].clone();

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert_eq!(
        states.first(),
        Some(&CloudState::Queued),
        "queued first: {states:?}"
    );
    assert!(
        states.iter().any(
            |state| matches!(state, CloudState::Uploading { missing, .. } if missing.len() == 4)
        ),
        "the server's first answer lists every object: {states:?}"
    );
    let CloudState::Draft { cloud_id } = states.last().unwrap() else {
        panic!("expected a draft, got {states:?}");
    };

    let original = std::fs::read(&setup.sources[0]).unwrap();
    assert_eq!(cloud_id.as_str(), hex(&Sha256::digest(&original)));
    assert_eq!(api.drafts(), vec![cloud_id.as_str().to_owned()]);
    let manifest = api.manifest(cloud_id.as_str()).unwrap();
    assert_eq!(manifest.original.bytes, original.len() as u64);
    assert_eq!((manifest.w480.width, manifest.w480.height), (480, 320));
    assert_eq!((manifest.w1200.width, manifest.w1200.height), (900, 600));
    assert_eq!((manifest.w2400.width, manifest.w2400.height), (900, 600));
    assert_eq!(api.puts(), 4, "each object is sent once");
    assert!(
        !setup.root.join("uploads").join(id.as_str()).exists(),
        "the staged copies are deleted once the photo is a draft"
    );
    drop(library);
    assert_eq!(
        stored_state(&setup.root, &api, &id),
        CloudState::Draft {
            cloud_id: cloud_id.clone()
        }
    );
}

#[test]
fn a_restart_mid_upload_resumes_with_what_the_server_still_misses() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(&setup.root, &api, TOKEN, quick_retry(1000));
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Put(ObjectKind::W2400), unavailable(), usize::MAX);
    library.upload(vec![id.clone()]);
    wait_for(
        &library,
        &id,
        |state| matches!(state, CloudState::Uploading { missing, .. } if missing == &[ObjectKind::W2400]),
    );
    drop(library);
    api.clear_faults();

    let library = open(&setup.root, &api);
    let resumed = library
        .photos()
        .iter()
        .find(|photo| photo.id == id)
        .unwrap()
        .cloud
        .clone();
    assert!(
        resumed.is_pending(),
        "the catalog kept the upload: {resumed:?}"
    );
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    assert_eq!(
        api.count(Route::Put(ObjectKind::Original)),
        1,
        "the original is not sent again"
    );
    assert_eq!(api.drafts().len(), 1);
}

#[test]
fn a_lost_answer_to_a_put_does_not_send_the_object_again() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(
        Route::Put(ObjectKind::Original),
        Action::LoseAnswer,
        LOST_BY_THE_CLIENT,
    );

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    assert_eq!(
        api.count(Route::Put(ObjectKind::Original)),
        LOST_BY_THE_CLIENT,
        "the client's own resend only; the app sends it no more"
    );
    assert_eq!(
        api.count(Route::Begin),
        2,
        "the retry asks the server what is left"
    );
    assert_eq!(api.drafts().len(), 1);
}

#[test]
fn a_lost_answer_to_finalize_keeps_the_one_draft() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Finalize, Action::LoseAnswer, LOST_BY_THE_CLIENT);

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    assert_eq!(
        api.count(Route::Finalize),
        LOST_BY_THE_CLIENT,
        "begin reports the draft; the app sends no finalize of its own again"
    );
    assert_eq!(api.puts(), 4, "nothing is sent again");
    assert_eq!(api.drafts().len(), 1);
}

#[test]
fn a_resumed_upload_of_a_photo_the_server_published_is_recorded_as_published() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(&setup.root, &api, TOKEN, quick_retry(1000));
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Put(ObjectKind::W2400), unavailable(), usize::MAX);
    library.upload(vec![id.clone()]);
    let states = wait_for(
        &library,
        &id,
        |state| matches!(state, CloudState::Uploading { missing, .. } if missing == &[ObjectKind::W2400]),
    );
    let cloud_id = states.last().unwrap().cloud_id().unwrap().clone();
    drop(library);
    api.clear_faults();
    // Another run finished the upload and published the photo, and its answers were lost.
    api.publish_directly(cloud_id.as_str());

    let library = open(&setup.root, &api);
    let states = wait_for(&library, &id, is_finished);
    assert_eq!(
        states.last(),
        Some(&CloudState::Published {
            cloud_id: cloud_id.clone()
        }),
        "the state the server reports, never Draft: {states:?}"
    );
    assert!(!states.iter().any(is_draft), "{states:?}");
    assert!(!setup.root.join("uploads").join(id.as_str()).exists());
    drop(library);
    assert_eq!(
        stored_state(&setup.root, &api, &id),
        CloudState::Published { cloud_id }
    );
}

#[test]
fn a_finalize_that_answers_published_is_recorded_as_published() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Finalize, Action::Hold, 1);

    library.upload(vec![id.clone()]);
    until_held(&api);
    let original = std::fs::read(&setup.sources[0]).unwrap();
    api.publish_directly(&hex(&Sha256::digest(&original)));
    api.release();
    let states = wait_for(&library, &id, is_finished);
    assert!(
        is_published(states.last().unwrap()),
        "the state the server reports: {states:?}"
    );
    assert_eq!(api.count(Route::Finalize), 1);
}

#[test]
fn a_prepared_copy_removed_before_it_is_sent_fails_with_a_reason_a_retry_fixes() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Begin, Action::Hold, 1);

    library.upload(vec![id.clone()]);
    until_held(&api);
    std::fs::remove_file(
        setup
            .root
            .join("uploads")
            .join(id.as_str())
            .join("w1200.webp"),
    )
    .unwrap();
    api.release();
    let states = wait_for(&library, &id, is_settled);
    let last = states.last().unwrap();
    assert!(
        matches!(failure(last), CloudFailure::Renditions { .. }),
        "{last:?}"
    );
    assert!(last.can_upload(), "a retry makes the copy again");
    assert_eq!(api.count(Route::Put(ObjectKind::W1200)), 0);

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    assert_eq!(api.drafts().len(), 1);
}

#[test]
fn an_unavailable_api_is_retried_until_it_answers() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Begin, unavailable(), 2);
    api.fault(Route::Put(ObjectKind::W480), unavailable(), 1);

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    assert_eq!(api.drafts().len(), 1);
}

#[test]
fn an_api_that_stays_unavailable_stops_at_the_retry_cap_and_can_be_retried() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(&setup.root, &api, TOKEN, quick_retry(3));
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Begin, unavailable(), usize::MAX);

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    let last = states.last().unwrap();
    assert!(
        matches!(failure(last), CloudFailure::Unavailable { .. }),
        "{last:?}"
    );
    assert!(last.can_upload(), "a retry can help");
    assert_eq!(api.count(Route::Begin), 3, "three tries in all");

    api.clear_faults();
    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(
        is_draft(states.last().unwrap()),
        "the retry finishes: {states:?}"
    );
}

#[test]
fn a_refused_token_stops_at_once_and_says_what_to_do() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = connected(
        &setup.root,
        &api,
        "not-the-owner-token-0123456789abcd",
        quick_retry(4),
    );
    let id = import(&library, &setup.sources)[0].clone();

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    let failure = failure(states.last().unwrap());
    assert_eq!(failure, &CloudFailure::Unauthorized);
    assert!(failure.message().contains("PHOTO_API_TOKEN"));
    assert!(
        !failure.message().contains("not-the-owner-token"),
        "the token stays secret"
    );
    assert_eq!(
        api.calls(),
        vec![Route::List, Route::Begin],
        "the list on open, then one begin: no retry on a refused token"
    );
}

#[test]
fn a_rejected_finalize_fails_without_a_retry() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(
        Route::Finalize,
        Action::Status(
            422,
            json!({"_tag": "FinalizeRejected", "photoId": "x", "problems": [{"_tag": "HasMetadata", "kind": "w1200"}]}),
        ),
        1,
    );

    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    let last = states.last().unwrap();
    let failure = failure(last);
    assert!(
        failure.message().contains("w1200 carries metadata"),
        "{}",
        failure.message()
    );
    assert!(!last.can_upload(), "a retry cannot help");
    assert_eq!(api.count(Route::Finalize), 1);
    assert!(api.drafts().is_empty());
}

#[test]
fn asking_to_upload_a_photo_twice_uploads_it_once() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();

    library.upload(vec![id.clone(), id.clone()]);
    library.upload(vec![id.clone()]);
    let states = wait_for(&library, &id, is_settled);
    assert!(is_draft(states.last().unwrap()), "{states:?}");
    library.upload(vec![id.clone()]);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(api.count(Route::Begin), 1);
    assert_eq!(api.puts(), 4);
    while let Ok(event) = library.try_recv() {
        assert!(
            !matches!(event, Event::CloudChanged { .. }),
            "a draft is not queued again: {event:?}"
        );
    }
}

#[test]
fn an_original_that_changed_or_went_missing_before_upload_is_never_sent() {
    let api = FakeApi::start();
    let setup = setup(2);
    let library = open(&setup.root, &api);
    let ids = import(&library, &setup.sources);
    std::thread::sleep(Duration::from_millis(20));
    photo(&setup.sources[0], 200);
    std::fs::remove_file(&setup.sources[1]).unwrap();

    library.upload(ids.clone());
    let changed = wait_for(&library, &ids[0], is_settled);
    let changed = changed.last().unwrap();
    assert_eq!(failure(changed), &CloudFailure::OriginalChanged);
    assert!(!changed.can_upload());
    let missing = wait_for(&library, &ids[1], is_settled);
    assert_eq!(
        failure(missing.last().unwrap()),
        &CloudFailure::OriginalMissing
    );
    assert!(!api.knows_any_photo(), "the server heard of neither");
}

#[test]
fn an_original_that_changes_after_it_was_prepared_is_not_sent() {
    let api = FakeApi::start();
    let setup = setup(1);
    let library = open(&setup.root, &api);
    let id = import(&library, &setup.sources)[0].clone();
    api.fault(Route::Begin, Action::Hold, 1);

    library.upload(vec![id.clone()]);
    until_held(&api);
    photo(&setup.sources[0], 99);
    api.release();
    let states = wait_for(&library, &id, is_settled);
    assert_eq!(
        failure(states.last().unwrap()),
        &CloudFailure::OriginalChanged
    );
    assert_eq!(api.count(Route::Put(ObjectKind::Original)), 0);
}

#[test]
fn previews_are_served_while_uploads_run() {
    let api = FakeApi::start();
    let setup = setup(3);
    let library = open(&setup.root, &api);
    let ids = import(&library, &setup.sources);
    for kind in ObjectKind::ALL {
        api.fault(
            Route::Put(kind),
            Action::Delay(Duration::from_millis(400)),
            usize::MAX,
        );
    }
    library.upload(ids[..2].to_vec());
    let deadline = Instant::now() + TIMEOUT;
    let mut uploading = false;
    while !uploading {
        if let Ok(Event::CloudChanged {
            state: CloudState::Uploading { .. },
            ..
        }) = library.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            uploading = true;
        }
    }

    let asked = Instant::now();
    library.request_preview(&ids[2]);
    loop {
        match library.recv_timeout(TIMEOUT).expect("preview stalled") {
            Event::Ready {
                id,
                size: photo_prototype::DerivativeSize::Desktop,
                ..
            } if id == ids[2] => break,
            Event::CloudChanged {
                state: CloudState::Draft { .. },
                ..
            } => {
                panic!("the preview waited for an upload to finish")
            }
            _ => {}
        }
    }
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "took {:?}",
        asked.elapsed()
    );
}

#[test]
fn a_photo_in_the_cloud_stays_when_its_original_changes_in_place() {
    let api = FakeApi::start();
    let setup = setup(2);
    let library = open(&setup.root, &api);
    let ids = import(&library, &setup.sources);
    library.upload(vec![ids[0].clone()]);
    let states = wait_for(&library, &ids[0], is_settled);
    let draft = states.last().unwrap().clone();
    assert!(is_draft(&draft));

    std::thread::sleep(Duration::from_millis(20));
    photo(&setup.sources[0], 150);
    photo(&setup.sources[1], 151);
    library.import(setup.sources.clone());
    let (mut detached, mut removed) = (Vec::new(), Vec::new());
    loop {
        match library.recv_timeout(TIMEOUT).unwrap() {
            Event::Detached { photo } => detached.push(photo),
            Event::Removed { id } => removed.push(id),
            Event::ImportFinished { .. } => break,
            _ => {}
        }
    }
    assert_eq!(
        removed,
        vec![ids[1].clone()],
        "a local photo leaves as before"
    );
    assert_eq!(detached.len(), 1, "the uploaded photo stays");
    assert_eq!(detached[0].id, ids[0]);
    assert!(detached[0].source_missing);
    assert_eq!(detached[0].cloud, draft);
    assert_eq!(detached[0].source, setup.sources[0].canonicalize().unwrap());
    let photos = library.current_photos().unwrap();
    assert_eq!(photos.len(), 3, "the draft and the two new photos");
    drop(library);
    assert_eq!(
        stored_state(&setup.root, &api, &ids[0]),
        draft,
        "and after a restart"
    );
}

#[test]
fn without_a_connection_the_library_stays_local() {
    let setup = setup(1);
    let library =
        Library::open(LibraryOptions::new(&setup.root).cloud(Err("Set PHOTO_API_URL".into())))
            .unwrap();
    assert_eq!(
        library.connection(),
        &Connection::NotConnected {
            reason: "Set PHOTO_API_URL".into()
        }
    );
    let id = import(&library, &setup.sources)[0].clone();
    library.upload(vec![id.clone()]);
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = library.try_recv() {
        assert!(!matches!(event, Event::CloudChanged { .. }), "{event:?}");
    }
    assert_eq!(
        library.current_photos().unwrap()[0].cloud,
        CloudState::Local
    );
}
