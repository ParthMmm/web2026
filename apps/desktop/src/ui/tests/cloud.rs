//! Press tests on a library connected to the fake API from `tests/support`: each press runs its
//! call, and the test waits for the answer to land in the window before it reads what the
//! person sees.

#[path = "../../../tests/support/mod.rs"]
mod support;

use super::{press, runtime, settle, shows, text_in};
use crate::{
    Library,
    cloud::{CloudState, CloudStep},
    ui::gallery::Gallery,
};
use rune::{PopupMenuError, Runtime};
use std::time::{Duration, Instant};
use support::{
    FakeApi, Route, TOKEN,
    flow::{self, Setup},
    unavailable,
};

/// How often the test lets the lanes run while it waits for an answer.
const WAIT_STEP: Duration = Duration::from_millis(5);

struct Connected {
    rt: Runtime<Gallery>,
    api: FakeApi,
    cloud_ids: Vec<String>,
    _setup: Setup,
}

/// A window on `count` drafts in the fake API, the first one selected. The first `published`
/// of them are published through the library before the window opens.
fn connected(count: u8, published: usize, attempts: u32) -> Connected {
    let api = FakeApi::start();
    let setup = flow::setup(count);
    let library = flow::connected(&setup.root, &api, TOKEN, flow::quick_retry(attempts));
    let drafts = flow::drafts(&library, &setup);
    for (id, _) in &drafts[..published] {
        library.publish(vec![id.clone()]);
        flow::wait_for(&library, id, flow::is_published);
    }
    let cloud_ids = drafts.into_iter().map(|(_, cloud_id)| cloud_id).collect();
    Connected {
        rt: on(library),
        api,
        cloud_ids,
        _setup: setup,
    }
}

fn on(library: Library) -> Runtime<Gallery> {
    runtime(Gallery::with_library(library))
}

/// Runs frames until the selected photo's state satisfies `done`, or fails with what it shows.
fn until(rt: &mut Runtime<Gallery>, done: impl Fn(&CloudState) -> bool) {
    let deadline = Instant::now() + flow::TIMEOUT;
    while !rt
        .state
        .selected_photo()
        .is_some_and(|photo| done(&photo.record.cloud))
    {
        assert!(
            Instant::now() < deadline,
            "the window shows {:?}: {}",
            rt.state.selected_photo().map(|photo| &photo.record.cloud),
            text_in(rt, "cloud")
        );
        std::thread::sleep(WAIT_STEP);
        rt.frame(0.05);
    }
    settle(rt);
}

fn in_gallery(api: &FakeApi, cloud_id: &str) -> Option<serde_json::Value> {
    api.gallery()
        .into_iter()
        .find(|(id, _)| id == cloud_id)
        .map(|(_, details)| details)
}

#[test]
fn pressing_publish_puts_the_draft_in_the_public_gallery() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 0, 4);
    press(&mut rt, "publish-photo");
    until(&mut rt, flow::is_published);
    let cloud = text_in(&rt, "cloud");
    assert!(cloud.contains("Anyone can see it"), "{cloud}");
    assert!(shows(&rt, "unpublish-photo") && !shows(&rt, "publish-photo"));
    assert!(text_in(&rt, "status").starts_with("Published "));
    let details = in_gallery(&api, &cloud_ids[0]).expect("the photo is public");
    assert_eq!(details["film"], "Classic Chrome", "{details}");
}

#[test]
fn pressing_unpublish_takes_the_photo_out_and_says_it_is_private() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 1, 4);
    assert!(text_in(&rt, "cloud").contains("Anyone can see it"));
    press(&mut rt, "unpublish-photo");
    until(&mut rt, flow::is_draft);
    assert!(text_in(&rt, "cloud").contains("Only you can see it"));
    assert!(shows(&rt, "publish-photo"));
    assert!(text_in(&rt, "status").contains("private draft again"));
    assert_eq!(in_gallery(&api, &cloud_ids[0]), None);
}

#[test]
fn a_publish_that_stops_offers_a_retry_that_finishes_it() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 0, 2);
    api.fault(Route::Publish, unavailable(), 2);
    press(&mut rt, "publish-photo");
    until(&mut rt, |state| matches!(state, CloudState::Failed { .. }));
    assert_eq!(
        rt.state.selected_photo().unwrap().record.cloud.retry(),
        Some(CloudStep::Publish)
    );
    assert!(text_in(&rt, "cloud").contains("didn't answer"));
    assert!(shows(&rt, "cloud-problems"), "the sidebar lists it");
    press(&mut rt, "retry-publish");
    until(&mut rt, flow::is_published);
    assert!(in_gallery(&api, &cloud_ids[0]).is_some());
    assert!(!shows(&rt, "cloud-problems"));
}

#[test]
fn a_description_saved_on_a_published_photo_becomes_its_public_alt_text() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 1, 4);
    press(&mut rt, "description");
    rt.paste("Low tide at  noon,\nthe harbour wall");
    settle(&mut rt);
    assert_eq!(text_in(&rt, "description-count"), "34 / 300");
    press(&mut rt, "save-description");
    let alt = |api: &FakeApi| in_gallery(api, &cloud_ids[0]).map(|details| details["alt"].clone());
    let deadline = Instant::now() + flow::TIMEOUT;
    while alt(&api) != Some("Low tide at noon, the harbour wall".into()) {
        assert!(
            Instant::now() < deadline,
            "the gallery's alt is {:?}",
            alt(&api)
        );
        std::thread::sleep(WAIT_STEP);
        rt.frame(0.05);
    }
    until(&mut rt, flow::is_published);
    assert!(!shows(&rt, "save-description"), "nothing is left to save");
}

#[test]
fn the_tile_menu_publishes_a_draft_and_offers_no_unpublish() {
    let Connected {
        mut rt,
        cloud_ids,
        api,
        ..
    } = connected(1, 0, 4);
    let key = rt.state.photos[0].key.clone();
    let (x, y) = rt.find(&key).expect("the first tile");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
    let menu = rt.popup_menu().expect("a context menu");
    assert!(menu.find("publish").is_some() && menu.find("unpublish").is_some());
    assert!(
        matches!(
            rt.resolve_popup(Some("unpublish")),
            Err(PopupMenuError::Disabled(_))
        ),
        "a draft can't be unpublished"
    );
    rt.resolve_popup(Some("publish")).unwrap();
    until(&mut rt, flow::is_published);
    assert!(in_gallery(&api, &cloud_ids[0]).is_some());
}
